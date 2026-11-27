import type { FilterOption } from "#app/ui/filters.tsx";
import type { ResultQuery } from "#app/ui/query.ts";

import { containsText } from "#app/ui/query.ts";
import { OUTCOME_LABELS, OUTCOMES } from "#app/ui/status.tsx";

import type { BehaviorResult, Outcome } from "../../../lib/model.ts";

export type CaseStatus = Outcome | "control";
export type CaseFilter = CaseStatus | "not-passed";

export const CASE_FILTERS: readonly CaseFilter[] = ["not-passed", ...OUTCOMES, "control"];

export const CASE_FILTER_OPTIONS: FilterOption[] = [
    { value: "", label: "All recorded files" },
    { value: "not-passed", label: "Did not pass under Bento" },
    ...OUTCOMES.map(outcome => ({ value: outcome, label: `Bento: ${OUTCOME_LABELS[outcome]}` })),
    { value: "control", label: "Node control did not pass" },
];

export function caseStatus(test: BehaviorResult): CaseStatus {
    return test.node.status === "passed" ? test.bento.status : "control";
}

export function matchesCase(test: BehaviorResult, query: ResultQuery<CaseFilter>): boolean {
    if (query.module !== "" && test.module !== query.module) return false;
    if (!containsText(test.id, query.q)) return false;
    let status = caseStatus(test);
    if (query.status === "not-passed") return status !== "passed" && status !== "control";
    return query.status === "" || query.status === status;
}

export interface ModuleRow {
    module: string;
    files: number;
    passed: number;
    notPassed: number;
    control: number;
}

export function summarizeModules(results: readonly BehaviorResult[]): ModuleRow[] {
    let rows = new Map<string, ModuleRow>();
    for (let test of results) {
        let row = rows.get(test.module);
        if (row === undefined) {
            row = { module: test.module, files: 0, passed: 0, notPassed: 0, control: 0 };
            rows.set(test.module, row);
        }
        row.files++;
        let status = caseStatus(test);
        if (status === "control") row.control++;
        else if (status === "passed") row.passed++;
        else row.notPassed++;
    }
    return [...rows.values()].sort(
        (a, b) => b.notPassed - a.notPassed || a.module.localeCompare(b.module),
    );
}

export function compareTests(a: BehaviorResult, b: BehaviorResult): number {
    return a.module.localeCompare(b.module) || a.id.localeCompare(b.id);
}
