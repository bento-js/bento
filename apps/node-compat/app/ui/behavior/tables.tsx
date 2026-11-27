import type { Handle } from "remix/component";

import type { CaseFilter, ModuleRow } from "#app/ui/behavior/results.ts";
import type { ResultQuery } from "#app/ui/query.ts";

import { routes } from "#app/routes.ts";
import { caseStatus } from "#app/ui/behavior/results.ts";
import { formatDuration, formatExit, withSearch } from "#app/ui/format.ts";
import { ControlBadge, OUTCOME_LABELS, OutcomeBadge } from "#app/ui/status.tsx";
import { Count, dataTableStyle, TableRegion } from "#app/ui/table.tsx";

import type { BehaviorResult } from "../../../lib/model.ts";

export interface ModuleTableProps {
    rows: ModuleRow[];
    action: string;
    query: ResultQuery<CaseFilter>;
}

export function ModuleTable(handle: Handle<ModuleTableProps>) {
    return () => {
        let { rows, action, query } = handle.props;
        return (
            <TableRegion labelledBy="modules-caption" tall>
                <table mix={dataTableStyle}>
                    <caption id="modules-caption">
                        {rows.length} modules, most Bento non-passes first. Not passed: the Node
                        control passed and Bento did not. Control not passed: excluded from Bento
                        counts.
                    </caption>
                    <thead>
                        <tr>
                            <th scope="col">Module</th>
                            <th class="num" scope="col">
                                Files
                            </th>
                            <th class="num" scope="col">
                                Passed
                            </th>
                            <th class="num" scope="col">
                                Not passed
                            </th>
                            <th class="num" scope="col">
                                Control not passed
                            </th>
                        </tr>
                    </thead>
                    <tbody>
                        {rows.map(row => (
                            <tr
                                aria-current={row.module === query.module ? "true" : undefined}
                                key={row.module}
                            >
                                <th class="mono" scope="row">
                                    <a
                                        href={withSearch(action, {
                                            module: row.module,
                                            status: query.status,
                                            q: query.q,
                                        })}
                                    >
                                        {row.module}
                                    </a>
                                </th>
                                <Count value={row.files} />
                                <Count value={row.passed} />
                                <Count
                                    href={withSearch(action, {
                                        module: row.module,
                                        status: "not-passed",
                                    })}
                                    tone="fail"
                                    value={row.notPassed}
                                />
                                <Count
                                    href={withSearch(action, {
                                        module: row.module,
                                        status: "control",
                                    })}
                                    value={row.control}
                                />
                            </tr>
                        ))}
                    </tbody>
                </table>
            </TableRegion>
        );
    };
}

export interface ResultsTableProps {
    runId: string;
    tests: BehaviorResult[];
    caption: string;
}

export function ResultsTable(handle: Handle<ResultsTableProps>) {
    return () => {
        let { runId, tests, caption } = handle.props;
        let testHref = routes.runs.test.href({ runId });
        return (
            <TableRegion labelledBy="results-caption">
                <table mix={dataTableStyle}>
                    <caption id="results-caption">{caption}</caption>
                    <thead>
                        <tr>
                            <th scope="col">Test file</th>
                            <th scope="col">Module</th>
                            <th scope="col">Node control</th>
                            <th scope="col">Bento</th>
                            <th scope="col">Bento exit</th>
                            <th class="num" scope="col">
                                Bento time
                            </th>
                            <th scope="col">Flags</th>
                            <th scope="col">Detail</th>
                        </tr>
                    </thead>
                    <tbody>
                        {tests.map(test => {
                            let counted = caseStatus(test) !== "control";
                            return (
                                <tr key={test.id}>
                                    <th class="mono wrap" scope="row">
                                        <a href={withSearch(testHref, { id: test.id })}>
                                            {test.id}
                                        </a>
                                    </th>
                                    <td class="mono">{test.module}</td>
                                    <td>
                                        <ControlBadge outcome={test.node.status} />
                                    </td>
                                    <td>
                                        <OutcomeBadge
                                            counted={counted}
                                            outcome={test.bento.status}
                                        />
                                    </td>
                                    <td class="mono">{formatExit(test.bento)}</td>
                                    <td class="num">{formatDuration(test.bento.durationMs)}</td>
                                    <td class="mono wrap">
                                        {test.flags.length === 0 ? (
                                            <span class="muted">None</span>
                                        ) : (
                                            test.flags.join(" ")
                                        )}
                                    </td>
                                    <td class="wrap">
                                        {counted
                                            ? test.bento.reason
                                            : `Not counted: Node control ${OUTCOME_LABELS[test.node.status].toLowerCase()}`}
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
