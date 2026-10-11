// Owns every workflow run of a session: the live snapshot, cancellation, the
// peer directory agents address each other through, and the completion message
// a background run delivers back to the coordinator.
import type { WorkflowSnapshot } from "./display.js";
import type { WorkflowPeer } from "./irc.js";
import type { WorkflowProgressHandlers } from "./progress.js";
import type { WorkflowAgentRunner, WorkflowMeta, WorkflowRunResult } from "./workflow.js";

import {
    createWorkflowSnapshot,
    formatWorkflowResult,
    recomputeWorkflowSnapshot,
    renderWorkflowText,
} from "./display.js";
import { createProgressHandlers, markAbortedRowsSkipped } from "./progress.js";
import { runWorkflow } from "./workflow.js";

export type WorkflowRunStatus = "running" | "done" | "failed" | "cancelled";

export interface WorkflowProgram {
    meta: WorkflowMeta;
    body: string;
}

/** What a running agent needs to address its peers and the coordinator. */
export interface WorkflowCoordination {
    runId: string;
    runName: string;
    coordinatorId: string;
    peers(selfAgentId: string): { peers: WorkflowPeer[]; foreignPeers: WorkflowPeer[] };
}

export interface WorkflowRunStartOptions {
    program: WorkflowProgram;
    cwd: string;
    args?: unknown;
    concurrency?: number;
    coordinatorId: string;
    /** Background runs deliver their result as a message; blocking runs do not. */
    background: boolean;
    createRunner: (coordination: WorkflowCoordination) => WorkflowAgentRunner;
    onFrame?: (frame: WorkflowSnapshot, complete: boolean) => void;
}

export interface WorkflowRun {
    readonly id: string;
    readonly name: string;
    readonly background: boolean;
    readonly status: WorkflowRunStatus;
    /** Live accumulator; clone with `recomputeWorkflowSnapshot` before handing it out. */
    readonly snapshot: WorkflowSnapshot;
    readonly startedAt: number;
    readonly endedAt?: number;
    readonly outcome?: WorkflowRunResult;
    readonly error?: string;
    /** The value the run threw, so a blocking caller can rethrow it unchanged. */
    readonly failure?: unknown;
    readonly finished: Promise<void>;
}

export interface WorkflowRunDelivery {
    runId: string;
    name: string;
    status: WorkflowRunStatus;
    content: string;
    snapshot: WorkflowSnapshot;
}

interface MutableRun extends WorkflowRun {
    status: WorkflowRunStatus;
    endedAt?: number;
    outcome?: WorkflowRunResult;
    error?: string;
    failure?: unknown;
    finished: Promise<void>;
    controller: AbortController;
}

export class WorkflowRunManager {
    readonly #runs = new Map<string, MutableRun>();
    readonly #deliver: (delivery: WorkflowRunDelivery) => void;
    #sequence = 0;
    // Cleared on shutdown: a run cancelled because the session is going away has
    // nobody left to deliver to.
    #delivering = true;

    constructor(deliver: (delivery: WorkflowRunDelivery) => void) {
        this.#deliver = deliver;
    }

    /**
     * Start a run and return its record synchronously, before any agent has had a
     * chance to finish. Agents the script queues before its first `await` are
     * already on the record's snapshot.
     */
    start(options: WorkflowRunStartOptions): WorkflowRun {
        let id = `wf-${++this.#sequence}`;
        let controller = new AbortController();
        let snapshot = createWorkflowSnapshot(options.program.meta);
        let run: MutableRun = {
            id,
            name: options.program.meta.name,
            background: options.background,
            status: "running",
            snapshot,
            startedAt: Date.now(),
            finished: Promise.resolve(),
            controller,
        };
        this.#runs.set(id, run);

        let emit = (complete = false) =>
            options.onFrame?.(recomputeWorkflowSnapshot(snapshot), complete);
        let handlers = createProgressHandlers(snapshot, {
            onChange: () => emit(),
            isAborted: () => controller.signal.aborted,
        });
        let runner = options.createRunner({
            runId: id,
            runName: run.name,
            coordinatorId: options.coordinatorId,
            peers: selfAgentId => this.#peers(id, selfAgentId),
        });

        run.finished = this.#execute(run, options, runner, handlers, emit);
        return run;
    }

