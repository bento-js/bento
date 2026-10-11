import { describe, expect, it, vi } from "vite-plus/test";

import type { WorkflowCoordination, WorkflowRunDelivery, WorkflowRunStartOptions } from "./runs.js";
import type {
    WorkflowAgentOutcome,
    WorkflowAgentRequest,
    WorkflowAgentRunner,
} from "./workflow.js";

import { WorkflowRunManager } from "./runs.js";
import { parseWorkflowScript } from "./workflow.js";

const META = `export const meta = {
  name: "scan_repo",
  description: "Scan the repository",
};`;

const TWO_AGENT = `${META}
phase("Scan");
const findings = await parallel([
  () => agent("scan packages", { label: "package scan" }),
  () => agent("scan configs", { label: "config scan" }),
]);
return { findings };`;

/** A runner whose agents only finish when the test releases them. */
class GatedRunner implements WorkflowAgentRunner {
    readonly started: WorkflowAgentRequest[] = [];
    readonly coordination: WorkflowCoordination[] = [];
    #release = Promise.withResolvers<void>();
    #reserved = 0;

    async reserve(request: { label: string; index: number }): Promise<string> {
        this.#reserved++;
        return `Agent${request.index}:${request.label}`;
    }

    async run(request: WorkflowAgentRequest): Promise<WorkflowAgentOutcome> {
        this.started.push(request);
        await this.#release.promise;
        return { value: `${request.prompt} done`, tokens: 2 };
    }

    releaseAll(): void {
        this.#release.resolve(undefined);
    }

    get reservedCount(): number {
        return this.#reserved;
    }
}

function start(
    manager: WorkflowRunManager,
    runner: WorkflowAgentRunner,
    overrides: Partial<WorkflowRunStartOptions> = {},
) {
    return manager.start({
        program: parseWorkflowScript(TWO_AGENT),
        cwd: "/repo",
        coordinatorId: "Main",
        background: true,
        createRunner: () => runner,
        ...overrides,
    });
}

