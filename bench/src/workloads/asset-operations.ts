import type { SampleRow, Series } from "../results-model.ts";
import type { Workload } from "../workload.ts";

import { readArray, readNumber, readString } from "../json-values.ts";
import { baseEnvironment, scriptCommand } from "../launch-commands.ts";
import { runToCompletion } from "../processes.ts";
import { median } from "../statistics.ts";

const OPERATIONS = [
    { key: "parse", label: "Parse (oxc-parser)" },
    { key: "resolve", label: "Resolve (oxc-resolver)" },
    { key: "transform", label: "Transform (oxc-transform)" },
    { key: "minify", label: "Minify (oxc-minify)" },
    { key: "css", label: "CSS (lightningcss)" },
] as const;

export const assetOperations: Workload = {
    name: "asset-operations",
    title: "Asset operations",
    question:
        "Where does time go inside the asset path? Each operation is timed over the fixture's own files through the locked npm packages with the options remix/assets uses. The cold pass is the first, with empty caches; warm is the median of the later passes. This workload does not render the application.",
    metrics: OPERATIONS.flatMap(({ key, label }) => [
        { key: `${key}ColdMs`, label: `${label}, cold pass`, unit: "ms" },
        { key: `${key}WarmMs`, label: `${label}, warm pass (median)`, unit: "ms" },
    ]),
    variants: [{ label: "", mode: "production" }],
    requiresApplicationGate: false,
    async runTrial({ runtime, executable, fixture, trial, context }) {
        let iterations = context.options.iterations;
        let command = scriptCommand(runtime, executable, ".bench/probe.ts", [String(iterations)]);
        let stdout = await runToCompletion(command, {
            cwd: fixture.directory,
            env: baseEnvironment(),
            label: `${runtime} asset operations probe`,
        });
        return rowFromReport(trial, JSON.parse(lastLine(stdout)));
    },
    notes: digestNotes,
};

function lastLine(text: string): string {
    let lines = text.trim().split("\n");
    return lines[lines.length - 1] ?? "";
}

function rowFromReport(trial: number, report: unknown): SampleRow {
    let passes = readArray(report, "passes");
    let digests: Record<string, string> = {};
    let row: SampleRow = {
        trial,
        scripts: readNumber(report, "scripts"),
        styles: readNumber(report, "styles"),
        bytes: readNumber(report, "bytes"),
        digests,
    };
    for (let { key } of OPERATIONS) {
        let durations = passes.map(pass => readNumber(pass, key, "ms"));
        row[`${key}ColdMs`] = durations[0];
        if (durations.length > 1) row[`${key}WarmMs`] = median(durations.slice(1));
        digests[key] = readString(passes[0], key, "digest");
    }
    return row;
}

/** The same inputs and options must yield the same outputs under every runtime. */
function digestNotes(series: Series[]): string[] {
    let digestsByRuntime = new Map<string, Set<string>>();
    for (let { runtime, outcome } of series) {
        if (outcome.status !== "measured") continue;
        let digests = outcome.samples.map(sample => JSON.stringify(sample.digests));
        digestsByRuntime.set(runtime, new Set(digests));
    }
    if (digestsByRuntime.size === 0) return [];
    let distinct = new Set([...digestsByRuntime.values()].flatMap(digests => [...digests]));
    if (distinct.size === 1) {
        return [
            `Every measured runtime produced identical outputs for ${OPERATIONS.map(({ key }) => key).join(", ")} (output digests match).`,
        ];
    }
    let detail = [...digestsByRuntime].map(
        ([runtime, digests]) => `${runtime}: ${[...digests].join(" | ")}`,
    );
    return [`Output digests differ, so the timed work was not identical: ${detail.join("; ")}`];
}
