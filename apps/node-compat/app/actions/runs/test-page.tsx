import type { Handle, RemixNode } from "remix/component";

import { css } from "remix/component";

import { routes } from "#app/routes.ts";
import { Document } from "#app/ui/document.tsx";
import { formatDuration, formatExit, formatTimestamp, shellQuote } from "#app/ui/format.ts";
import { definitionGridStyle, metaLineStyle, Notice, sectionStyle } from "#app/ui/layout.tsx";
import { ControlBadge, OUTCOME_LABELS, OutcomeBadge } from "#app/ui/status.tsx";
import { theme } from "#app/ui/theme.tsx";

import type { BehaviorResult, Execution, RunReport } from "../../../lib/model.ts";

export interface TestPageProps {
    report: RunReport;
    test: BehaviorResult;
}

export function TestPage(handle: Handle<TestPageProps>) {
    return () => {
        let { report, test } = handle.props;
        let counted = test.node.status === "passed";
        return (
            <Document section="behavior" title={test.id}>
                <header mix={headerStyle}>
                    <p mix={metaLineStyle}>
                        <a href={routes.runs.show.href({ runId: report.id })}>
                            Behavior run from {formatTimestamp(report.startedAt)}
                        </a>
                        <span>Bento {report.runtimes.bento.version}</span>
                        <span>Node control {report.runtimes.node.version}</span>
                    </p>
                    <h1>{test.id}</h1>
                    <dl mix={definitionGridStyle}>
                        <dt>Module</dt>
                        <dd>
                            <code>{test.module}</code>
                        </dd>
                        <dt>Declared flags</dt>
                        <dd>
                            {test.flags.length === 0 ? "None" : <code>{test.flags.join(" ")}</code>}
                        </dd>
                    </dl>
                </header>
                {counted ? null : (
                    <Notice title="Not counted for Bento" tone="warn">
                        <p>
                            The Node control {OUTCOME_LABELS[test.node.status].toLowerCase()}, so
                            this file does not measure Bento compatibility in this run.
                        </p>
                    </Notice>
                )}
                <div mix={executionsStyle}>
                    <ExecutionSection
                        badge={<ControlBadge outcome={test.node.status} />}
                        execution={test.node}
                        id="node"
                        title="Node control"
                    />
                    <ExecutionSection
                        badge={<OutcomeBadge counted={counted} outcome={test.bento.status} />}
                        execution={test.bento}
                        id="bento"
                        title="Bento"
                    />
                </div>
            </Document>
        );
    };
}

interface ExecutionSectionProps {
    id: string;
    title: string;
    badge: RemixNode;
    execution: Execution;
}

function ExecutionSection(handle: Handle<ExecutionSectionProps>) {
    return () => {
        let { id, title, badge, execution } = handle.props;
        return (
            <section aria-labelledby={`${id}-heading`} mix={sectionStyle}>
                <h2 id={`${id}-heading`}>{title}</h2>
                <dl mix={definitionGridStyle}>
                    <dt>Result</dt>
                    <dd>
                        {badge} {execution.reason}
                    </dd>
                    <dt>Exit</dt>
                    <dd>{formatExit(execution)}</dd>
                    <dt>Duration</dt>
                    <dd>{formatDuration(execution.durationMs)}</dd>
                    <dt>Invocation</dt>
                    <dd>
                        {execution.command.length === 0 ? (
                            "Not launched"
                        ) : (
                            <code mix={invocationStyle}>{shellQuote(execution.command)}</code>
                        )}
                    </dd>
                    <dt>Logs</dt>
                    <dd>
                        {execution.truncated
                            ? "Truncated at the collector's output limit; a marker in the log shows where."
                            : "Complete"}
                    </dd>
                </dl>
                <Log id={`${id}-stdout`} label={`${title} stdout`} text={execution.stdout} />
                <Log id={`${id}-stderr`} label={`${title} stderr`} text={execution.stderr} />
            </section>
        );
    };
}

function Log(handle: Handle<{ id: string; label: string; text: string }>) {
    return () => {
        let { id, label, text } = handle.props;
        return (
            <div mix={logStyle}>
                <h3 id={id}>{label}</h3>
                {text === "" ? (
                    <p>No output.</p>
                ) : (
                    <pre aria-labelledby={id} tabIndex={0}>
                        {text}
                    </pre>
                )}
            </div>
        );
    };
}

let headerStyle = css({
    display: "flex",
    flexDirection: "column",
    gap: theme.space.lg,
    "& h1": {
        fontFamily: theme.fontFamily.mono,
        fontSize: theme.fontSize.lg,
        fontWeight: theme.fontWeight.semibold,
        overflowWrap: "anywhere",
    },
});

let executionsStyle = css({
    display: "grid",
    gap: theme.space.xxl,
    "@media (min-width: 72rem)": {
        gridTemplateColumns: "minmax(0, 1fr) minmax(0, 1fr)",
    },
});

let invocationStyle = css({
    display: "block",
    whiteSpace: "pre-wrap",
    overflowWrap: "anywhere",
});

let logStyle = css({
    display: "flex",
    flexDirection: "column",
    gap: theme.space.sm,
    "& h3": {
        color: theme.colors.text.secondary,
        fontSize: theme.fontSize.xs,
        fontWeight: theme.fontWeight.semibold,
    },
    "& p": {
        color: theme.colors.text.muted,
        fontSize: theme.fontSize.sm,
    },
    "& pre": {
        maxHeight: "32rem",
        overflow: "auto",
        padding: `${theme.space.lg} ${theme.space.xl}`,
        backgroundColor: theme.surface.lvl1,
        border: `1px solid ${theme.colors.border.subtle}`,
        borderRadius: theme.radius.md,
        fontSize: theme.fontSize.xs,
        lineHeight: theme.lineHeight.normal,
        whiteSpace: "pre-wrap",
        overflowWrap: "anywhere",
    },
});
