import type { Handle } from "remix/component";

import type { EntryFilter } from "#app/ui/inventory/results.ts";
import type { ResultQuery } from "#app/ui/query.ts";

import { Document } from "#app/ui/document.tsx";
import { FilterForm, Pagination } from "#app/ui/filters.tsx";
import {
    ENTRY_FILTER_OPTIONS,
    matchesEntry,
    summarizeInventoryModules,
} from "#app/ui/inventory/results.ts";
import {
    AvailabilitySummary,
    EntriesTable,
    InventoryModuleTable,
} from "#app/ui/inventory/tables.tsx";
import { PageHeader, proseStyle, sectionStyle, splitStyle } from "#app/ui/layout.tsx";
import { paginate } from "#app/ui/query.ts";
import { RunConditions, RunMeta, StateNotice } from "#app/ui/run-conditions.tsx";

import type { RunReport } from "../../../lib/model.ts";

import { summarizeRun } from "../../../lib/summary.ts";

export interface InventoryPageProps {
    report: RunReport;
    latest: boolean;
    action: string;
    query: ResultQuery<EntryFilter>;
}

export function InventoryPage(handle: Handle<InventoryPageProps>) {
    return () => {
        let { report, latest, action, query } = handle.props;
        let summary = summarizeRun(report);
        let matching = report.inventory
            .filter(entry => matchesEntry(entry, query))
            .sort((a, b) => a.module.localeCompare(b.module) || a.name.localeCompare(b.name));
        let page = paginate(matching, query.page);
        let title = latest ? "Latest availability inventory" : "Availability inventory";
        return (
            <Document section="inventory" title={title}>
                <PageHeader title={title}>
                    <RunMeta report={report} />
                </PageHeader>
                <StateNotice noun="entries" report={report} summary={summary} />
                <section aria-labelledby="availability-heading" mix={sectionStyle}>
                    <h2 id="availability-heading">Summary</h2>
                    <p mix={proseStyle}>
                        Presence only: globals and public built-in module exports from the pinned
                        Node baseline, compared by name and <code>typeof</code>. Nothing here was
                        called, so a present export is not evidence that it behaves like Node. An
                        inspection error means the entry could not be examined; it is not counted as
                        missing.
                    </p>
                    <AvailabilitySummary action={action} summary={summary} />
                    <RunConditions report={report} />
                </section>
                {report.inventory.length === 0 ? (
                    <p mix={proseStyle}>This run has no recorded inventory entries.</p>
                ) : (
                    <div mix={splitStyle}>
                        <section aria-labelledby="inventory-modules-heading" mix={sectionStyle}>
                            <h2 id="inventory-modules-heading">Modules</h2>
                            <InventoryModuleTable
                                action={action}
                                query={query}
                                rows={summarizeInventoryModules(report.inventory)}
                            />
                        </section>
                        <section aria-labelledby="entries-heading" mix={sectionStyle}>
                            <h2 id="entries-heading">Entries</h2>
                            <FilterForm
                                action={action}
                                label="Filter inventory entries"
                                modules={[
                                    ...new Set(report.inventory.map(entry => entry.module)),
                                ].sort()}
                                query={query}
                                searchLabel="Name contains"
                                statuses={ENTRY_FILTER_OPTIONS}
                            />
                            <Pagination action={action} noun="entries" page={page} query={query} />
                            {page.items.length === 0 ? null : <EntriesTable entries={page.items} />}
                        </section>
                    </div>
                )}
            </Document>
        );
    };
}
