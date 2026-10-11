// Ported from pi-dynamic-workflows v1.0.1, commit 31b2aca0f1cb195aafbfc5e3ee2b8c83ad3f21a2.
// Bridges the deterministic workflow runtime to OMP's native subagent executor.
import type { CustomToolContext } from "@oh-my-pi/pi-coding-agent";
import type { DiscoveryResult } from "@oh-my-pi/pi-coding-agent/task/discovery";

import { getActiveSkills } from "@oh-my-pi/pi-coding-agent";
import { MCPManager } from "@oh-my-pi/pi-coding-agent/mcp/manager";
import {
    AgentOutputManager,
    discoverAgents,
    getAgent,
    type AgentDefinition,
} from "@oh-my-pi/pi-coding-agent/task";
import { runSubprocess } from "@oh-my-pi/pi-coding-agent/task/executor";
import {
    cfgTaskAgentModelOverrides,
    cfgTaskDisabledAgents,
} from "@oh-my-pi/pi-coding-agent/task/settings";
import { buildOutputValidator } from "@oh-my-pi/pi-coding-agent/tools/output-schema-validator";

import type { WorkflowCoordination } from "./runs.js";

import { agentIdFromLabel, appendIrcBlock } from "./irc.js";
import {
    WorkflowAgentFailure,
    type WorkflowAgentOutcome,
    type WorkflowAgentRequest,
    type WorkflowAgentRunner,
} from "./workflow.js";

// Tools a workflow subagent may keep. Everything else — notably `task` itself —
// is stripped so workflow agents cannot recursively spawn.
const SAFE_TOOLS = [
    "read",
    "bash",
    "edit",
    "write",
    "grep",
    "glob",
    "lsp",
    "ast_edit",
    "eval",
    "debug",
    "browser",
    "web_search",
] as const;

/**
 * Host calls the adapter depends on, injectable so tests replace only the
 * process-touching functions while the adapter's own logic runs unchanged.
 * Production defaults bind the real OMP implementations.
 */
export interface OmpWorkflowAgentDependencies {
    discoverAgents: typeof discoverAgents;
    getActiveSkills: typeof getActiveSkills;
    runSubprocess: typeof runSubprocess;
}

const DEFAULT_DEPENDENCIES: OmpWorkflowAgentDependencies = Object.freeze({
    discoverAgents,
    getActiveSkills,
    runSubprocess,
});

/**
 * The session surface the adapter reads. A custom-tool context and an extension
 * context both satisfy it; the extension supplies `settings` itself, since its
 * context does not carry one.
 */
export interface WorkflowSessionContext {
    sessionManager: CustomToolContext["sessionManager"];
    modelRegistry: CustomToolContext["modelRegistry"];
    settings?: CustomToolContext["settings"];
    localProtocolOptions?: CustomToolContext["localProtocolOptions"];
}

export interface OmpWorkflowAgentOptions {
    cwd: string;
    context: WorkflowSessionContext;
    /** Tool call that started the run, so child rows nest under it in the UI. */
    parentToolCallId?: string;
    dependencies?: Partial<OmpWorkflowAgentDependencies>;
    /**
     * Id allocator shared across the session's runs. Concurrent runs must draw
     * from one allocator or two agents could claim the same registry id.
     */
    outputManager?: AgentOutputManager;
    /** Peer and coordinator identities for the IRC block in each agent prompt. */
    coordination?: WorkflowCoordination;
}

/**
 * Runs each workflow agent request as an in-process OMP subagent. Resolves the
 * requested profile, hardens it against recursion, forwards session context to
 * `runSubprocess`, and normalizes the result into a {@link WorkflowAgentOutcome}
 * (or a {@link WorkflowAgentFailure} carrying the run's token count).
 */
