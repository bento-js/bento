import type { ExtensionAPI, Theme } from "@oh-my-pi/pi-coding-agent";

import { createRequire } from "node:module";
import { describe, expect, it, vi } from "vite-plus/test";

import type {
    WorkflowAgentOutcome,
    WorkflowAgentRequest,
    WorkflowAgentRunner,
} from "./workflow.js";

// The barrel loads `pi-natives`, which cannot initialize under vitest; the real
// zod and a faithful Text double keep schema parsing and rendering honest.
vi.mock("@oh-my-pi/pi-coding-agent", () => {
    let require = createRequire(import.meta.resolve("@oh-my-pi/pi-coding-agent"));
    let zod = require("zod/v4");
    class Text {
        #text: string;
        constructor(text = "") {
            this.#text = text;
        }
        getText(): string {
            return this.#text;
        }
    }
    return { zod, z: zod, Text };
});

import { Text as PiText, zod as piZod } from "@oh-my-pi/pi-coding-agent";

import { createWorkflowRunsTool } from "./runs-tool.js";
import { WorkflowRunManager } from "./runs.js";
import { parseWorkflowScript } from "./workflow.js";

const META = `export const meta = {
  name: "scan_repo",
  description: "Scan the repository",
};`;

const SCRIPT = `${META}
phase("Scan");
return await agent("scan", { label: "package scan" });`;

let api = { zod: piZod, pi: { Text: PiText } } as unknown as ExtensionAPI;
let theme = {
    fg: (_color: string, text: string) => text,
    bold: (text: string) => text,
} as unknown as Theme;

class GatedRunner implements WorkflowAgentRunner {
    #release = Promise.withResolvers<void>();
    started = Promise.withResolvers<void>();

    async reserve(request: { label: string; index: number }): Promise<string> {
        return `Agent${request.index}`;
    }

    async run(request: WorkflowAgentRequest): Promise<WorkflowAgentOutcome> {
        this.started.resolve(undefined);
        await this.#release.promise;
        return { value: `${request.prompt} finished`, tokens: 3 };
    }

    releaseAll(): void {
        this.#release.resolve(undefined);
    }
}

interface RunsResult {
    isError?: boolean;
    content: Array<{ type: string; text?: string }>;
    details?: Record<string, unknown>;
}

function setup() {
    let manager = new WorkflowRunManager(vi.fn());
    let tool = createWorkflowRunsTool(api, manager);
    let call = (
        params: { action: string; run?: string; timeoutSeconds?: number },
        signal?: AbortSignal,
    ) =>
        tool.execute(
            "call-1",
            params as never,
            signal,
            undefined,
            undefined as never,
        ) as Promise<RunsResult>;
    let start = (runner: WorkflowAgentRunner) =>
        manager.start({
            program: parseWorkflowScript(SCRIPT),
            cwd: "/repo",
            coordinatorId: "Main",
            background: true,
            createRunner: () => runner,
        });
    return { manager, tool, call, start };
}

describe("createWorkflowRunsTool", () => {
    it("exposes a read-only control tool", () => {
        let { tool } = setup();
        expect(tool.name).toBe("workflow_runs");
        expect(tool.approval).toBe("read");
        expect(tool.description).toMatch(/list/);
        expect(tool.parameters.safeParse({ action: "list" }).success).toBe(true);
        expect(tool.parameters.safeParse({ action: "status", run: "wf-1" }).success).toBe(true);
        expect(tool.parameters.safeParse({ action: "nope" }).success).toBe(false);
        expect(tool.parameters.safeParse({ action: "wait", timeoutSeconds: 30 }).success).toBe(
            true,
        );
        expect(tool.parameters.safeParse({ action: "wait", timeoutSeconds: 0 }).success).toBe(
            false,
        );
        expect(tool.parameters.safeParse({ action: "nope" }).success).toBe(false);
        expect(tool.parameters.safeParse({}).success).toBe(false);
    });

    it("says so when nothing has run yet", async () => {
        let { call } = setup();

        let listed = await call({ action: "list" });
        expect(listed.content[0]?.text).toContain("No workflow has run");

        let status = await call({ action: "status" });
        expect(status.isError).toBe(true);
        expect(status.content[0]?.text).toContain("No workflow has run");
    });

    it("reports an unknown run id as an error", async () => {
        let { call, start } = setup();
        let runner = new GatedRunner();
        start(runner);

        let status = await call({ action: "status", run: "wf-9" });

        expect(status.isError).toBe(true);
        expect(status.content[0]?.text).toContain("No workflow run wf-9");
        runner.releaseAll();
    });

    it("lists every run of the session with its status", async () => {
        let { call, start, manager } = setup();
        let first = new GatedRunner();
        let second = new GatedRunner();
        start(first);
        start(second);
        first.releaseAll();
        await manager.get("wf-1")?.finished;

        let listed = await call({ action: "list" });

        let text = listed.content[0]?.text ?? "";
        expect(text).toContain("wf-1  done");
        expect(text).toContain("wf-2  running");
        expect(listed.details?.runs).toMatchObject([
            { runId: "wf-1", name: "scan_repo", status: "done" },
            { runId: "wf-2", name: "scan_repo", status: "running" },
        ]);
        second.releaseAll();
    });

    it("shows the live agent rows with their agent ids", async () => {
        let { call, start } = setup();
        let runner = new GatedRunner();
        start(runner);
        await runner.started.promise;

        let status = await call({ action: "status", run: "wf-1" });

        let text = status.content[0]?.text ?? "";
        expect(text).toContain("Workflow scan_repo (wf-1) is running");
        expect(text).toContain("Scan");
        expect(text).toContain("Agent1 · package scan");
        expect(status.details?.runStatus).toBe("running");
        runner.releaseAll();
    });

    it("defaults to the newest run and shows its result once it finishes", async () => {
        let { call, start, manager } = setup();
        let runner = new GatedRunner();
        start(runner);
        runner.releaseAll();
        await manager.get("wf-1")?.finished;

        let status = await call({ action: "status" });

        let text = status.content[0]?.text ?? "";
        expect(text).toContain("Workflow scan_repo (wf-1) finished");
        expect(text).toContain("Result:");
        expect(text).toContain("scan finished");
    });

    it("cancels a live run and refuses a finished one", async () => {
        let { call, start, manager } = setup();
        let runner = new GatedRunner();
        let run = start(runner);
        await runner.started.promise;

        let cancelled = await call({ action: "cancel", run: "wf-1" });
        await run.finished;

        expect(cancelled.details?.cancelled).toBe(true);
        expect(cancelled.content[0]?.text).toContain("Cancelling wf-1");
        expect(run.status).toBe("cancelled");
        expect(manager.get("wf-1")?.snapshot.agents.every(row => row.status === "skipped")).toBe(
            true,
        );

        let again = await call({ action: "cancel", run: "wf-1" });
        expect(again.details?.cancelled).toBe(false);
        expect(again.content[0]?.text).toContain("already cancelled");
    });

    it("renders the call and the result", async () => {
        let { tool, call, start, manager } = setup();
        let runner = new GatedRunner();
        start(runner);
        runner.releaseAll();
        await manager.get("wf-1")?.finished;

        let callView = tool.renderCall?.(
            { action: "status", run: "wf-1" },
            { expanded: false, isPartial: false },
            theme,
        );
        expect(textOf(callView)).toBe("workflow_runs status wf-1");

        let result = await call({ action: "status", run: "wf-1" });
        let resultView = tool.renderResult?.(
            result as never,
            { expanded: false, isPartial: false },
            theme,
        );
        expect(textOf(resultView)).toContain("Workflow scan_repo (wf-1) finished");
    });
});

