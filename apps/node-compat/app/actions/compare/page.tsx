import type { Handle } from "remix/component";

import { css } from "remix/component";

import type { ComparedRuns } from "#app/actions/compare/result.tsx";

import { CompareResult } from "#app/actions/compare/result.tsx";
import { routes } from "#app/routes.ts";
import { Document } from "#app/ui/document.tsx";
import { EmptyState } from "#app/ui/empty-state.tsx";
import { formatTimestamp } from "#app/ui/format.ts";
import { PageHeader, proseStyle } from "#app/ui/layout.tsx";
import { theme } from "#app/ui/theme.tsx";

import type { RunSummary } from "../../../lib/model.ts";

export interface ComparePageProps {
    runs: RunSummary[];
    current: string;
    previous: string;
    result: ComparedRuns | null;
}

export function ComparePage(handle: Handle<ComparePageProps>) {
    return () => {
        let { runs, current, previous, result } = handle.props;
        return (
            <Document section="compare" title="Compare runs">
                <PageHeader title="Compare behavior runs">
                    <p mix={proseStyle}>
                        A regression is a test file whose Node control passed in both runs and that
                        passed under Bento in the baseline run but not in the current one. Only
                        complete runs recorded with the same corpus, selection, platform, deadline,
                        Node control, and Bento arguments are compared; the Bento build may differ.
                    </p>
                </PageHeader>
                {runs.length < 2 && result === null ? (
                    <EmptyState kind="behavior" title="Comparisons need two behavior runs" />
                ) : (
                    <CompareForm current={current} previous={previous} runs={runs} />
                )}
                {result === null ? null : <CompareResult {...result} />}
            </Document>
        );
    };
}

function runLabel(run: RunSummary): string {
    let filter = run.filter === "" ? "no filter" : `filter ${run.filter}`;
    return `${formatTimestamp(run.startedAt)} · Bento ${run.bentoVersion} · ${filter} · ${run.state}`;
}

function CompareForm(handle: Handle<{ runs: RunSummary[]; current: string; previous: string }>) {
    return () => {
        let { runs, current, previous } = handle.props;
        return (
            <form
                action={routes.compare.href()}
                aria-label="Choose runs"
                method="get"
                mix={formStyle}
            >
                <label htmlFor="compare-previous">Baseline run</label>
                <select id="compare-previous" name="previous" value={previous}>
                    {runs.map(run => (
                        <option key={run.id} value={run.id}>
                            {runLabel(run)}
                        </option>
                    ))}
                </select>
                <label htmlFor="compare-current">Current run</label>
                <select id="compare-current" name="current" value={current}>
                    {runs.map(run => (
                        <option key={run.id} value={run.id}>
                            {runLabel(run)}
                        </option>
                    ))}
                </select>
                <button type="submit">Compare</button>
            </form>
        );
    };
}

let formStyle = css({
    display: "grid",
    gridTemplateColumns: "max-content minmax(0, 40rem)",
    alignItems: "center",
    gap: `${theme.space.md} ${theme.space.lg}`,
    "& label": {
        color: theme.colors.text.secondary,
        fontSize: theme.fontSize.sm,
        fontWeight: theme.fontWeight.medium,
    },
    "& select": {
        height: theme.control.height,
        width: "100%",
        padding: `0 ${theme.space.md}`,
        backgroundColor: theme.surface.lvl0,
        border: `1px solid ${theme.colors.border.default}`,
        borderRadius: theme.radius.md,
    },
    "& button": {
        gridColumn: "2",
        justifySelf: "start",
        height: theme.control.height,
        padding: `0 ${theme.space.xl}`,
        borderRadius: theme.radius.md,
        backgroundColor: theme.colors.action.background,
        color: theme.colors.action.foreground,
        fontWeight: theme.fontWeight.medium,
        cursor: "pointer",
    },
    "& button:hover": {
        backgroundColor: theme.colors.action.backgroundHover,
    },
    "@media (max-width: 40rem)": {
        gridTemplateColumns: "minmax(0, 1fr)",
        "& button": { gridColumn: "1" },
    },
});
