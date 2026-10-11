// Translates the runtime's agent/phase/log events into the live snapshot every
// consumer renders from. Shared by blocking and background runs.
import type { WorkflowSnapshot } from "./display.js";
import type { WorkflowRunOptions } from "./workflow.js";

import { preview } from "./display.js";

/** The subset of run options that keeps a snapshot current. */
export type WorkflowProgressHandlers = Pick<
    WorkflowRunOptions,
    | "onLog"
    | "onPhase"
    | "onAgentQueued"
    | "onAgentReserved"
    | "onAgentStart"
    | "onAgentProgress"
    | "onAgentEnd"
>;

export interface ProgressHandlerOptions {
    /** Called after every mutation so a consumer can emit a frame. */
    onChange: () => void;
    /** Whether the run's own signal has already fired. */
    isAborted: () => boolean;
}

/**
 * Build the event handlers that keep `snapshot` current. The snapshot stays the
 * mutable accumulator; `onChange` is where consumers clone a frame from it.
 */
export function createProgressHandlers(
    snapshot: WorkflowSnapshot,
    options: ProgressHandlerOptions,
): WorkflowProgressHandlers {
    let { onChange, isAborted } = options;
    return {
        onLog(message) {
            snapshot.logs.push(message);
            onChange();
        },
        onPhase(title) {
            snapshot.currentPhase = title;
            if (!snapshot.phases.includes(title)) snapshot.phases.push(title);
            onChange();
        },
        onAgentQueued(event) {
            snapshot.agents.push({
                id: event.id,
                label: event.label,
                phase: event.phase,
                prompt: event.prompt,
                status: "queued",
            });
            onChange();
        },
        onAgentReserved(event) {
            let row = snapshot.agents.find(agent => agent.id === event.id);
            if (!row) return;
            row.agentId = event.agentId;
            onChange();
        },
        onAgentStart(event) {
            // Transition the row queued for this id to running (ids are unique, so
            // this is the one matching row). Fall back to a fresh running row only
            // if the queued event was somehow never seen.
            let row = snapshot.agents.find(
                agent => agent.id === event.id && agent.status === "queued",
            );
            if (row) {
                row.status = "running";
            } else {
                snapshot.agents.push({
                    id: event.id,
                    label: event.label,
                    phase: event.phase,
                    prompt: event.prompt,
                    status: "running",
                });
            }
            onChange();
        },
        onAgentProgress() {
            onChange();
        },
        onAgentEnd(event) {
            // Resolve the running row for this display id (ids are unique, so this
            // is the single row the completion belongs to).
            let row = snapshot.agents.find(
                agent => agent.id === event.id && agent.status === "running",
            );
            if (!row) {
                onChange();
                return;
            }
            if (!event.error) {
                row.status = "done";
                row.resultPreview = preview(event.result);
            } else if (isAborted() && event.error === "Workflow was aborted") {
                // This agent was drained by an abort that has already fired — render
                // it as skipped, not a failure, and omit the synthetic abort error. A
                // pre-abort failure that happens to carry the same text still lands
                // here with the signal unaborted, so it stays a genuine error.
                row.status = "skipped";
            } else {
                row.status = "error";
                row.error = event.error;
            }
            onChange();
        },
    };
}

/**
 * Mark rows cut short by an abort before `onAgentEnd` could classify them: still
 * queued (never acquired a permit) or running (in flight when the abort fired).
 * Abort-drained rows are already skipped; genuine failures keep "error".
 */
export function markAbortedRowsSkipped(snapshot: WorkflowSnapshot): void {
    for (let row of snapshot.agents) {
        if (row.status === "running" || row.status === "queued") row.status = "skipped";
    }
}
