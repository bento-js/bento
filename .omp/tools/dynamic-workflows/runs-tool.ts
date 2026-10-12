// The `workflow_runs` tool: list background runs, read one run's live snapshot,
// block until a run settles, or cancel a run.
import type { ExtensionAPI, ToolDefinition } from "@oh-my-pi/pi-coding-agent";

import type { WorkflowSnapshot } from "./display.js";
import type { WorkflowRun, WorkflowRunManager, WorkflowRunStatus } from "./runs.js";

import { formatWorkflowResult, recomputeWorkflowSnapshot, renderWorkflowText } from "./display.js";
import { runHeadline } from "./runs.js";

const DEFAULT_WAIT_SECONDS = 300;

const RUNS_DESCRIPTION = [
    "Inspect, await, or cancel workflow runs started by the workflow tool.",
    "list: every run of this session with its status.",
    "status: one run's phases, agent rows with their agent:// ids, logs, and result when it has one.",
    "wait: block until that run settles (or any live run settles when run is omitted), until timeoutSeconds, or until you are interrupted. Use it instead of polling status.",
    "cancel: abort a live run; its agents end skipped and a workflow-result message reports the cancellation.",
    "run defaults to the most recent run.",
].join(" ");

export interface WorkflowRunsToolDetails {
    action: "list" | "status" | "wait" | "cancel";
    runs?: Array<{ runId: string; name: string; status: WorkflowRunStatus; agentCount: number }>;
    runId?: string;
    runStatus?: WorkflowRunStatus;
    snapshot?: WorkflowSnapshot;
    cancelled?: boolean;
    /** How a `wait` ended. */
    outcome?: "settled" | "timeout" | "interrupted";
}

export function createWorkflowRunsTool(
    pi: ExtensionAPI,
    manager: WorkflowRunManager,
): ToolDefinition<any, WorkflowRunsToolDetails> {
    return {
        name: "workflow_runs",
        label: "Workflow Runs",
        loadMode: "essential",
        description: RUNS_DESCRIPTION,
        approval: "read",
        parameters: pi.zod
            .object({
                action: pi.zod
                    .enum(["list", "status", "wait", "cancel"])
                    .describe(
                        "list runs, read one run's snapshot, block until a run settles, or cancel a live run.",
                    ),
                run: pi.zod.string().optional().describe('Run id, e.g. "wf-1".'),
                timeoutSeconds: pi.zod
                    .number()
                    .int()
                    .min(1)
                    .max(3600)
                    .optional()
                    .describe(`How long "wait" may block; defaults to ${DEFAULT_WAIT_SECONDS}.`),
            })
            .strict(),
        async execute(_toolCallId, params, signal) {
            if (params.action === "list") return listResult(manager);
            // Waiting without a run id watches every live run, so a coordinator that
            // started several can block once instead of picking one.
            if (params.action === "wait" && !params.run) {
                return await waitForRuns(manager.live(), params.timeoutSeconds, signal);
            }

            let run = params.run ? manager.get(params.run) : manager.latest();
            if (!run) {
                return {
                    isError: true,
                    content: [
                        {
                            type: "text" as const,
                            text: params.run
                                ? `No workflow run ${params.run}. Use action "list" to see the runs of this session.`
                                : "No workflow has run in this session yet.",
                        },
                    ],
                    details: { action: params.action },
                };
            }
            if (params.action === "cancel") return cancelResult(run, manager.cancel(run.id));
            if (params.action === "wait") {
                return await waitForRuns([run], params.timeoutSeconds, signal);
            }
            return statusResult(run);
        },
        renderCall(args, _options, theme) {
            let target = args.run ? ` ${args.run}` : "";
            return new pi.pi.Text(
                theme.fg("toolTitle", theme.bold(`workflow_runs ${args.action}${target}`)),
                0,
                0,
            );
        },
        renderResult(result, _options, theme) {
            let first = result.content[0];
            let text = first?.type === "text" ? first.text : "workflow_runs";
            return new pi.pi.Text(result.isError ? theme.fg("error", text) : text, 0, 0);
        },
    };
}

