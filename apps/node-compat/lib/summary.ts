import type { RunReport, RunSummary } from "./model.ts";

export function summarizeRun(report: RunReport): RunSummary {
    let recorded = report.kind === "behavior" ? report.behavior.length : report.inventory.length;
    let summary: RunSummary = {
        id: report.id,
        kind: report.kind,
        state: report.state,
        startedAt: report.startedAt,
        finishedAt: report.finishedAt,
        nodeVersion: report.runtimes.node.version,
        bentoVersion: report.runtimes.bento.version,
        suiteRevision: report.suite.revision,
        platform: report.platform,
        arch: report.arch,
        filter: report.selection.filter,
        total: report.selection.total,
        selected: report.selection.selected,
        recorded,
        passed: 0,
        failed: 0,
        skipped: 0,
        unsupported: 0,
        timeout: 0,
        crashed: 0,
        notRun: Math.max(0, report.selection.selected - recorded),
        baselineFailed: 0,
        available: 0,
        missing: 0,
        typeMismatch: 0,
        inspectionErrors: 0,
    };
    for (let test of report.behavior) {
        if (test.node.status !== "passed") {
            summary.baselineFailed++;
            continue;
        }
        if (test.bento.status === "not-run") summary.notRun++;
        else summary[test.bento.status]++;
    }
    for (let entry of report.inventory) {
        if (entry.status === "present") summary.available++;
        else if (entry.status === "type-mismatch") summary.typeMismatch++;
        else if (entry.status === "error") summary.inspectionErrors++;
        else summary.missing++;
    }
    return summary;
}
