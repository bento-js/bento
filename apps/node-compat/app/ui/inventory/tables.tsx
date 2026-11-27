import type { Handle } from "remix/component";

import type { EntryFilter, InventoryModuleRow } from "#app/ui/inventory/results.ts";
import type { ResultQuery } from "#app/ui/query.ts";

import { formatCount, withSearch } from "#app/ui/format.ts";
import { InventoryBadge } from "#app/ui/status.tsx";
import { Count, dataTableStyle, TableRegion } from "#app/ui/table.tsx";

import type { InventoryResult, RunSummary } from "../../../lib/model.ts";

export function AvailabilitySummary(handle: Handle<{ summary: RunSummary; action: string }>) {
    return () => {
        let { summary, action } = handle.props;
        let href = (status: string) => withSearch(action, { status });
        return (
            <TableRegion labelledBy="availability-caption">
                <table mix={dataTableStyle}>
                    <caption id="availability-caption">
                        {formatCount(summary.recorded)} Node baseline entries inspected
                    </caption>
                    <thead>
                        <tr>
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
                        <tr>
                            <Count href={href("present")} value={summary.available} />
                            <Count href={href("missing")} tone="fail" value={summary.missing} />
                            <Count
                                href={href("type-mismatch")}
                                tone="warn"
                                value={summary.typeMismatch}
                            />
                            <Count href={href("error")} value={summary.inspectionErrors} />
                        </tr>
                    </tbody>
                </table>
            </TableRegion>
        );
    };
}

export interface InventoryModuleTableProps {
    rows: InventoryModuleRow[];
    action: string;
    query: ResultQuery<EntryFilter>;
}

export function InventoryModuleTable(handle: Handle<InventoryModuleTableProps>) {
    return () => {
        let { rows, action, query } = handle.props;
        return (
            <TableRegion labelledBy="inventory-modules-caption" tall>
                <table mix={dataTableStyle}>
                    <caption id="inventory-modules-caption">
                        {rows.length} modules and globals, most missing, mismatched, or errored
                        entries first
                    </caption>
                    <thead>
                        <tr>
                            <th scope="col">Module</th>
                            <th class="num" scope="col">
                                Baseline
                            </th>
                            <th class="num" scope="col">
                                Present
                            </th>
                            <th class="num" scope="col">
                                Missing
                            </th>
                            <th class="num" scope="col">
                                Mismatch
                            </th>
                            <th class="num" scope="col">
                                Error
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
                                <Count value={row.baseline} />
                                <Count value={row.present} />
                                <Count tone="fail" value={row.missing} />
                                <Count tone="warn" value={row.mismatch} />
                                <Count value={row.error} />
                            </tr>
                        ))}
                    </tbody>
                </table>
            </TableRegion>
        );
    };
}

export function EntriesTable(handle: Handle<{ entries: InventoryResult[] }>) {
    return () => {
        let { entries } = handle.props;
        return (
            <TableRegion labelledBy="entries-caption">
                <table mix={dataTableStyle}>
                    <caption id="entries-caption">
                        Baseline entries by module and name, with the <code>typeof</code> each
                        runtime reports
                    </caption>
                    <thead>
                        <tr>
                            <th scope="col">Module</th>
                            <th scope="col">Name</th>
                            <th scope="col">Node</th>
                            <th scope="col">Bento</th>
                            <th scope="col">Status</th>
                            <th scope="col">Error</th>
                        </tr>
                    </thead>
                    <tbody>
                        {entries.map(entry => (
                            <tr key={`${entry.module}\u0000${entry.name}`}>
                                <td class="mono">{entry.module}</td>
                                <th class="mono wrap" scope="row">
                                    {entry.name === "" ? (
                                        <span class="muted">(module)</span>
                                    ) : (
                                        entry.name
                                    )}
                                </th>
                                <td class="mono">{entry.node}</td>
                                <td class="mono">
                                    {entry.bento === null ? (
                                        <span class="muted">none</span>
                                    ) : (
                                        entry.bento
                                    )}
                                </td>
                                <td>
                                    <InventoryBadge status={entry.status} />
                                </td>
                                <td class="mono wrap">{entry.error}</td>
                            </tr>
                        ))}
                    </tbody>
                </table>
            </TableRegion>
        );
    };
}