describe("WorkflowRunManager", () => {
    it("returns a run id and the queued agents before any agent finishes", () => {
        let runner = new GatedRunner();
        let manager = new WorkflowRunManager(vi.fn());

        let run = start(manager, runner);

        expect(run.id).toBe("wf-1");
        expect(run.name).toBe("scan_repo");
        expect(run.status).toBe("running");
        expect(run.snapshot.agents.map(agent => agent.label)).toEqual([
            "package scan",
            "config scan",
        ]);
        expect(runner.started.every(request => request.agentId !== undefined)).toBe(true);
    });

    it("numbers runs in start order", () => {
        let manager = new WorkflowRunManager(vi.fn());

        let first = start(manager, new GatedRunner());
        let second = start(manager, new GatedRunner());

        expect([first.id, second.id]).toEqual(["wf-1", "wf-2"]);
        expect(manager.list().map(run => run.id)).toEqual(["wf-1", "wf-2"]);
        expect(manager.latest()?.id).toBe("wf-2");
        expect(manager.get("wf-1")?.name).toBe("scan_repo");
    });

    it("delivers exactly one message carrying the result when the run completes", async () => {
        let deliveries: WorkflowRunDelivery[] = [];
        let manager = new WorkflowRunManager(delivery => deliveries.push(delivery));
        let runner = new GatedRunner();

        let run = start(manager, runner);
        expect(deliveries).toHaveLength(0);
        runner.releaseAll();
        await run.finished;

        expect(deliveries).toHaveLength(1);
        expect(deliveries[0]).toMatchObject({ runId: "wf-1", name: "scan_repo", status: "done" });
        expect(deliveries[0].content).toContain("Workflow scan_repo (wf-1) finished");
        expect(deliveries[0].content).toContain("scan packages done");
        expect(run.status).toBe("done");
        expect(run.outcome?.agentCount).toBe(2);
    });

    it("marks a failed run and says so in the delivered message", async () => {
        let deliveries: WorkflowRunDelivery[] = [];
        let manager = new WorkflowRunManager(delivery => deliveries.push(delivery));
        let runner: WorkflowAgentRunner = {
            async run() {
                return { value: "ok", tokens: 1 };
            },
        };

        let run = start(manager, runner, {
            program: parseWorkflowScript(`${META}\nthrow new Error("script exploded");`),
        });
        await run.finished;

        expect(run.status).toBe("failed");
        // A script error crosses the vm realm boundary, so it arrives stringified.
        expect(run.error).toContain("script exploded");
        expect(deliveries).toHaveLength(1);
        expect(deliveries[0].status).toBe("failed");
        expect(deliveries[0].content).toContain("Error: script exploded");
    });

    it("cancels a live run, skips its agents, and delivers a cancellation", async () => {
        let deliveries: WorkflowRunDelivery[] = [];
        let manager = new WorkflowRunManager(delivery => deliveries.push(delivery));
        let runner = new GatedRunner();

        let run = start(manager, runner);
        expect(manager.cancel("wf-1")).toBe(true);
        await run.finished;

        expect(run.status).toBe("cancelled");
        expect(run.snapshot.agents.every(agent => agent.status === "skipped")).toBe(true);
        expect(deliveries).toHaveLength(1);
        expect(deliveries[0].status).toBe("cancelled");
        expect(deliveries[0].content).toContain("was cancelled");
        expect(manager.cancel("wf-1")).toBe(false);
        expect(manager.cancel("wf-404")).toBe(false);
    });

    it("aborts every live run on shutdown without delivering", async () => {
        let deliver = vi.fn();
        let manager = new WorkflowRunManager(deliver);
        let first = new GatedRunner();
        let second = new GatedRunner();

        let runs = [start(manager, first), start(manager, second)];
        manager.cancelAll();
        await Promise.all(runs.map(run => run.finished));

        expect(runs.map(run => run.status)).toEqual(["cancelled", "cancelled"]);
        expect(deliver).not.toHaveBeenCalled();
    });

    it("keeps a blocking run out of message delivery", async () => {
        let deliver = vi.fn();
        let manager = new WorkflowRunManager(deliver);
        let runner = new GatedRunner();

        let run = start(manager, runner, { background: false });
        runner.releaseAll();
        await run.finished;

        expect(run.status).toBe("done");
        expect(deliver).not.toHaveBeenCalled();
    });

    it("streams frames to a blocking caller and ends with a completed frame", async () => {
        let frames: Array<{ complete: boolean; agents: number }> = [];
        let manager = new WorkflowRunManager(vi.fn());
        let runner = new GatedRunner();

        let run = start(manager, runner, {
            background: false,
            onFrame: (frame, complete) => frames.push({ complete, agents: frame.agentCount }),
        });
        runner.releaseAll();
        await run.finished;

        expect(frames.length).toBeGreaterThan(1);
        expect(frames.at(-1)).toEqual({ complete: true, agents: 2 });
    });

    it("passes the concurrency bound to the runtime", async () => {
        let manager = new WorkflowRunManager(vi.fn());
        let live = 0;
        let peak = 0;
        let runner: WorkflowAgentRunner = {
            async run(request) {
                live++;
                peak = Math.max(peak, live);
                await Promise.resolve();
                live--;
                return { value: request.prompt, tokens: 1 };
            },
        };

        let run = start(manager, runner, { concurrency: 1 });
        await run.finished;

        expect(peak).toBe(1);
    });

    it("gives each agent the coordinator id and its live peers", async () => {
        let manager = new WorkflowRunManager(vi.fn());
        let runner = new GatedRunner();
        let coordination: WorkflowCoordination | undefined;

        let run = start(manager, runner, {
            coordinatorId: "Main",
            createRunner: value => {
                coordination = value;
                return runner;
            },
        });

        expect(coordination?.runId).toBe("wf-1");
        expect(coordination?.runName).toBe("scan_repo");
        expect(coordination?.coordinatorId).toBe("Main");
        // Reservations resolve asynchronously; the adapter waits for them before
        // it builds a prompt, so the peer view is read after they settle.
        await Promise.resolve();
        let view = coordination?.peers("Agent1:package scan");
        expect(view?.peers.map(peer => peer.agentId)).toEqual(["Agent2:config scan"]);
        expect(view?.foreignPeers).toEqual([]);

        runner.releaseAll();
        await run.finished;
    });

    it("exposes agents of other live runs as foreign peers", async () => {
        let manager = new WorkflowRunManager(vi.fn());
        let first = new GatedRunner();
        let second = new GatedRunner();
        let coordination: WorkflowCoordination | undefined;

        let firstRun = start(manager, first, {
            createRunner: value => {
                coordination = value;
                return first;
            },
        });
        let secondRun = start(manager, second, {
            program: parseWorkflowScript(`${META}\nreturn agent("probe", { label: "api probe" });`),
        });
        await Promise.resolve();

        let view = coordination?.peers("Agent1:package scan");
        expect(view?.foreignPeers.map(peer => `${peer.agentId}@${peer.runName}`)).toEqual([
            "Agent1:api probe@scan_repo",
        ]);

        first.releaseAll();
        second.releaseAll();
        await Promise.all([firstRun.finished, secondRun.finished]);

        // A finished run's agents are no longer offered as correspondents.
        expect(coordination?.peers("Agent1:package scan").foreignPeers).toEqual([]);
    });
});
