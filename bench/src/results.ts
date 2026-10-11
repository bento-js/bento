import { mkdir, writeFile } from "node:fs/promises";
import path from "node:path";

import type { RunRecord } from "./results-model.ts";

import { renderSummary } from "./summary.ts";

/** `2026-10-10T23-00-00Z`: an ISO timestamp that is also a safe directory name. */
export function timestampName(date: Date): string {
    return `${date.toISOString().slice(0, 19).replaceAll(":", "-")}Z`;
}

/** Writes `run.json`, per-workload raw JSON (each with the environment), and `summary.md`. */
export async function writeResults(directory: string, run: RunRecord): Promise<string> {
    await mkdir(directory, { recursive: true });
    let { workloads, ...header } = run;
    await writeJson(path.join(directory, "run.json"), header);
    for (let workload of workloads) {
        let raw = { ...header, ...workload };
        await writeJson(path.join(directory, `${workload.workload}.json`), raw);
    }
    let summaryPath = path.join(directory, "summary.md");
    await writeFile(summaryPath, `${renderSummary(run)}\n`);
    return summaryPath;
}

async function writeJson(file: string, value: unknown): Promise<void> {
    await writeFile(file, `${JSON.stringify(value, null, 2)}\n`);
}
