import type { FilterOption } from "#app/ui/filters.tsx";
import type { ResultQuery } from "#app/ui/query.ts";

import { containsText } from "#app/ui/query.ts";
import { INVENTORY_LABELS, INVENTORY_STATUSES } from "#app/ui/status.tsx";

import type { InventoryResult } from "../../../lib/model.ts";

export type EntryFilter = InventoryResult["status"] | "problems";

export const ENTRY_FILTERS: readonly EntryFilter[] = ["problems", ...INVENTORY_STATUSES];

export const ENTRY_FILTER_OPTIONS: FilterOption[] = [
    { value: "", label: "All baseline entries" },
    { value: "problems", label: "Missing, type mismatch, or error" },
    ...INVENTORY_STATUSES.map(status => ({ value: status, label: INVENTORY_LABELS[status] })),
];

export function matchesEntry(entry: InventoryResult, query: ResultQuery<EntryFilter>): boolean {
    if (query.module !== "" && entry.module !== query.module) return false;
    if (!containsText(entry.name, query.q)) return false;
    if (query.status === "problems") return entry.status !== "present";
    return query.status === "" || query.status === entry.status;
}

export interface InventoryModuleRow {
    module: string;
    baseline: number;
    present: number;
    missing: number;
    mismatch: number;
    error: number;
}

export function summarizeInventoryModules(
    entries: readonly InventoryResult[],
): InventoryModuleRow[] {
    let rows = new Map<string, InventoryModuleRow>();
    for (let entry of entries) {
        let row = rows.get(entry.module);
        if (row === undefined) {
            row = {
                module: entry.module,
                baseline: 0,
                present: 0,
                missing: 0,
                mismatch: 0,
                error: 0,
            };
            rows.set(entry.module, row);
        }
        row.baseline++;
        if (entry.status === "present") row.present++;
        else if (entry.status === "missing") row.missing++;
        else if (entry.status === "type-mismatch") row.mismatch++;
        else row.error++;
    }
    let problems = (row: InventoryModuleRow) => row.missing + row.mismatch + row.error;
    return [...rows.values()].sort(
        (a, b) => problems(b) - problems(a) || a.module.localeCompare(b.module),
    );
}
