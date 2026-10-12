// Registers the workflow runtime as session tools. It is an extension rather
// than a custom tool because only the extension API can wake the calling session
// with a message when a background run finishes.
import type { ExtensionAPI } from "@oh-my-pi/pi-coding-agent";

import type { OmpWorkflowAgentDependencies, WorkflowSessionContext } from "./agent.js";
import type { WorkflowRunDelivery } from "./runs.js";

import { createWorkflowRunsTool } from "./runs-tool.js";
import { WorkflowRunManager } from "./runs.js";
import { createWorkflowTool } from "./tool.js";

export const WORKFLOW_RESULT_MESSAGE = "workflow-result";

export interface DynamicWorkflowsOptions {
    /** Session settings for spawned subagents; defaults to OMP's live settings. */
    settings?: () => WorkflowSessionContext["settings"];
    dependencies?: Partial<OmpWorkflowAgentDependencies>;
}

export default function dynamicWorkflows(pi: ExtensionAPI, options: DynamicWorkflowsOptions = {}) {
    let manager = new WorkflowRunManager(delivery => deliver(pi, delivery));
    // Read through a getter: the global settings handle is not ready while
    // extensions load, only once a tool actually runs.
    let settings = options.settings ?? (() => pi.pi.settings);

    pi.registerTool(
        createWorkflowTool(pi, { manager, settings, dependencies: options.dependencies }),
    );
    pi.registerTool(createWorkflowRunsTool(pi, manager));

    // A run outlives the tool call that started it, so the session's own teardown
    // is what ends one that is still live.
    pi.on("session_shutdown", () => manager.cancelAll());
}

function deliver(pi: ExtensionAPI, delivery: WorkflowRunDelivery): void {
    pi.sendMessage(
        {
            customType: WORKFLOW_RESULT_MESSAGE,
            content: delivery.content,
            display: true,
            details: delivery,
            attribution: "agent",
        },
        // `followUp` is the only delivery a busy session is guaranteed to read: it
        // enters the agent's own queue, so the turn in flight continues into the
        // result instead of settling without it. An `aside` is picked up only at a
        // step boundary, and a run that lands while the agent is already composing
        // its final message strands it. When the session is idle, `triggerTurn`
        // starts the turn that reads it.
        { deliverAs: "followUp", triggerTurn: true },
    );
}