function listResult(manager: WorkflowRunManager) {
    let runs = manager.list();
    let rows = runs.map(run => {
        let snapshot = recomputeWorkflowSnapshot(run.snapshot);
        return {
            runId: run.id,
            name: run.name,
            status: run.status,
            agentCount: snapshot.agentCount,
            line: `${run.id}  ${run.status.padEnd(9)} ${run.name} — ${snapshot.doneCount}/${snapshot.agentCount} agents done`,
        };
    });
    return {
        content: [
            {
                type: "text" as const,
                text: rows.length
                    ? ["Workflow runs:", ...rows.map(row => `  ${row.line}`)].join("\n")
                    : "No workflow has run in this session yet.",
            },
        ],
        details: {
            action: "list" as const,
            runs: rows.map(({ runId, name, status, agentCount }) => ({
                runId,
                name,
                status,
                agentCount,
            })),
        },
    };
}

/**
 * Block until one of `runs` settles, the timeout elapses, or the caller is
 * interrupted. Every branch awaits a promise — the run's own completion, one
 * timer, the abort signal — so waiting costs nothing while it blocks.
 */
async function waitForRuns(
    runs: WorkflowRun[],
    timeoutSeconds: number | undefined,
    signal: AbortSignal | undefined,
) {
    if (runs.length === 0) {
        return {
            content: [
                {
                    type: "text" as const,
                    text: 'No workflow run is live, so there is nothing to wait for. Use action "list" to see this session\'s runs.',
                },
            ],
            details: { action: "wait" as const, outcome: "settled" as const },
        };
    }

    let settled = Promise.race(runs.map(async run => (await run.finished, run)));
    let deadline = Promise.withResolvers<"timeout">();
    let interrupt = Promise.withResolvers<"interrupted">();
    let seconds = timeoutSeconds ?? DEFAULT_WAIT_SECONDS;
    let timer = setTimeout(() => deadline.resolve("timeout"), seconds * 1000);
    let abort = () => interrupt.resolve("interrupted");
    signal?.addEventListener("abort", abort, { once: true });
    let outcome: WorkflowRun | "timeout" | "interrupted";
    try {
        outcome = await Promise.race([settled, deadline.promise, interrupt.promise]);
    } finally {
        clearTimeout(timer);
        signal?.removeEventListener("abort", abort);
    }

    if (outcome === "timeout" || outcome === "interrupted") {
        let waited = runs.map(run => `${run.id} (${run.status})`).join(", ");
        return {
            content: [
                {
                    type: "text" as const,
                    text:
                        outcome === "timeout"
                            ? `Still running after ${seconds}s: ${waited}. Wait again or use action "status".`
                            : `Wait interrupted; still running: ${waited}.`,
                },
            ],
            details: { action: "wait" as const, outcome },
        };
    }
    let status = statusResult(outcome);
    return {
        content: status.content,
        details: { ...status.details, action: "wait" as const, outcome: "settled" as const },
    };
}

function statusResult(run: WorkflowRun) {
    let snapshot = recomputeWorkflowSnapshot(run.snapshot);
    let sections = [
        runHeadline(run, snapshot),
        renderWorkflowText(snapshot, run.status !== "running"),
    ];
    if (run.status === "done")
        sections.push(`Result:\n${formatWorkflowResult(run.outcome?.result)}`);
    if (run.status === "failed") sections.push(`Error: ${run.error ?? "unknown failure"}`);
    return {
        content: [{ type: "text" as const, text: sections.join("\n\n") }],
        details: {
            action: "status" as const,
            runId: run.id,
            runStatus: run.status,
            snapshot,
        },
    };
}

function cancelResult(run: WorkflowRun, cancelled: boolean) {
    return {
        content: [
            {
                type: "text" as const,
                text: cancelled
                    ? `Cancelling ${run.id} (${run.name}). Its agents end skipped and a workflow-result message reports the cancellation.`
                    : `Run ${run.id} (${run.name}) already ${run.status}; nothing to cancel.`,
            },
        ],
        details: {
            action: "cancel" as const,
            runId: run.id,
            runStatus: run.status,
            cancelled,
        },
    };
}
