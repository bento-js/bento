import type { Handle } from "remix/component";

import { routes } from "#app/routes.ts";
import { compareTests } from "#app/ui/behavior/results.ts";
import { formatTimestamp, shortHash, withSearch } from "#app/ui/format.ts";
import { Notice, proseStyle, sectionStyle } from "#app/ui/layout.tsx";
import { OutcomeBadge, StateBadge } from "#app/ui/status.tsx";
import { dataTableStyle, TableRegion } from "#app/ui/table.tsx";

import type { RunComparison } from "../../../lib/comparison.ts";
import type { BehaviorResult, RunReport } from "../../../lib/model.ts";

export interface ComparedRuns {
    current: RunReport;
    previous: RunReport;
    comparison: RunComparison;
}

export function CompareResult(handle: Handle<ComparedRuns>) {
    return () => {
        let { current, previous, comparison } = handle.props;
        let before = new Map(previous.behavior.map(test => [test.id, test]));
        return (
            <>
                <RunPair current={current} previous={previous} />
                {comparison.comparable ? (
                    <>
                        <Changes
                            before={before}
                            current={current}
                            id="regressions"
                            tests={comparison.regressions}
                            title="Regressions"
                        />
                        <Changes
                            before={before}
                            current={current}
                            id="improvements"
                            tests={comparison.improvements}
                            title="Improvements"
                        />
                    </>
                ) : (
                    <Notice title="These runs are not comparable" tone="warn">
                        <p>
                            {comparison.reason} Differences between runs recorded under different
                            conditions are not reported as regressions or improvements.
                        </p>
                    </Notice>
                )}
            </>
        );
    };
}

function RunPair(handle: Handle<{ current: RunReport; previous: RunReport }>) {
    return () => {
        let { current, previous } = handle.props;
        let rows: [string, RunReport][] = [
            ["Baseline", previous],
            ["Current", current],
        ];
        return (
            <TableRegion labelledBy="pair-caption">
                <table mix={dataTableStyle}>
                    <caption id="pair-caption">Compared runs</caption>
                    <thead>
                        <tr>
                            <th scope="col">Role</th>
                            <th scope="col">Started</th>
                            <th scope="col">State</th>
                            <th scope="col">Bento</th>
                            <th scope="col">Bento SHA-256</th>
                            <th scope="col">Filter</th>
                            <th scope="col">Corpus revision</th>
                        </tr>
                    </thead>
                    <tbody>
                        {rows.map(([role, run]) => (
                            <tr key={role}>
                                <th scope="row">{role}</th>
                                <td class="mono">
                                    <a href={routes.runs.show.href({ runId: run.id })}>
                                        {formatTimestamp(run.startedAt)}
                                    </a>
                                </td>
                                <td>
                                    <StateBadge state={run.state} />
                                </td>
                                <td class="mono">{run.runtimes.bento.version}</td>
                                <td class="mono" title={run.runtimes.bento.sha256}>
                                    {shortHash(run.runtimes.bento.sha256)}
                                </td>
                                <td class="mono">{run.selection.filter || "None"}</td>
                                <td class="mono">{shortHash(run.suite.revision)}</td>
                            </tr>
                        ))}
                    </tbody>
                </table>
            </TableRegion>
        );
    };
}

interface ChangesProps {
    id: string;
    title: string;
    tests: BehaviorResult[];
    before: Map<string, BehaviorResult>;
    current: RunReport;
}

function Changes(handle: Handle<ChangesProps>) {
    return () => {
        let { id, title, tests, before, current } = handle.props;
        let testHref = routes.runs.test.href({ runId: current.id });
        return (
            <section aria-labelledby={`${id}-heading`} mix={sectionStyle}>
                <h2 id={`${id}-heading`}>
                    {title} ({tests.length})
                </h2>
                {tests.length === 0 ? (
                    <p mix={proseStyle}>None.</p>
                ) : (
                    <TableRegion labelledBy={`${id}-heading`}>
                        <table aria-labelledby={`${id}-heading`} mix={dataTableStyle}>
                            <thead>
                                <tr>
                                    <th scope="col">Test file</th>
                                    <th scope="col">Module</th>
                                    <th scope="col">Baseline Bento</th>
                                    <th scope="col">Current Bento</th>
                                    <th scope="col">Current detail</th>
                                </tr>
                            </thead>
                            <tbody>
                                {[...tests].sort(compareTests).map(test => {
                                    let previous = before.get(test.id);
                                    return (
                                        <tr key={test.id}>
                                            <th class="mono wrap" scope="row">
                                                <a href={withSearch(testHref, { id: test.id })}>
                                                    {test.id}
                                                </a>
                                            </th>
                                            <td class="mono">{test.module}</td>
                                            <td>
                                                {previous === undefined ? null : (
                                                    <OutcomeBadge outcome={previous.bento.status} />
                                                )}
                                            </td>
                                            <td>
                                                <OutcomeBadge outcome={test.bento.status} />
                                            </td>
                                            <td class="wrap">{test.bento.reason}</td>
                                        </tr>
                                    );
                                })}
                            </tbody>
                        </table>
                    </TableRegion>
                )}
            </section>
        );
    };
}