    list(): WorkflowRun[] {
        return [...this.#runs.values()];
    }

    /** Runs that have not settled yet. */
    live(): WorkflowRun[] {
        return [...this.#runs.values()].filter(run => run.status === "running");
    }

    get(runId: string): WorkflowRun | undefined {
        return this.#runs.get(runId);
    }

    /** The newest run, which is what a status request without an id means. */
    latest(): WorkflowRun | undefined {
        let runs = [...this.#runs.values()];
        return runs[runs.length - 1];
    }

    /** Abort a live run. Returns false for an unknown or already finished run. */
    cancel(runId: string): boolean {
        let run = this.#runs.get(runId);
        if (!run || run.status !== "running") return false;
        run.controller.abort();
        return true;
    }

    /** Abort every live run without delivering results; the session is ending. */
    cancelAll(): void {
        this.#delivering = false;
        for (let run of this.#runs.values()) {
            if (run.status === "running") run.controller.abort();
        }
    }

    async #execute(
        run: MutableRun,
        options: WorkflowRunStartOptions,
        runner: WorkflowAgentRunner,
        handlers: WorkflowProgressHandlers,
        emit: (complete?: boolean) => void,
    ): Promise<void> {
        try {
            let outcome = await runWorkflow(options.program, {
                cwd: options.cwd,
                args: options.args,
                runner,
                concurrency: options.concurrency,
                signal: run.controller.signal,
                ...handlers,
            });
            run.snapshot.result = outcome.result;
            run.snapshot.durationMs = outcome.durationMs;
            run.outcome = outcome;
            run.status = "done";
        } catch (error) {
            // Only a genuine signal abort is a cancellation; a real failure whose
            // message merely mentions aborting stays a failure.
            if (run.controller.signal.aborted) {
                markAbortedRowsSkipped(run.snapshot);
                run.status = "cancelled";
            } else {
                run.status = "failed";
                run.failure = error;
                run.error = error instanceof Error ? error.message : String(error);
            }
        }
        run.endedAt = Date.now();
        emit(true);
        if (run.background && this.#delivering) this.#deliver(deliveryFor(run));
    }

    #peers(
        runId: string,
        selfAgentId: string,
    ): { peers: WorkflowPeer[]; foreignPeers: WorkflowPeer[] } {
        let peers: WorkflowPeer[] = [];
        let foreignPeers: WorkflowPeer[] = [];
        for (let run of this.#runs.values()) {
            let own = run.id === runId;
            if (!own && run.status !== "running") continue;
            for (let row of run.snapshot.agents) {
                if (!row.agentId || row.agentId === selfAgentId) continue;
                let peer: WorkflowPeer = {
                    agentId: row.agentId,
                    label: row.label,
                    status: row.status,
                };
                if (own) peers.push(peer);
                // A finished agent of another run is not a useful correspondent:
                // only its own run's result still carries its work.
                else if (row.status === "queued" || row.status === "running") {
                    foreignPeers.push({ ...peer, runName: run.name });
                }
            }
        }
        return { peers, foreignPeers };
    }
}

export function deliveryFor(run: WorkflowRun): WorkflowRunDelivery {
    let snapshot = recomputeWorkflowSnapshot(run.snapshot);
    let outcome =
        run.status === "failed"
            ? `Error: ${run.error ?? "unknown failure"}`
            : run.status === "cancelled"
              ? "Cancelled before the workflow produced a result."
              : `Result:\n${formatWorkflowResult(run.outcome?.result)}`;
    return {
        runId: run.id,
        name: run.name,
        status: run.status,
        snapshot,
        content: [
            runHeadline(run, snapshot),
            "",
            renderWorkflowText(snapshot, true),
            "",
            outcome,
        ].join("\n"),
    };
}

/** One-line state of a run, shared by the delivered message and the status view. */
export function runHeadline(run: WorkflowRun, snapshot: WorkflowSnapshot): string {
    let seconds = (((run.endedAt ?? Date.now()) - run.startedAt) / 1000).toFixed(1);
    let verb =
        run.status === "done"
            ? "finished"
            : run.status === "failed"
              ? "failed"
              : run.status === "cancelled"
                ? "was cancelled"
                : "is running";
    return `Workflow ${run.name} (${run.id}) ${verb} after ${seconds}s with ${snapshot.agentCount} agent(s).`;
}
