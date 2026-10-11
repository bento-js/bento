// Ported from pi-dynamic-workflows v1.0.1, commit 31b2aca0f1cb195aafbfc5e3ee2b8c83ad3f21a2.
// The `workflow` tool: parses the script, starts a run, and either reports the
// run id (background) or waits for the result (blocking).
import type { ExtensionAPI, ToolDefinition } from "@oh-my-pi/pi-coding-agent";

import { AgentOutputManager } from "@oh-my-pi/pi-coding-agent/task";

import type { OmpWorkflowAgentDependencies, WorkflowSessionContext } from "./agent.js";
import type { WorkflowSnapshot } from "./display.js";
import type { WorkflowRun, WorkflowRunManager, WorkflowRunStatus } from "./runs.js";

import { OmpWorkflowAgent } from "./agent.js";
import { formatWorkflowResult, recomputeWorkflowSnapshot, renderWorkflowText } from "./display.js";
import { parseWorkflowScript } from "./workflow.js";

const WORKFLOW_DESCRIPTION = [
    "Execute a deterministic JavaScript workflow that orchestrates OMP subagents.",
    "The first statement must export literal meta with non-empty name and description.",
    "Globals: agent, parallel, pipeline, phase, log, args, cwd, process.cwd, and budget.",
    "parallel receives zero-argument functions, not promises.",
    "agent options are agent, model, label, and schema; every agent needs a short unique label.",
    "Each label becomes the agent's id (runtime-core -> RuntimeCore), reachable with write agent://<id>.",
    "Failed branches return null unless the workflow is aborted.",
    "Use a final synthesis agent when several branches feed one conclusion.",
    "Runs in the background by default: the call returns a run id at once and the result arrives later as a workflow-result message, so keep working meanwhile.",
    "Pass background false to wait for the result instead; use workflow_runs to list, inspect, or cancel runs.",
].join(" ");

// Strip exactly one outer Markdown code fence when the entire script is wrapped in
// it; a fence that only wraps part of the input means the model sent prose around
// the script, which the deterministic runtime cannot parse.
const OUTER_FENCE = /^```[^\n]*\r?\n([\s\S]*?)\r?\n?```$/;

export interface WorkflowToolOptions {
    manager: WorkflowRunManager;
    /**
     * Settings for the spawned subagents; the extension context carries none.
     * Resolved per call, since the global settings handle is not ready while
     * extensions load.
     */
    settings?: () => WorkflowSessionContext["settings"];
    dependencies?: Partial<OmpWorkflowAgentDependencies>;
}

/** Tool result details: the run's snapshot plus the run it belongs to. */
export interface WorkflowToolDetails extends WorkflowSnapshot {
    runId?: string;
    runStatus?: WorkflowRunStatus;
}

