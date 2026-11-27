import type { Handle } from "remix/component";

import { routes } from "#app/routes.ts";
import { Document } from "#app/ui/document.tsx";
import { EmptyState } from "#app/ui/empty-state.tsx";
import { formatCount, formatTimestamp, withSearch } from "#app/ui/format.ts";
import { PageHeader, proseStyle, sectionStyle } from "#app/ui/layout.tsx";
import { StateBadge } from "#app/ui/status.tsx";
import { Count, dataTableStyle, TableRegion } from "#app/ui/table.tsx";

import type { RunSummary } from "../../../lib/model.ts";

export function HistoryPage(handle: Handle<{ runs: RunSummary[] }>) {
    return () => {
        let { runs } = handle.props;
        let behavior = runs.filter(run => run.kind === "behavior");
        let inventory = runs.filter(run => run.kind === "inventory");
        return (
            <Document section="history" title="History">
                <PageHeader title="History">
                    <p mix={proseStyle}>
                        Every saved run, newest first. Interrupted and failed runs stay incomplete;
                        their counts cover only what was recorded.
                    </p>
                </PageHeader>
                {runs.length === 0 ? <EmptyState kind="all" title="No runs recorded yet" /> : null}
                {behavior.length === 0 ? null : (
                    <section aria-labelledby="behavior-history-heading" mix={sectionStyle}>
                        <h2 id="behavior-history-heading">Behavior runs</h2>
                        <BehaviorHistory runs={behavior} />
                    </section>
                )}
                {inventory.length === 0 ? null : (
                    <section aria-labelledby="inventory-history-heading" mix={sectionStyle}>
                        <h2 id="inventory-history-heading">Availability inventories</h2>
                        <InventoryHistory runs={inventory} />
                    </section>
                )}
            </Document>
        );
    };
}

function RunCells(handle: Handle<{ run: RunSummary }>) {
    return () => {
        let { run } = handle.props;
        return (
            <>
                <th class="mono" scope="row">
                    <a href={routes.runs.show.href({ runId: run.id })}>
                        <time dateTime={run.startedAt}>{formatTimestamp(run.startedAt)}</time>
                    </a>
                </th>
                <td>
                    <StateBadge state={run.state} />
                </td>
                <td class="mono">{run.bentoVersion}</td>
                <td class="mono">{run.nodeVersion}</td>
                <td class="mono">
                    {run.platform} {run.arch}
                </td>
            </>
        );
    };
}

function BehaviorHistory(handle: Handle<{ runs: RunSummary[] }>) {
    return () => {
        let { runs } = handle.props;
        return (
            <TableRegion labelledBy="behavior-history-caption">
                <table mix={dataTableStyle}>
                    <caption id="behavior-history-caption">
                        Behavior runs. Counts are test files; a pass requires the Node control and
                        Bento to pass.
                    </caption>
                    <thead>
                        <tr>
                            <th scope="col">Started</th>
                            <th scope="col">State</th>
                            <th scope="col">Bento</th>
                            <th scope="col">Node</th>
                            <th scope="col">Platform</th>
                            <th scope="col">Filter</th>
                            <th class="num" scope="col">
                                Selected of corpus
                            </th>
                            <th class="num" scope="col">
                                Recorded
                            </th>
                            <th class="num" scope="col">
                                Passed
                            </th>
                            <th class="num" scope="col">
                                Failed
                            </th>
                            <th class="num" scope="col">
                                Timed out
                            </th>
                            <th class="num" scope="col">
                                Crashed
                            </th>
                            <th class="num" scope="col">
                                Unsupported
                            </th>
                            <th class="num" scope="col">
                                Skipped
                            </th>
                            <th class="num" scope="col">
                                Not run
                            </th>
                            <th class="num" scope="col">
                                Control not passed
                            </th>
                            <th scope="col">Compare</th>
                        </tr>
                    </thead>
                    <tbody>
                        {runs.map((run, index) => {
                            let previous = runs[index + 1];
                            return (
                                <tr key={run.id}>
                                    <RunCells run={run} />
                                    <td class="mono">
                                        {run.filter === "" ? (
                                            <span class="muted">None</span>
                                        ) : (
                                            run.filter
                                        )}
                                    </td>
                                    <td class="num">
                                        {formatCount(run.selected)} of {formatCount(run.total)}
                                    </td>
                                    <Count value={run.recorded} />
                                    <Count value={run.passed} />
                                    <Count tone="fail" value={run.failed} />
                                    <Count tone="warn" value={run.timeout} />
                                    <Count tone="fail" value={run.crashed} />
                                    <Count value={run.unsupported} />
                                    <Count value={run.skipped} />
                                    <Count value={run.notRun} />
                                    <Count value={run.baselineFailed} />
                                    <td>
                                        {previous === undefined ? null : (
                                            <a
                                                href={withSearch(routes.compare.href(), {
                                                    current: run.id,
                                                    previous: previous.id,
                                                })}
                                            >
                                                With previous
                                            </a>
                                        )}
                                    </td>
                                </tr>
                            );
                        })}
                    </tbody>
                </table>
            </TableRegion>
        );
    };
}

function InventoryHistory(handle: Handle<{ runs: RunSummary[] }>) {
    return () => {
        let { runs } = handle.props;
        return (
            <TableRegion labelledBy="inventory-history-caption">
                <table mix={dataTableStyle}>
                    <caption id="inventory-history-caption">
                        Availability inventories. Presence only; no API was called.
                    </caption>
                    <thead>
                        <tr>
                            <th scope="col">Started</th>
                            <th scope="col">State</th>
                            <th scope="col">Bento</th>
                            <th scope="col">Node</th>
                            <th scope="col">Platform</th>
                            <th class="num" scope="col">
                                Entries
                            </th>
                            <th class="num" scope="col">
                                Present
                            </th>
                            <th class="num" scope="col">
                                Missing
                            </th>
                            <th class="num" scope="col">
                                Type mismatch
                            </th>
                            <th class="num" scope="col">
                                Inspection error
                            </th>
                        </tr>
                    </thead>
                    <tbody>
                        {runs.map(run => (
                            <tr key={run.id}>
                                <RunCells run={run} />
                                <Count value={run.recorded} />
                                <Count value={run.available} />
                                <Count tone="fail" value={run.missing} />
                                <Count tone="warn" value={run.typeMismatch} />
                                <Count value={run.inspectionErrors} />
                            </tr>
                        ))}
                    </tbody>
                </table>
            </TableRegion>
        );
    };
}
