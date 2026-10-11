import type { ExtensionAPI, ExtensionContext, Theme } from "@oh-my-pi/pi-coding-agent";
import type { AgentDefinition, SingleResult } from "@oh-my-pi/pi-coding-agent/task";
import type { ExecutorOptions } from "@oh-my-pi/pi-coding-agent/task/executor";

import { createRequire } from "node:module";
import { beforeEach, describe, expect, it, vi } from "vite-plus/test";

import type { OmpWorkflowAgentDependencies } from "./agent.js";
import type { WorkflowRunDelivery } from "./runs.js";
import type { WorkflowToolDetails } from "./tool.js";

// `agent.ts` (imported transitively by `tool.ts`) eagerly imports OMP's `/task`
// and `/task/executor` entrypoints, which load `pi-natives` — impossible under
// vitest. Mock those seams so the tool keeps its production imports while the
// runner runs against injected doubles. The barrel mock also re-supplies the real
// `zod` (resolved from OMP's own dependency tree, since the barrel itself would
// load natives) and a faithful `Text` double, so the API double stays real where
// it matters: strict schema parsing and renderer text extraction.
let { built } = vi.hoisted(() => ({ built: { allocators: 0 } }));

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
    return { getActiveSkills: () => [], zod, z: zod, Text };
});
vi.mock("@oh-my-pi/pi-coding-agent/task", () => ({
    AgentOutputManager: class {
        readonly #taken = new Set<string>();
        constructor(_getArtifactsDir: () => string | null) {
            built.allocators += 1;
        }
        async allocate(id: string): Promise<string> {
            let candidate = id;
            for (let n = 2; this.#taken.has(candidate); n++) candidate = `${id}-${n}`;
            this.#taken.add(candidate);
            return candidate;
        }
    },
    discoverAgents: vi.fn(),
    getAgent: (agents: AgentDefinition[], name: string): AgentDefinition | undefined =>
        agents.find(agent => agent.name === name),
}));
vi.mock("@oh-my-pi/pi-coding-agent/task/executor", () => ({ runSubprocess: vi.fn() }));
// Setting handles load `pi-utils` (crashes under vitest). These return each
// setting's default, as the real handles do for an unconfigured session.
vi.mock("@oh-my-pi/pi-coding-agent/task/settings", () => ({
    cfgTaskDisabledAgents: { get: () => [] },
    cfgTaskAgentModelOverrides: { get: () => ({}) },
}));
vi.mock("@oh-my-pi/pi-coding-agent/mcp/manager", () => ({
    MCPManager: class {
        getTools() {
            return [];
        }
    },
}));
vi.mock("@oh-my-pi/pi-coding-agent/tools/output-schema-validator", () => ({
    buildOutputValidator: () => ({}),
}));

import { Text as PiText, zod as piZod } from "@oh-my-pi/pi-coding-agent";

import { WorkflowRunManager } from "./runs.js";
import { createWorkflowTool } from "./tool.js";

const TASK: AgentDefinition = {
    name: "task",
    description: "General worker",
    systemPrompt: "You do tasks.",
    source: "bundled",
};

function makeResult(overrides: Partial<SingleResult> = {}): SingleResult {
    return {
        index: 0,
        id: "Agent",
        agent: "task",
        agentSource: "bundled",
        task: "",
        exitCode: 0,
        output: "",
        stderr: "",
        truncated: false,
        durationMs: 0,
        tokens: 0,
        requests: 0,
        ...overrides,
    };
}

// The API double carries the real zod (strict-schema fidelity) and the faithful
// Text double; casting through unknown is the DI seam vitest cannot type.
let api = {
    zod: piZod,
    pi: { Text: PiText },
} as unknown as ExtensionAPI;

let context = {
    cwd: "/repo",
    sessionManager: { getArtifactsDir: () => null, getArtifactManager: () => null },
    modelRegistry: {},
    agent: { kind: "main", id: "Main", name: "main", depth: 0 },
} as unknown as ExtensionContext;

// renderCall/renderResult only read `fg`/`bold`; identity styling keeps the raw
// text observable through Text.getText().
let theme = {
    fg: (_color: string, text: string) => text,
    bold: (text: string) => text,
} as unknown as Theme;

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

function makeDeps(
    run: (options: ExecutorOptions) => Promise<SingleResult> = async () => makeResult(),
): Partial<OmpWorkflowAgentDependencies> {
    return {
        discoverAgents: async () => ({ agents: [TASK], projectAgentsDir: null }),
        getActiveSkills: () => [],
        runSubprocess: vi.fn(run),
    };
}

interface ToolHarness {
    tool: ReturnType<typeof createWorkflowTool>;
    manager: WorkflowRunManager;
    deliveries: WorkflowRunDelivery[];
    updates: Array<{
        content: Array<{ type: string; text?: string }>;
        details?: WorkflowToolDetails;
    }>;
    execute(params: Record<string, unknown>, signal?: AbortSignal): Promise<ToolResult>;
}

interface ToolResult {
    isError?: boolean;
    content: Array<{ type: string; text?: string }>;
    details?: WorkflowToolDetails;
}

function harness(deps: Partial<OmpWorkflowAgentDependencies> = makeDeps()): ToolHarness {
    let deliveries: WorkflowRunDelivery[] = [];
    let manager = new WorkflowRunManager(delivery => deliveries.push(delivery));
    let tool = createWorkflowTool(api, {
        manager,
        settings: () => ({}) as never,
        dependencies: deps,
    });
    let updates: ToolHarness["updates"] = [];
    return {
        tool,
        manager,
        deliveries,
        updates,
        execute: (params, signal) =>
            tool.execute(
                "call-1",
                params as never,
                signal,
                update => updates.push(update as ToolHarness["updates"][number]),
                context,
            ) as Promise<ToolResult>,
    };
}

const META = `export const meta = {
  name: "metadata_scan",
  description: "Summarize repository metadata",
};`;

const TWO_AGENT = `${META}
phase("Scan");
const findings = await parallel([
  () => agent("scan packages", { label: "package scan" }),
  () => agent("scan configs", { label: "config scan" }),
]);
return { findings };`;

let validScript = `${META}
return await agent("inspect", { label: "inspect repo" });`;

/** Run to completion in blocking mode, the way the old custom tool always ran. */
async function runTool(script: string, deps: Partial<OmpWorkflowAgentDependencies>) {
    let tool = harness(deps);
    let result = await tool.execute({ script, background: false });
    return { result, updates: tool.updates, deliveries: tool.deliveries };
}

beforeEach(() => {
    built.allocators = 0;
});

describe("createWorkflowTool", () => {
    it("exposes an essential top-level workflow tool with a model-facing description", () => {
        let { tool } = harness();
        expect(tool.name).toBe("workflow");
        expect(tool.loadMode).toBe("essential");
        expect(tool.description).toMatch(/deterministic JavaScript workflow/);
        expect(tool.description).toMatch(/every agent needs a short unique label/);
        expect(tool.description).toMatch(/parallel receives zero-argument functions/);
        expect(tool.description).toMatch(/background by default/);
        expect(tool.description).toMatch(/workflow_runs/);
    });

    it("validates parameters with a strict schema", () => {
        let { tool } = harness();
        expect(tool.parameters.safeParse({ script: validScript }).success).toBe(true);
        expect(tool.parameters.safeParse({ script: validScript, args: { a: 1 } }).success).toBe(
            true,
        );
        expect(
            tool.parameters.safeParse({ script: validScript, background: false, concurrency: 4 })
                .success,
        ).toBe(true);
        expect(tool.parameters.safeParse({ script: validScript, concurrency: 100 }).success).toBe(
            false,
        );
        expect(tool.parameters.safeParse({ script: validScript, concurrency: 0 }).success).toBe(
            false,
        );
        expect(tool.parameters.safeParse({}).success).toBe(false);
        expect(tool.parameters.safeParse({ script: 42 }).success).toBe(false);
        expect(tool.parameters.safeParse({ script: validScript, extra: true }).success).toBe(false);
    });

    it("streams running snapshots and preserves the final result of a two-agent workflow", async () => {
        let outputs: Record<string, string> = {
            "scan packages": "3 packages",
            "scan configs": "2 configs",
        };
        let deps = makeDeps(async options =>
            makeResult({
                task: options.task,
                output: outputs[firstLine(options.task)] ?? "",
                tokens: 5,
            }),
        );

        let { result, updates } = await runTool(TWO_AGENT, deps);

        let partials = updates.map(update => update.content[0]?.text ?? "");
        expect(partials.some(text => text.startsWith("Workflow running"))).toBe(true);

        let finalText = result.content[0];
        expect(finalText.type).toBe("text");
        expect(finalText.text).toContain("metadata_scan");

        let details = result.details as WorkflowToolDetails;
        expect(details.name).toBe("metadata_scan");
        expect(details.agents).toHaveLength(2);
        expect(details.agents.every(agent => agent.status === "done")).toBe(true);
        expect(details.doneCount).toBe(2);
        expect(details.result).toEqual({ findings: ["3 packages", "2 configs"] });
        // The blocking result augments the snapshot with the run's token total.
        let detailed = details as WorkflowToolDetails & { tokens: number };
        expect(detailed.tokens).toBe(10);
    });

    it("strips a single outer code fence before parsing", async () => {
        let fenced = ["```js", validScript, "```"].join("\n");
        let deps = makeDeps(async () => makeResult({ output: "done" }));

        let { result } = await runTool(fenced, deps);

        let details = result.details as WorkflowToolDetails;
        expect(details.name).toBe("metadata_scan");
        expect(details.agents).toHaveLength(1);
        expect(built.allocators).toBe(1);
    });

    it("rejects a script with surrounding prose without allocating anything", async () => {
        let withProse = [
            "Here is the workflow you asked for:",
            "```js",
            validScript,
            "```",
            "Enjoy!",
        ].join("\n");

        await expect(runTool(withProse, makeDeps())).rejects.toThrow(/prose/i);
        expect(built.allocators).toBe(0);
    });

    it("rejects a workflow that never calls an agent", async () => {
        let noAgent = `${META}
phase("Idle");
return 42;`;

        await expect(runTool(noAgent, makeDeps())).rejects.toThrow(/at least one agent/i);
    });

    it("fails parsing before the run starts", async () => {
        let brokenMeta = `export const meta = { name: "broken" };
return await agent("x", { label: "x" });`;

        await expect(runTool(brokenMeta, makeDeps())).rejects.toThrow();
        expect(built.allocators).toBe(0);
    });

    it("marks an aborted in-flight agent skipped, clears its abort error, and resolves an error result", async () => {
        let controller = new AbortController();
        let started = Promise.withResolvers<void>();
        let tool = harness(
            makeDeps(async () => {
                started.resolve();
                await new Promise<void>(() => {}); // never settles; the abort ends it
                return makeResult();
            }),
        );

        let run = tool.execute({ script: validScript, background: false }, controller.signal);
        await started.promise;
        controller.abort();

        let result = await run;
        expect(result.isError).toBe(true);
        expect(result.content[0]).toEqual({ type: "text", text: "Workflow was aborted" });
        let rows = result.details?.agents ?? [];
        expect(rows).toHaveLength(1);
        expect(rows[0]?.status).toBe("skipped");
        expect(rows[0]?.error).toBeUndefined();

        // The final streamed frame mirrors the resolved skipped snapshot.
        let last = tool.updates.at(-1);
        expect(last?.content[0]?.text?.startsWith("Workflow completed")).toBe(true);
        expect(last?.details?.agents?.[0]?.status).toBe("skipped");
        // A blocking run reports through its tool result, never through a message.
        expect(tool.deliveries).toHaveLength(0);
    });

    it("keeps a genuine branch failure as error while skipping the aborted branch", async () => {
        let controller = new AbortController();
        let hangStarted = Promise.withResolvers<void>();
        let tool = harness(
            makeDeps(async options => {
                if (firstLine(options.task) === "hang") {
                    hangStarted.resolve();
                    await new Promise<void>(() => {}); // in-flight until abort
                }
                return makeResult({ task: options.task, exitCode: 1, error: "branch blew up" });
            }),
        );
        let script = `${META}
phase("Scan");
const failed = await agent("fail", { label: "failing agent" });
const hung = agent("hang", { label: "hanging agent" });
await hung;
return { failed, hung };`;

        let run = tool.execute({ script, background: false }, controller.signal);
        await hangStarted.promise;
        controller.abort();

        let result = await run;
        expect(result.isError).toBe(true);
        let rows = result.details?.agents ?? [];
        expect(rows.find(row => row.label === "failing agent")?.status).toBe("error");
        expect(rows.find(row => row.label === "failing agent")?.error).toBe("branch blew up");
        expect(rows.find(row => row.label === "hanging agent")?.status).toBe("skipped");
        expect(rows.find(row => row.label === "hanging agent")?.error).toBeUndefined();
    });

    it("keeps a pre-abort failure that carries the exact abort message as error, not skipped", async () => {
        let controller = new AbortController();
        let hangStarted = Promise.withResolvers<void>();
        let tool = harness(
            makeDeps(async options => {
                if (firstLine(options.task) === "hang") {
                    hangStarted.resolve();
                    await new Promise<void>(() => {}); // in-flight until abort
                }
                // The genuine failure carries the exact text an abort would produce;
                // landing before the signal aborts, it must stay error, not skipped.
                return makeResult({
                    task: options.task,
                    exitCode: 1,
                    error: "Workflow was aborted",
                });
            }),
        );
        let script = `${META}
phase("Scan");
const failed = await agent("fail", { label: "failing agent" });
const hung = agent("hang", { label: "hanging agent" });
await hung;
return { failed, hung };`;

        let run = tool.execute({ script, background: false }, controller.signal);
        await hangStarted.promise;
        controller.abort();

        let result = await run;
        expect(result.isError).toBe(true);
        let rows = result.details?.agents ?? [];
        expect(rows.find(row => row.label === "failing agent")?.status).toBe("error");
        expect(rows.find(row => row.label === "failing agent")?.error).toBe("Workflow was aborted");
        expect(rows.find(row => row.label === "hanging agent")?.status).toBe("skipped");
    });

    it("propagates a non-abort error whose message contains 'abort' when the signal is not aborted", async () => {
        let script = `${META}
await agent("x", { label: "x" });
throw new Error("abort check failed");`;

        // The message contains "abort", but the signal never aborted, so it must
        // surface as a real failure, not be reclassified as a cancellation.
        await expect(
            runTool(
                script,
                makeDeps(async () => makeResult({ output: "done" })),
            ),
        ).rejects.toThrow("abort check failed");
    });

    it("emits one coherent completed result for a BigInt result without throwing", async () => {
        let script = `${META}
await agent("x", { label: "x" });
return { total: 10n };`;

        let { result } = await runTool(
            script,
            makeDeps(async () => makeResult({ output: "done" })),
        );

        expect(result.isError).toBeFalsy();
        let body = result.content[0]?.text ?? "";
        expect(body).toContain("Workflow metadata_scan completed");
        expect(body).toContain("10n");
        expect(result.details?.result).toEqual({ total: 10n });
    });

    it("emits one coherent completed result for a cyclic result without throwing", async () => {
        let script = `${META}
await agent("x", { label: "x" });
const node = { name: "root" };
node.self = node;
return node;`;

        let { result } = await runTool(
            script,
            makeDeps(async () => makeResult({ output: "done" })),
        );

        expect(result.isError).toBeFalsy();
        let body = result.content[0]?.text ?? "";
        expect(body).toContain("Workflow metadata_scan completed");
        expect(body).toContain("root");
        let cyclic = result.details?.result as { self?: unknown };
        expect(cyclic.self).toBe(cyclic);
    });

    it("bounds live agents with the concurrency parameter", async () => {
        let live = 0;
        let peak = 0;
        let tool = harness(
            makeDeps(async options => {
                live++;
                peak = Math.max(peak, live);
                await Promise.resolve();
                live--;
                return makeResult({ task: options.task });
            }),
        );

        await tool.execute({ script: TWO_AGENT, background: false, concurrency: 1 });

        expect(peak).toBe(1);
    });

    it("skips both agents when aborted at concurrency 1 while the first runs", async () => {
        let controller = new AbortController();
        let firstStarted = Promise.withResolvers<void>();
        let tool = harness(
            makeDeps(async options => {
                if (firstLine(options.task) === "first") {
                    firstStarted.resolve();
                    await new Promise<void>(() => {}); // in-flight until abort
                }
                return makeResult({ task: options.task });
            }),
        );
        let script = `${META}
const first = agent("first", { label: "first agent" });
const second = agent("second", { label: "second agent" });
return Promise.all([first, second]);`;

        let run = tool.execute({ script, background: false, concurrency: 1 }, controller.signal);
        await firstStarted.promise;
        controller.abort();

        let result = await run;
        expect(result.isError).toBe(true);
        let rows = result.details?.agents ?? [];
        expect(rows).toHaveLength(2);
        expect(rows.every(row => row.status === "skipped")).toBe(true);
        expect(new Set(rows.map(row => row.id)).size).toBe(2);
    });

    it("renders the tool call title", () => {
        let { tool } = harness();
        let component = tool.renderCall?.(
            { script: validScript },
            { expanded: false, isPartial: false },
            theme,
        );
        expect(textOf(component)).toContain("workflow");
    });

    it("renders the workflow snapshot for results with details", () => {
        let { tool } = harness();
        let details: WorkflowToolDetails = {
            name: "demo",
            phases: [],
            logs: [],
            agents: [],
            agentCount: 0,
            runningCount: 0,
            doneCount: 0,
            errorCount: 0,
        };
        let finalView = tool.renderResult?.(
            { content: [], details },
            { expanded: false, isPartial: false },
            theme,
        );
        expect(textOf(finalView)).toContain("Workflow completed");
        expect(textOf(finalView)).toContain("demo");

        let partialView = tool.renderResult?.(
            { content: [], details },
            { expanded: false, isPartial: true },
            theme,
        );
        expect(textOf(partialView)).toContain("Workflow running");
    });

    it("renders the skipped snapshot and the abort line for an aborted error result", () => {
        let { tool } = harness();
        let details: WorkflowToolDetails = {
            name: "demo",
            phases: [],
            logs: [],
            agents: [{ id: 1, label: "inspect repo", prompt: "p", status: "skipped" }],
            agentCount: 1,
            runningCount: 0,
            doneCount: 0,
            errorCount: 0,
        };
        let view = tool.renderResult?.(
            { isError: true, content: [{ type: "text", text: "Workflow was aborted" }], details },
            { expanded: false, isPartial: false },
            theme,
        );
        let text = textOf(view);
        expect(text).toContain("Workflow completed");
        expect(text).toContain("inspect repo");
        expect(text).toContain("Workflow was aborted");
    });

    it("falls back to text content, then a muted label, when details are missing", () => {
        let { tool } = harness();
        let withText = tool.renderResult?.(
            { content: [{ type: "text", text: "raw summary" }] },
            { expanded: false, isPartial: false },
            theme,
        );
        expect(textOf(withText)).toBe("raw summary");

        let empty = tool.renderResult?.(
            { content: [] },
            { expanded: false, isPartial: false },
            theme,
        );
        expect(textOf(empty)).toBe("workflow");
    });
});

describe("background runs", () => {
    it("returns the run id and queued agent ids before any agent finishes", async () => {
        let finished = false;
        let release = Promise.withResolvers<void>();
        let tool = harness(
            makeDeps(async options => {
                await release.promise;
                finished = true;
                return makeResult({ task: options.task });
            }),
        );

        let result = await tool.execute({ script: TWO_AGENT });

        expect(finished).toBe(false);
        expect(result.details?.runId).toBe("wf-1");
        expect(result.details?.runStatus).toBe("running");
        let text = result.content[0]?.text ?? "";
        expect(text).toContain("Workflow metadata_scan started in the background as wf-1");
        expect(text).toContain("PackageScan (package scan)");
        expect(text).toContain("ConfigScan (config scan)");
        expect(result.details?.agents.map(agent => agent.agentId)).toEqual([
            "PackageScan",
            "ConfigScan",
        ]);

        release.resolve(undefined);
        await tool.manager.get("wf-1")?.finished;
    });

    it("renders the start line rather than an empty snapshot", async () => {
        let tool = harness();
        let result = await tool.execute({ script: validScript });
        await tool.manager.get("wf-1")?.finished;

        let view = tool.tool.renderResult?.(
            result as never,
            { expanded: false, isPartial: false },
            theme,
        );
        expect(textOf(view)).toContain("started in the background as wf-1");
    });

    it("delivers exactly one message carrying the result when the run completes", async () => {
        let tool = harness(makeDeps(async () => makeResult({ output: "3 packages" })));

        await tool.execute({ script: TWO_AGENT });
        await tool.manager.get("wf-1")?.finished;

        expect(tool.deliveries).toHaveLength(1);
        expect(tool.deliveries[0]).toMatchObject({ runId: "wf-1", status: "done" });
        expect(tool.deliveries[0].content).toContain("Workflow metadata_scan (wf-1) finished");
        expect(tool.deliveries[0].content).toContain("3 packages");
    });

    it("delivers a failure message when the script throws", async () => {
        let tool = harness();
        let script = `${META}
await agent("x", { label: "x" });
throw new Error("script exploded");`;

        await tool.execute({ script });
        await tool.manager.get("wf-1")?.finished;

        expect(tool.deliveries).toHaveLength(1);
        expect(tool.deliveries[0].status).toBe("failed");
        expect(tool.deliveries[0].content).toContain("script exploded");
    });

    it("delivers a cancellation message and skips the running agents", async () => {
        let started = Promise.withResolvers<void>();
        let tool = harness(
            makeDeps(async () => {
                started.resolve();
                await new Promise<void>(() => {}); // in-flight until cancel
                return makeResult();
            }),
        );

        await tool.execute({ script: validScript });
        await started.promise;
        expect(tool.manager.cancel("wf-1")).toBe(true);
        await tool.manager.get("wf-1")?.finished;

        expect(tool.deliveries).toHaveLength(1);
        expect(tool.deliveries[0].status).toBe("cancelled");
        expect(tool.deliveries[0].content).toContain("was cancelled");
        expect(tool.deliveries[0].snapshot.agents.every(row => row.status === "skipped")).toBe(
            true,
        );
    });

    it("gives every agent of a wave the IRC block naming its siblings", async () => {
        let prompts: string[] = [];
        let tool = harness(
            makeDeps(async options => {
                prompts.push(options.task);
                return makeResult({ task: options.task });
            }),
        );

        await tool.execute({ script: TWO_AGENT });
        await tool.manager.get("wf-1")?.finished;

        expect(prompts).toHaveLength(2);
        let [first, second] = prompts;
        expect(first).toContain("You are `PackageScan` in workflow `metadata_scan` (run wf-1)");
        expect(first).toContain("Coordinator: `Main`");
        expect(first).toContain("- `ConfigScan` — config scan");
        expect(second).toContain("You are `ConfigScan`");
        expect(second).toContain("- `PackageScan` — package scan");
        for (let prompt of prompts) expect(prompt).toContain('`path: "agent://<id>"`');
    });

    it("keeps agent ids unique across concurrent runs of the same workflow", async () => {
        let release = Promise.withResolvers<void>();
        let tool = harness(
            makeDeps(async options => {
                await release.promise;
                return makeResult({ task: options.task });
            }),
        );

        await tool.execute({ script: validScript });
        await tool.execute({ script: validScript });
        release.resolve(undefined);
        await Promise.all(tool.manager.list().map(run => run.finished));

        let ids = tool.manager
            .list()
            .flatMap(run => run.snapshot.agents.map(agent => agent.agentId));
        expect(ids).toEqual(["InspectRepo", "InspectRepo-2"]);
    });
});

/** The prompt carries the IRC block beneath it; tests match on the task line. */
function firstLine(task: string): string {
    return task.split("\n")[0];
}