export class OmpWorkflowAgent implements WorkflowAgentRunner {
    readonly #cwd: string;
    readonly #context: WorkflowSessionContext;
    readonly #parentToolCallId: string | undefined;
    readonly #dependencies: OmpWorkflowAgentDependencies;
    readonly #outputManager: AgentOutputManager;
    readonly #coordination: WorkflowCoordination | undefined;
    #index = 0;
    // Serializes per-instance ID allocation so the manager's initial disk scan
    // completes before the next allocation runs — concurrent runs then never
    // reuse an existing artifact id.
    #allocationChain: Promise<unknown> = Promise.resolve();
    // Every id handed out so far. A caller awaits these before reading the peer
    // table: a consumer's own reservation callback is attached to the same
    // promise first, so by then the peer rows carry their ids.
    readonly #reservations: Array<Promise<string>> = [];
    // A fresh, unconnected MCP manager handed to every child run. The executor
    // reads a supplied manager as "MCP already provided" (sets enableMCP = false),
    // so the child never rediscovers .mcp.json servers, and an empty manager makes
    // createMCPProxyTools() return nothing — the public way to disable MCP for a
    // subagent. An explicit safe `agent.tools` list cannot do this: OMP always
    // includes custom/extension tools (MCP tools among them) regardless of the
    // tool allowlist.
    readonly #mcpManager: MCPManager;
    // One discovery snapshot per instance so every request in a workflow resolves
    // against a single, stable agent-profile set instead of rescanning each call.
    #discovery: Promise<DiscoveryResult> | undefined;

    constructor(options: OmpWorkflowAgentOptions) {
        this.#cwd = options.cwd;
        this.#context = options.context;
        this.#parentToolCallId = options.parentToolCallId;
        this.#dependencies = { ...DEFAULT_DEPENDENCIES, ...options.dependencies };
        this.#coordination = options.coordination;
        this.#mcpManager = new MCPManager(options.cwd);
        this.#outputManager =
            options.outputManager ??
            new AgentOutputManager(() => options.context.sessionManager.getArtifactsDir());
    }

    /**
     * Claim this agent's registry id from its label while the agent is queued.
     * Returns the allocation promise itself so a consumer that attached to it
     * earlier (to record the id) always observes the reservation first.
     */
    reserve(request: { label: string }): Promise<string> {
        let allocated = this.#allocateId(agentIdFromLabel(request.label));
        this.#reservations.push(allocated);
        return allocated;
    }

