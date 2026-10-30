import { mkdtemp, readFile, readdir, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { afterEach, expect, test } from "vitest";

import type { RunRecord } from "./results-model.ts";

import { timestampName, writeResults } from "./results.ts";

let directories: string[] = [];

afterEach(async () => {
    await Promise.all(directories.map(directory => rm(directory, { recursive: true })));
    directories = [];
});

function runRecord(): RunRecord {
    return {
        startedAt: "2026-10-10T23:00:00.000Z",
        options: { runtimes: ["node"], iterations: 2 },
        environment: {
            hardware: "Mac16,1, Apple M4, 10 cores, 32 GiB",
            os: "macOS 27.0.1 (arm64)",
            runtimes: { node: "v26.11.1" },
            tools: { pnpm: "12.11.2" },
            revisions: { bento: "abc1234" },
            remixVersion: "3.0.0",
        },
        workloads: [
            {
                workload: "memory",
                title: "Memory",
                question: "How much?",
                metrics: [{ key: "idleRssMb", label: "Idle RSS", unit: "MB" }],
                notes: [],
                series: [
                    {
                        runtime: "node",
                        variant: "production",
                        outcome: {
                            status: "measured",
                            failures: [],
                            samples: [{ trial: 0, idleRssMb: 180.5, detail: { samples: [1, 2] } }],
                        },
                    },
                ],
            },
        ],
    };
}

test("a run is written as raw JSON per workload plus a summary", async () => {
    let directory = await mkdtemp(path.join(tmpdir(), "bento-bench-results-"));
    directories.push(directory);

    let summaryPath = await writeResults(directory, runRecord());

    expect((await readdir(directory)).sort()).toEqual(["memory.json", "run.json", "summary.md"]);
    expect(summaryPath).toBe(path.join(directory, "summary.md"));
    expect(await readFile(summaryPath, "utf8")).toContain("| Idle RSS (MB) | 180.5 (");
});

test("every raw result file carries the environment and options that produced it", async () => {
    let directory = await mkdtemp(path.join(tmpdir(), "bento-bench-results-"));
    directories.push(directory);

    await writeResults(directory, runRecord());

    for (let file of ["run.json", "memory.json"]) {
        let raw = JSON.parse(await readFile(path.join(directory, file), "utf8"));
        expect(raw.environment.hardware).toBe("Mac16,1, Apple M4, 10 cores, 32 GiB");
        expect(raw.environment.remixVersion).toBe("3.0.0");
        expect(raw.options).toEqual({ runtimes: ["node"], iterations: 2 });
    }
    let memory = JSON.parse(await readFile(path.join(directory, "memory.json"), "utf8"));
    expect(memory.series[0].outcome.samples[0].detail).toEqual({ samples: [1, 2] });
});

test("each runtime's gate outcome is in its workload's raw file, not in run.json", async () => {
    let directory = await mkdtemp(path.join(tmpdir(), "bento-bench-results-"));
    directories.push(directory);
    let run = runRecord();
    run.workloads[0]!.series.push({
        runtime: "bun",
        variant: "production",
        outcome: {
            status: "incompatible",
            check: "document",
            failures: ["response body is empty"],
            evidence: [],
        },
    });

    await writeResults(directory, run);

    let header = JSON.parse(await readFile(path.join(directory, "run.json"), "utf8"));
    let memory = JSON.parse(await readFile(path.join(directory, "memory.json"), "utf8"));
    expect(Object.keys(header).sort()).toEqual(["environment", "options", "startedAt"]);
    expect(memory.series[1].outcome).toMatchObject({ status: "incompatible", check: "document" });
});

test("result directories are named by UTC timestamp", () => {
    expect(timestampName(new Date("2026-10-10T23:05:09.123Z"))).toBe("2026-10-10T23-05-09Z");
});