describe("workflow_runs wait", () => {
    it("blocks until the named run settles and returns its result", async () => {
        let { call, start } = setup();
        let runner = new GatedRunner();
        start(runner);
        await runner.started.promise;

        let waiting = call({ action: "wait", run: "wf-1" });
        let pending = true;
        void waiting.then(() => (pending = false));
        // Flush pending microtasks: a wait that resolved eagerly would land here.
        await Promise.resolve();
        expect(pending).toBe(true);

        runner.releaseAll();
        let result = await waiting;

        expect(result.details?.outcome).toBe("settled");
        expect(result.details?.runStatus).toBe("done");
        expect(result.content[0]?.text).toContain("Workflow scan_repo (wf-1) finished");
        expect(result.content[0]?.text).toContain("scan finished");
    });

    it("returns at once for a run that already settled", async () => {
        let { call, start, manager } = setup();
        let runner = new GatedRunner();
        start(runner);
        runner.releaseAll();
        await manager.get("wf-1")?.finished;

        let result = await call({ action: "wait", run: "wf-1" });

        expect(result.details?.outcome).toBe("settled");
        expect(result.details?.runStatus).toBe("done");
    });

    it("without a run id settles on the first live run to finish", async () => {
        let { call, start } = setup();
        let first = new GatedRunner();
        let second = new GatedRunner();
        start(first);
        start(second);

        let waiting = call({ action: "wait" });
        second.releaseAll();
        let result = await waiting;

        expect(result.details?.runId).toBe("wf-2");
        expect(result.details?.outcome).toBe("settled");
        first.releaseAll();
    });

    it("says there is nothing to wait for when no run is live", async () => {
        let { call, start, manager } = setup();
        let runner = new GatedRunner();
        start(runner);
        runner.releaseAll();
        await manager.get("wf-1")?.finished;

        let result = await call({ action: "wait" });

        expect(result.content[0]?.text).toContain("No workflow run is live");
    });

    it("returns when the caller is interrupted, leaving the run alive", async () => {
        let { call, start, manager } = setup();
        let runner = new GatedRunner();
        start(runner);
        let controller = new AbortController();

        let waiting = call({ action: "wait", run: "wf-1" }, controller.signal);
        controller.abort();
        let result = await waiting;

        expect(result.details?.outcome).toBe("interrupted");
        expect(result.content[0]?.text).toContain("wf-1 (running)");
        expect(manager.get("wf-1")?.status).toBe("running");
        runner.releaseAll();
    });

    it("gives up at the timeout without touching the run", async () => {
        vi.useFakeTimers();
        try {
            let { call, start, manager } = setup();
            let runner = new GatedRunner();
            start(runner);

            let waiting = call({ action: "wait", run: "wf-1", timeoutSeconds: 30 });
            await vi.advanceTimersByTimeAsync(30_000);
            let result = await waiting;

            expect(result.details?.outcome).toBe("timeout");
            expect(result.content[0]?.text).toContain("Still running after 30s");
            expect(manager.get("wf-1")?.status).toBe("running");
            runner.releaseAll();
        } finally {
            vi.useRealTimers();
        }
    });
});

function textOf(component: unknown): string {
    if (component && typeof component === "object" && "getText" in component) {
        let getText = component.getText;
        if (typeof getText === "function") {
            let value = getText.call(component);
            if (typeof value === "string") return value;
        }
    }
    throw new Error("component is not a Text");
}