    /** Resolve once every reservation issued so far has been recorded. */
    async settleReservations(): Promise<void> {
        await Promise.allSettled(this.#reservations);
    }

    async run(request: WorkflowAgentRequest): Promise<WorkflowAgentOutcome> {
        let context = this.#context;
        let dependencies = this.#dependencies;

        let { agents } = await this.#discoverAgents();
        let profile = getAgent(agents, request.agent);
        if (!profile) {
            let available = agents.map(agent => agent.name).join(", ") || "none";
            throw new Error(`Unknown agent "${request.agent}". Available: ${available}`);
        }

        let settings = context.settings;
        let disabledAgents = settings ? cfgTaskDisabledAgents.get(settings) : [];
        if (disabledAgents.includes(profile.name)) {
            throw new Error(
                `Agent "${profile.name}" is disabled in settings. Enable it via /agents, or use a different agent type.`,
            );
        }

        // Clone the profile, disable recursive spawns, and constrain tools to the
        // safe allowlist (full list when the profile declares none, otherwise the
        // intersection preserving the profile's ordering).
        let tools = profile.tools
            ? profile.tools.filter(tool => (SAFE_TOOLS as readonly string[]).includes(tool))
            : [...SAFE_TOOLS];
        // A profile that declares tools but shares none with the safe allowlist must
        // be rejected: an empty `tools` array reads to the executor as "no explicit
        // whitelist", which silently re-expands to the full OMP default tool set —
        // the opposite of the intended restriction.
        if (profile.tools && tools.length === 0) {
            throw new Error(
                `Agent "${profile.name}" declares no workflow-safe tools; all of its tools are restricted from workflow subagents. Use a different agent type.`,
            );
        }
        let safeProfile: AgentDefinition = { ...profile, spawns: undefined, tools };

        let settingsModels = settings ? cfgTaskAgentModelOverrides.get(settings) : {};
        let modelOverride = request.model ?? settingsModels[profile.name];
        let index = ++this.#index;
        let id = request.agentId ?? (await this.reserve(request));
        let prompt = await this.#promptWithIrcBlock(request.prompt, id);

        let activeSkills = dependencies.getActiveSkills();
        let autoloadNames = safeProfile.autoloadSkills ?? [];
        let autoloadSkills =
            autoloadNames.length > 0 && activeSkills.length > 0
                ? autoloadNames
                      .map(name => activeSkills.find(skill => skill.name === name))
                      .filter((skill): skill is NonNullable<typeof skill> => skill !== undefined)
                : [];

        let result = await dependencies.runSubprocess({
            cwd: this.#cwd,
            agent: safeProfile,
            task: prompt,
            assignment: request.prompt,
            description: request.label,
            parentToolCallId: this.#parentToolCallId,
            index,
            id,
            modelOverride,
            outputSchema: request.schema,
            outputSchemaOverridesAgent: request.schema !== undefined,
            signal: request.signal,
            onProgress: progress =>
                request.onProgress?.(
                    progress.lastIntent ?? progress.currentTool ?? progress.status,
                ),
            modelRegistry: context.modelRegistry,
            settings: context.settings,
            skills: [...activeSkills],
            autoloadSkills,
            localProtocolOptions: context.localProtocolOptions,
            parentArtifactManager: context.sessionManager.getArtifactManager() ?? undefined,
            artifactsDir: context.sessionManager.getArtifactsDir() ?? undefined,
            // Child isolation: hand the child empty preloaded path lists so it skips
            // its own `.omp/tools/` and extension FS scans and cannot rediscover the
            // `workflow` tool (recursion guard) or any extension tools outside the
            // safe allowlist, and a fresh empty MCP manager so it neither rediscovers
            // .mcp.json servers nor inherits any (see #mcpManager).
            preloadedCustomToolPaths: [],
            preloadedExtensionPaths: [],
            mcpManager: this.#mcpManager,
            keepAlive: false,
        });

        // Cancellation via the shared workflow signal wins over failure reporting so
        // an aborted run reads as a workflow abort, not an agent failure.
        if (request.signal?.aborted) {
            throw new Error("Workflow was aborted");
        }
        if (result.exitCode !== 0 || result.aborted) {
            let message =
                result.error ||
                result.stderr ||
                `Agent "${profile.name}" exited with code ${result.exitCode}`;
            throw new WorkflowAgentFailure(message, result.tokens);
        }

        if (request.schema !== undefined) {
            // The executor emits a JSON document for object/array/quoted outputs, but
            // may emit unquoted text for a schema that accepts a raw string. Validate
            // the decoded candidate against OMP's normalized validator (an
            // unconstrained schema has no validator and accepts anything): prefer the
            // parsed value when it is valid (quoted string -> string, union object
            // JSON -> object), otherwise fall back to the raw text when that validates
            // (e.g. an unquoted `123` under a string schema stays the string "123").
            let { validator } = buildOutputValidator(request.schema);
            let accepts = (value: unknown): boolean =>
                !validator || validator.validate(value).success;
            let parsed: unknown;
            let parsedOk = false;
            let parseError: unknown;
            try {
                parsed = JSON.parse(result.output);
                parsedOk = true;
            } catch (error) {
                parseError = error;
            }
            if (parsedOk && accepts(parsed)) {
                return { value: parsed, tokens: result.tokens };
            }
            if (accepts(result.output)) {
                return { value: result.output, tokens: result.tokens };
            }
            let message =
                parseError instanceof Error
                    ? parseError.message
                    : `Agent "${profile.name}" output did not satisfy the requested schema`;
            throw new WorkflowAgentFailure(message, result.tokens);
        }

        return { value: result.output, tokens: result.tokens };
    }

    /**
     * Append the run's IRC block to a prompt. Waits for the reservations issued
     * so far to settle first: the agents of a wave are all queued in one tick, so
     * this is what lets each of them name the others.
     */
    async #promptWithIrcBlock(prompt: string, selfId: string): Promise<string> {
        let coordination = this.#coordination;
        if (!coordination) return prompt;
        await this.settleReservations();
        let { peers, foreignPeers } = coordination.peers(selfId);
        return appendIrcBlock(prompt, {
            selfId,
            coordinatorId: coordination.coordinatorId,
            runId: coordination.runId,
            runName: coordination.runName,
            peers,
            foreignPeers,
        });
    }

    /**
     * Resolve the agent-profile set once per instance and reuse that promise for
     * every request, so all agents in one workflow see one stable snapshot even
     * when several requests race the first discovery.
     */
    #discoverAgents(): Promise<DiscoveryResult> {
        return (this.#discovery ??= this.#dependencies.discoverAgents(this.#cwd));
    }

    /**
     * Serialize allocation through a per-instance promise chain so the manager's
     * lazy disk scan runs to completion before the next allocation, keeping ids
     * unique across concurrent runs. Uniqueness stays owned by the manager.
     */
    #allocateId(base: string): Promise<string> {
        let allocated = this.#allocationChain.then(() => this.#outputManager.allocate(base));
        // Keep the chain alive across a rejected allocation so later runs still queue.
        this.#allocationChain = allocated.then(
            () => undefined,
            () => undefined,
        );
        return allocated;
    }
}
