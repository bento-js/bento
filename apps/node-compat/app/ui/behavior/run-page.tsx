import type { Handle } from "remix/component";

import type { CaseFilter } from "#app/ui/behavior/results.ts";
import type { ResultQuery } from "#app/ui/query.ts";
import type { PreviousComparison } from "#app/utils/runs.ts";

import { routes } from "#app/routes.ts";
import { OutcomeSummary, Scope } from "#app/ui/behavior/outcome-summary.tsx";
import {
    CASE_FILTER_OPTIONS,
    compareTests,
    matchesCase,
    summarizeModules,
} from "#app/ui/behavior/results.ts";
import { ModuleTable, ResultsTable } from "#app/ui/behavior/tables.tsx";
import { Document } from "#app/ui/document.tsx";
import { FilterForm, Pagination } from "#app/ui/filters.tsx";
import { formatCount, formatTimestamp, withSearch } from "#app/ui/format.ts";
import { PageHeader, proseStyle, sectionStyle, stackStyle } from "#app/ui/layout.tsx";
import { paginate } from "#app/ui/query.ts";
import { RunConditions, RunMeta, StateNotice } from "#app/ui/run-conditions.tsx";

import type { RunReport } from "../../../lib/model.ts";

import { summarizeRun } from "../../../lib/summary.ts";

export interface BehaviorRunPageProps {
    report: RunReport;
    latest: boolean;
    action: string;
    query: ResultQuery<CaseFilter>;
    comparison: PreviousComparison;
}

export function BehaviorRunPage(handle: Handle<BehaviorRunPageProps>) {
    return () => {
        let { report, latest, action, query, comparison } = handle.props;
        let summary = summarizeRun(report);
        let matching = report.behavior.filter(test => matchesCase(test, query)).sort(compareTests);
        let page = paginate(matching, query.page);
        return (
            <Document section="behavior" title={latest ? "Latest behavior run" : "Behavior run"}>
                <PageHeader title={latest ? "Latest behavior run" : "Behavior run"}>
                    <RunMeta report={report} />
                </PageHeader>
                <StateNotice noun="files" report={report} summary={summary} />
                <section aria-labelledby="summary-heading" mix={sectionStyle}>
                    <h2 id="summary-heading">Summary</h2>
                    <Scope report={report} />
                    <OutcomeSummary action={action} summary={summary} />
                    <ComparisonLine comparison={comparison} report={report} />
                    <RunConditions report={report} />
                </section>
                {report.behavior.length === 0 ? (
                    <p mix={proseStyle}>This run has no recorded test results.</p>
                ) : (
                    <div mix={stackStyle}>
                        <section aria-labelledby="modules-heading" mix={sectionStyle}>
                            <h2 id="modules-heading">Modules</h2>
                            <ModuleTable
                                action={action}
                                query={query}
                                rows={summarizeModules(report.behavior)}
                            />
                        </section>
                        <section aria-labelledby="files-heading" mix={sectionStyle}>
                            <h2 id="files-heading">Test files</h2>
                            <FilterForm
                                action={action}
                                label="Filter test files"
                                modules={[
                                    ...new Set(report.behavior.map(test => test.module)),
                                ].sort()}
                                query={query}
                                searchLabel="Test path contains"
                                statuses={CASE_FILTER_OPTIONS}
                            />
                            <Pagination
                                action={action}
                                noun="test files"
                                page={page}
                                query={query}
                            />
                            {page.items.length === 0 ? null : (
                                <ResultsTable
                                    caption="Recorded test files by module and path. Select a path for logs and the exact invocations."
                                    runId={report.id}
                                    tests={page.items}
                                />
                            )}
                        </section>
                    </div>
                )}
            </Document>
        );
    };
}

function ComparisonLine(handle: Handle<{ report: RunReport; comparison: PreviousComparison }>) {
    return () => {
        let { report, comparison } = handle.props;
        if (comparison.kind === "incomplete") {
            return <p mix={proseStyle}>Not compared: only complete runs are compared.</p>;
        }
        if (comparison.kind === "none") {
            return (
                <p mix={proseStyle}>
                    Not compared: no earlier complete run used the same corpus revision, selection,
                    and platform.
                </p>
            );
        }
        let { previous } = comparison;
        let previousLink = (
            <a href={routes.runs.show.href({ runId: previous.id })}>
                {formatTimestamp(previous.startedAt)}
            </a>
        );
        if (comparison.kind === "incomparable") {
            return (
                <p mix={proseStyle}>
                    Not compared with the previous run from {previousLink}: {comparison.reason}
                </p>
            );
        }
        let { regressions, improvements } = comparison;
        let compareHref = withSearch(routes.compare.href(), {
            current: report.id,
            previous: previous.id,
        });
        return (
            <p mix={proseStyle}>
                Compared with the run from {previousLink} (Bento {previous.bentoVersion}):{" "}
                <a href={compareHref}>
                    {formatCount(regressions)} {regressions === 1 ? "regression" : "regressions"},{" "}
                    {formatCount(improvements)}{" "}
                    {improvements === 1 ? "improvement" : "improvements"}
                </a>
                .
            </p>
        );
    };
}
