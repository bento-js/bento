import { describe, expect, it } from "vitest";

import type { WorkflowIrcContext, WorkflowPeer } from "./irc.js";

import { agentIdFromLabel, appendIrcBlock, renderIrcBlock } from "./irc.js";

function peer(overrides: Partial<WorkflowPeer> = {}): WorkflowPeer {
    return { agentId: "DocsSweep", label: "docs sweep", status: "running", ...overrides };
}

function context(overrides: Partial<WorkflowIrcContext> = {}): WorkflowIrcContext {
    return {
        selfId: "RuntimeCore",
        coordinatorId: "Main",
        runId: "wf-1",
        runName: "metadata_scan",
        peers: [peer()],
        foreignPeers: [],
        ...overrides,
    };
}

describe("agentIdFromLabel", () => {
    it("derives a readable PascalCase id from a hyphenated label", () => {
        expect(agentIdFromLabel("runtime-core")).toBe("RuntimeCore");
    });

    it("derives one id per word across separators", () => {
        expect(agentIdFromLabel("package scan")).toBe("PackageScan");
        expect(agentIdFromLabel("docs/sweep_2")).toBe("DocsSweep2");
        expect(agentIdFromLabel("  review   the change  ")).toBe("ReviewTheChange");
    });

    it("is stable for the same label", () => {
        expect(agentIdFromLabel("runtime-core")).toBe(agentIdFromLabel("runtime-core"));
    });

    it("keeps the generic prefix when a label yields no leading letter", () => {
        expect(agentIdFromLabel("2nd pass")).toBe("Workflow2ndPass");
        expect(agentIdFromLabel("***")).toBe("Workflow");
    });

    it("never claims the coordinator's registry id", () => {
        expect(agentIdFromLabel("main")).toBe("MainAgent");
    });

    it("bounds the id so it stays usable as an artifact name", () => {
        let id = agentIdFromLabel("a".repeat(80));
        expect(id.length).toBe(40);
    });
});

describe("renderIrcBlock", () => {
    it("names the agent, the run, the coordinator, and how to message", () => {
        let block = renderIrcBlock(context());

        expect(block).toContain("## IRC");
        expect(block).toContain("You are `RuntimeCore` in workflow `metadata_scan` (run wf-1).");
        expect(block).toContain('`path: "agent://<id>"`');
        expect(block).toContain("Coordinator: `Main`");
        expect(block).toContain("Never wait for a message to arrive");
    });

    it("lists this run's peers with their labels and status", () => {
        let block = renderIrcBlock(
            context({
                peers: [
                    peer(),
                    peer({ agentId: "ApiProbe", label: "api probe", status: "queued" }),
                ],
            }),
        );

        expect(block).toContain("Peers in this run:");
        expect(block).toContain("- `DocsSweep` — docs sweep [running]");
        expect(block).toContain("- `ApiProbe` — api probe [queued]");
    });

    it("lists peers in other live runs with their workflow name", () => {
        let block = renderIrcBlock(
            context({
                foreignPeers: [peer({ agentId: "OtherOne", label: "other", runName: "api_scan" })],
            }),
        );

        expect(block).toContain("Peers in other live workflow runs:");
        expect(block).toContain("- `OtherOne` — other [running] in `api_scan`");
    });

    it("says so when the run has no other agents yet", () => {
        let block = renderIrcBlock(context({ peers: [] }));

        expect(block).toContain("Peers in this run: none yet");
        expect(block).not.toContain("Peers in other live workflow runs:");
    });
});

describe("appendIrcBlock", () => {
    it("keeps the prompt first and separates the block by one blank line", () => {
        let prompt = appendIrcBlock("Scan the repository.\n", context());

        expect(prompt.startsWith("Scan the repository.\n\n## IRC")).toBe(true);
    });
});
