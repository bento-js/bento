import type { Handle } from "remix/component";

import { formatCount, withSearch } from "#app/ui/format.ts";
import { proseStyle } from "#app/ui/layout.tsx";
import { Count, dataTableStyle, TableRegion } from "#app/ui/table.tsx";

import type { RunReport, RunSummary } from "../../../lib/model.ts";

export interface OutcomeSummaryProps {
    summary: RunSummary;
    action: string;
}

export function Scope(handle: Handle<{ report: RunReport }>) {
    return () => {
        let { report } = handle.props;
        let { selection, suite } = report;
        return (
            <p mix={proseStyle}>
                Upstream Node {suite.version} test files ({suite.scope}).{" "}
                {selection.filter === "" ? (
                    <>
                        No filter: <strong>{formatCount(selection.selected)}</strong> of{" "}
                        {formatCount(selection.total)} corpus files selected.
                    </>
                ) : (
                    <>
                        Filter <code>{selection.filter}</code> selected{" "}
                        <strong>{formatCount(selection.selected)}</strong> of{" "}
                        {formatCount(selection.total)} corpus files; this is not a measurement of
                        the whole corpus.
                    </>
                )}{" "}
                A pass requires both the Node control and Bento to pass. Files whose Node control
                did not pass are reported separately and never counted for Bento.
            </p>
        );
    };
}

export function OutcomeSummary(handle: Handle<OutcomeSummaryProps>) {
    return () => {
        let { summary, action } = handle.props;
        let href = (status: string) => withSearch(action, { status });
        return (
            <TableRegion labelledBy="outcomes-caption">
                <table mix={dataTableStyle}>
                    <caption id="outcomes-caption">
                        Outcomes of {formatCount(summary.selected)} selected files,{" "}
                        {formatCount(summary.recorded)} recorded
                    </caption>
                    <thead>
                        <tr>
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
                                Node control did not pass
                            </th>
                        </tr>
                    </thead>
                    <tbody>
                        <tr>
                            <Count href={href("passed")} value={summary.passed} />
                            <Count href={href("failed")} tone="fail" value={summary.failed} />
                            <Count href={href("timeout")} tone="warn" value={summary.timeout} />
                            <Count href={href("crashed")} tone="fail" value={summary.crashed} />
                            <Count href={href("unsupported")} value={summary.unsupported} />
                            <Count href={href("skipped")} value={summary.skipped} />
                            <Count value={summary.notRun} />
                            <Count href={href("control")} value={summary.baselineFailed} />
                        </tr>
                    </tbody>
                </table>
            </TableRegion>
        );
    };
}
