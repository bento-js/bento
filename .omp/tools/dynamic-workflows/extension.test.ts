import type { ExtensionAPI, ExtensionContext, ToolDefinition } from "@oh-my-pi/pi-coding-agent";
import type { AgentDefinition, SingleResult } from "@oh-my-pi/pi-coding-agent/task";
import type { ExecutorOptions } from "@oh-my-pi/pi-coding-agent/task/executor";

import { execFileSync } from "node:child_process";
import { createRequire } from "node:module";
import { resolve } from "node:path";
import { describe, expect, it, vi } from "vite-plus/test";

import type { OmpWorkflowAgentDependencies } from "./agent.js";

// `agent.ts` (imported transitively) eagerly imports OMP's `/task` and
// `/task/executor` entrypoints, which load `pi-natives` — impossible under
// vitest. Mock those seams so the extension keeps its production imports.
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
        constructor(_getArtifactsDir: () => string | null) {}
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

import dynamicWorkflows, { WORKFLOW_RESULT_MESSAGE } from "./extension.js";

const TASK: AgentDefinition = {
    name: "task",
    description: "General worker",
    systemPrompt: "You do tasks.",
    source: "bundled",
};

const SCRIPT = `export const meta = {
  name: "metadata_scan",
  description: "Summarize repository metadata",
};
return await agent("inspect", { label: "inspect repo" });`;

function makeResult(overrides: Partial<SingleResult> = {}): SingleResult {
    return {
        index: 0,
        id: "InspectRepo",
        agent: "task",
        agentSource: "bundled",
        task: "",
        exitCode: 0,
        output: "findings",
        stderr: "",
        truncated: false,
        durationMs: 0,
        tokens: 0,
        requests: 0,
        ...overrides,
    };
}

interface Harness {
    tools: Map<string, ToolDefinition<any, any>>;
    handlers: Map<string, Array<() => void>>;
    messages: Array<{ message: Record<string, unknown>; options?: Record<string, unknown> }>;
    /** Resolves on the first delivered message. */
    delivered: Promise<void>;
}

