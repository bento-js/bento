import type { ResultStore, RunReport, RunSummary } from "../../lib/model.ts";

import { compareRuns } from "../../lib/comparison.ts";

export interface LatestRun {
    runs: RunSummary[];
    report: RunReport | null;
}

export async function loadLatest(store: ResultStore, kind: RunReport["kind"]): Promise<LatestRun> {
    let runs = await store.list();
    let latest = runs.find(run => run.kind === kind);
    return { runs, report: latest === undefined ? null : await store.get(latest.id) };
}

export type PreviousComparison =
    | { kind: "incomplete" }
    | { kind: "none" }
    | { kind: "incomparable"; previous: RunSummary; reason: string }
    | { kind: "compared"; previous: RunSummary; regressions: number; improvements: number };

export async function comparePrevious(
    store: ResultStore,
    report: RunReport,
    runs: readonly RunSummary[],
): Promise<PreviousComparison> {
    if (report.state !== "complete") return { kind: "incomplete" };
    let candidate = runs.find(
        run =>
            run.id !== report.id &&
            run.kind === "behavior" &&
            run.state === "complete" &&
            run.startedAt < report.startedAt &&
            run.suiteRevision === report.suite.revision &&
            run.filter === report.selection.filter &&
            run.platform === report.platform &&
            run.arch === report.arch,
    );
    if (candidate === undefined) return { kind: "none" };
    let previous = await store.get(candidate.id);
    if (previous === null) return { kind: "none" };
    let comparison = compareRuns(report, previous);
    if (!comparison.comparable) {
        return {
            kind: "incomparable",
            previous: candidate,
            reason: comparison.reason ?? "The runs were recorded under different conditions.",
        };
    }
    return {
        kind: "compared",
        previous: candidate,
        regressions: comparison.regressions.length,
        improvements: comparison.improvements.length,
    };
}