export function createWorkflowTool(
    pi: ExtensionAPI,
    options: WorkflowToolOptions,
): ToolDefinition<any, WorkflowToolDetails> {
    let manager = options.manager;
    // One allocator per session: concurrent runs drawing from separate allocators
    // could hand two live agents the same registry id.
    let outputManager: AgentOutputManager | undefined;

    return {
        name: "workflow",
        label: "Workflow",
        loadMode: "essential",
        description: WORKFLOW_DESCRIPTION,
        parameters: pi.zod
            .object({
                script: pi.zod.string().describe("Raw JavaScript workflow script without prose."),
                args: pi.zod.unknown().optional(),
                background: pi.zod
                    .boolean()
                    .optional()
                    .describe(
                        "Default true: return a run id immediately and deliver the result as a message. False waits for the result.",
                    ),
                concurrency: pi.zod
                    .number()
                    .int()
                    .min(1)
                    .max(16)
                    .optional()
                    .describe("Maximum agents running at once, 1 to 16."),
            })
            .strict(),
        async execute(toolCallId, params, signal, onUpdate, ctx) {
            let program = parseWorkflowScript(normalizeWorkflowScript(params.script));
            let background = params.background ?? true;
            let context: WorkflowSessionContext = {
                sessionManager: ctx.sessionManager,
                modelRegistry: ctx.modelRegistry,
                settings: options.settings?.(),
                localProtocolOptions: ctx.localProtocolOptions,
            };
            outputManager ??= new AgentOutputManager(() => ctx.sessionManager.getArtifactsDir());
            let allocator = outputManager;

            let runner: OmpWorkflowAgent | undefined;
            let run = manager.start({
                program,
                cwd: ctx.cwd,
                args: params.args,
                concurrency: params.concurrency,
                coordinatorId: ctx.agent.id,
                background,
                createRunner: coordination =>
                    (runner = new OmpWorkflowAgent({
                        cwd: ctx.cwd,
                        context,
                        parentToolCallId: toolCallId,
                        dependencies: options.dependencies,
                        outputManager: allocator,
                        coordination,
                    })),
                // A background call has already returned by the time frames arrive,
                // so only a blocking caller streams them.
                onFrame: background
                    ? undefined
                    : (frame, complete) =>
                          onUpdate?.({
                              content: [
                                  { type: "text", text: renderWorkflowText(frame, complete) },
                              ],
                              details: frame,
                          }),
            });

            if (background) {
                // The script queues its first wave synchronously; letting those ids
                // land names them in the result without waiting for any agent.
                await runner?.settleReservations();
                return startedResult(run);
            }
            return await blockingResult(run, manager, signal);
        },
        renderCall(_args, _options, theme) {
            return new pi.pi.Text(theme.fg("toolTitle", theme.bold("workflow")), 0, 0);
        },
        renderResult(result, { isPartial }, theme) {
            let details = result.details;
            let first = result.content[0];
            let text = first?.type === "text" ? first.text : undefined;
            // A background start has no snapshot worth rendering yet; its text names
            // the run and the agents that are queued.
            if (details?.runStatus === "running" && text) return new pi.pi.Text(text, 0, 0);
            if (details?.name) {
                let body = renderWorkflowText(details, !isPartial);
                // An aborted run resolves as an error result carrying the final skipped
                // snapshot; surface the abort line beneath it so both stay visible.
                if (result.isError) {
                    return new pi.pi.Text(
                        `${body}\n${theme.fg("error", text ?? "Workflow was aborted")}`,
                        0,
                        0,
                    );
                }
                return new pi.pi.Text(body, 0, 0);
            }
            return new pi.pi.Text(text ?? theme.fg("muted", "workflow"), 0, 0);
        },
    };
}

function normalizeWorkflowScript(script: string): string {
    let trimmed = script.trim();
    let fenced = OUTER_FENCE.exec(trimmed);
    if (fenced) return fenced[1];
    if (/^```/m.test(trimmed)) {
        throw new Error(
            "Workflow script must be raw JavaScript or a single fenced code block with no surrounding prose.",
        );
    }
    return trimmed;
}

function startedResult(run: WorkflowRun) {
    let snapshot = recomputeWorkflowSnapshot(run.snapshot);
    let agents = snapshot.agents
        .map(agent => `${agent.agentId ?? "(id pending)"} (${agent.label})`)
        .join(", ");
    let lines = [
        `Workflow ${run.name} started in the background as ${run.id}.`,
        agents ? `Agents so far: ${agents}` : "No agents queued yet.",
        `The result arrives as a workflow-result message; keep working. Use workflow_runs with run "${run.id}" to check or cancel it.`,
    ];
    return {
        content: [{ type: "text" as const, text: lines.join("\n") }],
        details: { ...snapshot, runId: run.id, runStatus: run.status },
    };
}

async function blockingResult(
    run: WorkflowRun,
    manager: WorkflowRunManager,
    signal: AbortSignal | undefined,
) {
    let cancel = () => manager.cancel(run.id);
    signal?.addEventListener("abort", cancel, { once: true });
    try {
        await run.finished;
    } finally {
        signal?.removeEventListener("abort", cancel);
    }

    let snapshot = recomputeWorkflowSnapshot(run.snapshot);
    let details: WorkflowToolDetails = { ...snapshot, runId: run.id, runStatus: run.status };
    if (run.status === "cancelled") {
        // OMP-native abort contract: resolve with an error result instead of
        // throwing, so the terminal frame (skipped rows + abort line) still renders
        // while `isError` surfaces the abort to the model.
        return {
            isError: true,
            content: [{ type: "text" as const, text: "Workflow was aborted" }],
            details,
        };
    }
    if (run.status === "failed") throw run.failure;

    let outcome = run.outcome;
    return {
        content: [
            {
                type: "text" as const,
                text: `Workflow ${run.name} completed with ${outcome?.agentCount ?? 0} agent(s).\n\nResult:\n${formatWorkflowResult(outcome?.result)}`,
            },
        ],
        details: {
            ...details,
            meta: outcome?.meta,
            phases: outcome?.phases ?? snapshot.phases,
            logs: outcome?.logs ?? snapshot.logs,
            result: outcome?.result,
            durationMs: outcome?.durationMs,
            tokens: outcome?.tokens,
        },
    };
}