function load(dependencies: Partial<OmpWorkflowAgentDependencies>): Harness {
    let tools = new Map<string, ToolDefinition<any, any>>();
    let handlers = new Map<string, Array<() => void>>();
    let messages: Harness["messages"] = [];
    let firstMessage = Promise.withResolvers<void>();
    let pi = {
        zod: piZod,
        pi: { Text: PiText },
        registerTool: (tool: ToolDefinition<any, any>) => tools.set(tool.name, tool),
        on: (event: string, handler: () => void) => {
            handlers.set(event, [...(handlers.get(event) ?? []), handler]);
        },
        sendMessage: (message: Record<string, unknown>, options?: Record<string, unknown>) => {
            messages.push({ message, options });
            firstMessage.resolve(undefined);
        },
    } as unknown as ExtensionAPI;

    dynamicWorkflows(pi, { settings: () => ({}) as never, dependencies });
    return { tools, handlers, messages, delivered: firstMessage.promise };
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

let context = {
    cwd: "/repo",
    sessionManager: { getArtifactsDir: () => null, getArtifactManager: () => null },
    modelRegistry: {},
    agent: { kind: "main", id: "Main", name: "main", depth: 0 },
} as unknown as ExtensionContext;

describe("dynamicWorkflows extension", () => {
    it("registers exactly one workflow tool and its run control tool", () => {
        let { tools } = load(makeDeps());

        expect([...tools.keys()].sort()).toEqual(["workflow", "workflow_runs"]);
        expect(tools.get("workflow")?.loadMode).toBe("essential");
    });

    it("wakes the session with one workflow-result message when a run finishes", async () => {
        let release = Promise.withResolvers<void>();
        let { tools, messages, delivered } = load(
            makeDeps(async () => {
                await release.promise;
                return makeResult({ output: "findings" });
            }),
        );

        let started = await tools
            .get("workflow")!
            .execute("call-1", { script: SCRIPT }, undefined, undefined, context);
        // The call returned while the only agent is still gated.
        expect(messages).toHaveLength(0);
        expect(started.content[0]).toMatchObject({ type: "text" });
        release.resolve(undefined);
        await delivered;

        expect(messages).toHaveLength(1);
        let [{ message, options }] = messages;
        expect(message.customType).toBe(WORKFLOW_RESULT_MESSAGE);
        expect(message.display).toBe(true);
        expect(message.attribution).toBe("agent");
        expect(String(message.content)).toContain("Workflow metadata_scan (wf-1) finished");
        expect(String(message.content)).toContain("findings");
        // `followUp` enters the agent's queue, so a busy session continues into the
        // result instead of settling without it; `triggerTurn` wakes an idle one.
        expect(options).toEqual({ deliverAs: "followUp", triggerTurn: true });
    });

    it("cancels live runs on session shutdown", async () => {
        let { tools, handlers, messages } = load(
            makeDeps(async () => {
                await new Promise<void>(() => {}); // in-flight until shutdown
                return makeResult();
            }),
        );

        // A blocking run settles exactly when the shutdown abort reaches it, which
        // is the observable the background path shares (see runs.test.ts).
        let running = tools
            .get("workflow")!
            .execute(
                "call-1",
                { script: SCRIPT, background: false },
                undefined,
                undefined,
                context,
            );
        for (let handler of handlers.get("session_shutdown") ?? []) handler();

        let result = await running;
        expect(result.isError).toBe(true);
        expect(textContent(result.content[0])).toBe("Workflow was aborted");
        let listed = await tools
            .get("workflow_runs")!
            .execute("call-2", { action: "list" }, undefined, undefined, context);
        expect(textContent(listed.content[0])).toContain("wf-1  cancelled");
        expect(messages).toHaveLength(0);
    });
});

describe("OMP extension discovery (real loader boundary)", () => {
    // OMP's public `loadExtensions` transitively pulls in the Bun runtime
    // (`import { YAML } from "bun"`, `bun:ffi`, native addons) through pi-utils and
    // the task executor — the same coupling the unit tests above mock away. Node's
    // vitest worker cannot evaluate that graph, so to exercise the *real* loader
    // against the *real* source path we run it in the runtime OMP actually uses
    // (Bun) and assert on its structured result.
    it("loads the extension and registers both tools exactly once", () => {
        let cwd = process.cwd();
        let extensionPath = resolve(".omp/extensions/dynamic-workflows.ts");
        let script = [
            `import { loadExtensions } from "@oh-my-pi/pi-coding-agent";`,
            `import { existsSync } from "node:fs";`,
            `const loaded = await loadExtensions([${JSON.stringify(extensionPath)}], ${JSON.stringify(cwd)});`,
            `process.stdout.write(JSON.stringify({`,
            `  errors: loaded.errors,`,
            `  tools: loaded.extensions.flatMap(ext => [...ext.tools.keys()]),`,
            // `.omp/tools/<dir>/index.ts` is what custom-tool discovery loads; its
            // absence is what keeps the extension's `workflow` tool unique.
            `  customToolEntry: existsSync(".omp/tools/dynamic-workflows/index.ts"),`,
            `}));`,
        ].join("\n");

        let out = execFileSync("bun", ["-e", script], {
            cwd,
            encoding: "utf8",
            killSignal: "SIGKILL",
            timeout: 30_000,
        });
        let loaded = JSON.parse(out) as {
            errors: unknown[];
            tools: string[];
            customToolEntry: boolean;
        };

        expect(loaded.errors).toEqual([]);
        expect(loaded.tools.sort()).toEqual(["workflow", "workflow_runs"]);
        expect(loaded.customToolEntry).toBe(false);
    }, 35_000);
});

function textContent(block: { type: string } | undefined): string {
    return block && block.type === "text" && "text" in block ? String(block.text) : "";
}
