import type { LoadResult, LoadSpec } from "../load-generator.ts";
import type { Metric, SampleRow } from "../results-model.ts";
import type { RunContext, Workload } from "../workload.ts";

import {
    STARTUP_TIMEOUT_MS,
    waitForDocument,
    waitUntilListening,
    withServer,
} from "../app-server.ts";
import { countEntries } from "../database.ts";
import { entryScriptUrl } from "../document-gate.ts";
import { runLoad } from "../load-generator.ts";
import { SEED_ENTRIES } from "../seed-entries.ts";

const SCENARIOS = [
    { key: "get", label: "Rendered GET /", successStatus: "200" },
    { key: "asset", label: "Asset GET", successStatus: "200" },
    { key: "post", label: "Persisted POST /", successStatus: "303" },
] as const;

type ScenarioKey = (typeof SCENARIOS)[number]["key"];
type Target = Pick<LoadSpec, "form" | "method" | "url">;

const LOAD_FORM = new URLSearchParams({
    name: "Load Tester",
    message: "Signed under steady load.",
}).toString();

let metrics: Metric[] = SCENARIOS.flatMap(({ key, label }) => [
    { key: `${key}Rps`, label: `${label}, throughput`, unit: "req/s" },
    { key: `${key}P50Ms`, label: `${label}, latency p50`, unit: "ms" },
    { key: `${key}P99Ms`, label: `${label}, latency p99`, unit: "ms" },
]);

export const steadyState: Workload = {
    name: "steady-state",
    title: "Steady-state load",
    question:
        "How many requests per second, at what latency, does a runtime sustain at fixed concurrency after a warmup, for a rendered page, an asset, and a persisted valid submission? Responses must keep their expected status and persisted rows must be counted.",
    metrics,
    variants: [{ label: "production", mode: "production" }],
    requiresApplicationGate: true,
    async runTrial({ runtime, executable, fixture, mode, trial, context }) {
        let ohaExecutable = context.ohaExecutable;
        if (ohaExecutable === undefined) throw new Error("oha is required for steady-state load");
        let request = { runtime, executable, fixture, mode, watch: false };
        return withServer(request, async server => {
            await waitUntilListening(server, STARTUP_TIMEOUT_MS);
            let page = await waitForDocument(server, SEED_ENTRIES, STARTUP_TIMEOUT_MS);
            let assetPath = entryScriptUrl(page.snapshot.body);
            if (assetPath === undefined) throw new Error("the page has no entry script to request");

            let targets: Record<ScenarioKey, Target> = {
                get: { method: "GET", url: `${server.baseUrl}/` },
                asset: { method: "GET", url: `${server.baseUrl}${assetPath}` },
                post: { method: "POST", url: `${server.baseUrl}/`, form: LOAD_FORM },
            };
            let row: SampleRow = { trial };
            let acceptedPosts = 0;
            for (let { key, successStatus } of SCENARIOS) {
                let { warmup, measured } = await loadScenario(ohaExecutable, context, targets[key]);
                requireExpectedOutcome(key, successStatus, warmup, measured);
                Object.assign(row, flatten(key, measured), { [`${key}Load`]: measured });
                if (key === "post") acceptedPosts = requestsWithStatus(warmup, measured, "303");
            }
            requirePersistence(fixture.directory, acceptedPosts);
            return row;
        });
    },
};

async function loadScenario(
    ohaExecutable: string,
    context: RunContext,
    target: Target,
): Promise<{ warmup: LoadResult; measured: LoadResult }> {
    let { concurrency, warmupSeconds, durationSeconds } = context.options.steadyState;
    let spec = { ...target, ohaExecutable, concurrency };
    let warmup = await runLoad({ ...spec, durationSeconds: warmupSeconds });
    let measured = await runLoad({ ...spec, durationSeconds });
    return { warmup, measured };
}

function flatten(key: ScenarioKey, load: LoadResult): Record<string, number> {
    return {
        [`${key}Rps`]: load.requestsPerSecond,
        [`${key}P50Ms`]: load.latencyMs.p50,
        [`${key}P99Ms`]: load.latencyMs.p99,
    };
}

function requestsWithStatus(warmup: LoadResult, measured: LoadResult, status: string): number {
    return (warmup.statusCodes[status] ?? 0) + (measured.statusCodes[status] ?? 0);
}

function requireExpectedOutcome(
    key: ScenarioKey,
    successStatus: string,
    warmup: LoadResult,
    measured: LoadResult,
): void {
    for (let load of [warmup, measured]) {
        let statuses = Object.keys(load.statusCodes);
        let failed = Object.keys(load.errors).length > 0;
        if (statuses.some(status => status !== successStatus) || statuses.length === 0 || failed) {
            throw new Error(
                `${key} load did not return only ${successStatus}: statuses ${JSON.stringify(load.statusCodes)}, errors ${JSON.stringify(load.errors)}`,
            );
        }
    }
}

function requirePersistence(fixtureDirectory: string, acceptedPosts: number): void {
    let expected = SEED_ENTRIES.length + acceptedPosts;
    let stored = countEntries(fixtureDirectory);
    if (stored !== expected) {
        throw new Error(
            `${acceptedPosts} submissions were accepted but the database holds ${stored} entries, expected ${expected}`,
        );
    }
}
