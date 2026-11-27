import type { Handle } from "remix/component";

import { css } from "remix/component";

import { routes } from "#app/routes.ts";
import {
    formatCount,
    formatDuration,
    formatElapsed,
    formatTimestamp,
    shellQuote,
    shortHash,
} from "#app/ui/format.ts";
import { definitionGridStyle, metaLineStyle, Notice } from "#app/ui/layout.tsx";
import { StateBadge } from "#app/ui/status.tsx";
import { theme } from "#app/ui/theme.tsx";

import type { RunReport, RunSummary, RuntimeIdentity } from "../../lib/model.ts";

export function RunMeta(handle: Handle<{ report: RunReport }>) {
    return () => {
        let { report } = handle.props;
        return (
            <p mix={metaLineStyle}>
                <StateBadge state={report.state} />
                <span>
                    Started{" "}
                    <time dateTime={report.startedAt}>{formatTimestamp(report.startedAt)}</time>
                </span>
                <span>
                    Bento <strong>{report.runtimes.bento.version}</strong>
                </span>
                <span>
                    Node control <strong>{report.runtimes.node.version}</strong>
                </span>
                <span>
                    {report.platform} {report.arch}
                </span>
            </p>
        );
    };
}

export function StateNotice(
    handle: Handle<{ report: RunReport; summary: RunSummary; noun: string }>,
) {
    return () => {
        let { report, summary, noun } = handle.props;
        let unrecorded = Math.max(0, summary.selected - summary.recorded);
        let missing =
            unrecorded > 0
                ? ` ${formatCount(unrecorded)} of ${formatCount(summary.selected)} selected ${noun} have no result and count as not run.`
                : "";
        if (report.state === "running") {
            return (
                <Notice title="Collection is still running" tone="warn">
                    <p>
                        The collector saves results when the run completes, is interrupted, or
                        fails. Counts below are partial until then; reload to update.{missing}
                    </p>
                </Notice>
            );
        }
        if (report.state === "interrupted") {
            return (
                <Notice title="Collection was interrupted" tone="warn">
                    <p>This run is incomplete and is not used for comparisons.{missing}</p>
                </Notice>
            );
        }
        if (report.state === "failed") {
            return (
                <Notice title="Collection failed" tone="fail">
                    <p>This run is incomplete and is not used for comparisons.{missing}</p>
                    {report.error === null ? null : <pre>{report.error}</pre>}
                </Notice>
            );
        }
        return null;
    };
}

export function RunConditions(handle: Handle<{ report: RunReport }>) {
    return () => {
        let { report } = handle.props;
        let elapsed = formatElapsed(report.startedAt, report.finishedAt);
        let filter = report.selection.filter;
        return (
            <details mix={detailsStyle}>
                <summary>Run conditions</summary>
                <dl mix={definitionGridStyle}>
                    <dt>Run</dt>
                    <dd>
                        <a href={routes.runs.show.href({ runId: report.id })}>
                            <code>{report.id}</code>
                        </a>
                    </dd>
                    <dt>Recorded</dt>
                    <dd>
                        {formatTimestamp(report.startedAt)} to{" "}
                        {report.finishedAt === null
                            ? "not finished"
                            : formatTimestamp(report.finishedAt)}
                        {elapsed === null ? null : ` (${elapsed})`}
                    </dd>
                    <dt>Corpus</dt>
                    <dd>
                        {report.suite.source} {report.suite.version} at{" "}
                        <code>{report.suite.revision}</code>, scope{" "}
                        <code>{report.suite.scope}</code>
                    </dd>
                    <dt>Selection</dt>
                    <dd>
                        {filter === "" ? (
                            "No filter"
                        ) : (
                            <>
                                Filter <code>{filter}</code>
                            </>
                        )}
                        : {formatCount(report.selection.selected)} of{" "}
                        {formatCount(report.selection.total)} selected
                    </dd>
                    <dt>Platform</dt>
                    <dd>
                        {report.platform} {report.arch}
                    </dd>
                    <dt>Deadline</dt>
                    <dd>{formatDuration(report.timeoutMs)} per process</dd>
                    <dt>Node control</dt>
                    <dd>
                        <Runtime identity={report.runtimes.node} />
                    </dd>
                    <dt>Bento</dt>
                    <dd>
                        <Runtime identity={report.runtimes.bento} />
                    </dd>
                </dl>
            </details>
        );
    };
}

function Runtime(handle: Handle<{ identity: RuntimeIdentity }>) {
    return () => {
        let { identity } = handle.props;
        return (
            <>
                {identity.version} · <code>{identity.executable}</code> · SHA-256{" "}
                <code title={identity.sha256}>{shortHash(identity.sha256)}</code> · arguments{" "}
                {identity.arguments.length === 0 ? (
                    "none"
                ) : (
                    <code>{shellQuote(identity.arguments)}</code>
                )}
            </>
        );
    };
}

let detailsStyle = css({
    fontSize: theme.fontSize.sm,
    "& > summary": {
        width: "fit-content",
        color: theme.colors.text.secondary,
        fontWeight: theme.fontWeight.medium,
    },
    "& > dl": {
        marginTop: theme.space.lg,
        padding: `${theme.space.lg} ${theme.space.xl}`,
        backgroundColor: theme.surface.lvl1,
        border: `1px solid ${theme.colors.border.subtle}`,
        borderRadius: theme.radius.md,
    },
});
