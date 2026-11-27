import { randomBytes } from "node:crypto";

import type { ResultStore, RunReport } from "../lib/model.ts";

import { describeError } from "./errors.ts";

export interface ReportInit {
    kind: RunReport["kind"];
    suite: RunReport["suite"];
    runtimes: RunReport["runtimes"];
    timeoutMs: number;
    selection: RunReport["selection"];
}

export interface RecordOptions {
    signal: AbortSignal;
    collect(report: RunReport): Promise<void>;
    verify(): Promise<string | null>;
}

export function createReport(init: ReportInit, now = new Date()): RunReport {
    let startedAt = now.toISOString();
    return {
        schemaVersion: 1,
        id: `${startedAt.replace(/[:.]/g, "-")}-${init.kind}-${randomBytes(3).toString("hex")}`,
        kind: init.kind,
        state: "running",
        startedAt,
        finishedAt: null,
        suite: init.suite,
        platform: process.platform,
        arch: process.arch,
        timeoutMs: init.timeoutMs,
        runtimes: init.runtimes,
        selection: init.selection,
        behavior: [],
        inventory: [],
        error: null,
    };
}

export async function recordRun(
    store: ResultStore,
    report: RunReport,
    options: RecordOptions,
): Promise<RunReport> {
    await store.save(report);
    try {
        await options.collect(report);
        let changed = await options.verify();
        if (changed === null) {
            report.state = options.signal.aborted ? "interrupted" : "complete";
        } else {
            report.state = "failed";
            report.error = changed;
        }
    } catch (error) {
        report.state = "failed";
        report.error = describeError(error);
    }
    report.finishedAt = new Date().toISOString();
    await store.save(report);
    return report;
}
