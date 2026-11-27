import type { Handle, RemixNode } from "remix/component";

import { css } from "remix/component";

import { theme } from "#app/ui/theme.tsx";

export interface PageHeaderProps {
    title: string;
    children?: RemixNode;
}

export function PageHeader(handle: Handle<PageHeaderProps>) {
    return () => {
        let { title, children } = handle.props;
        return (
            <header mix={pageHeaderStyle}>
                <h1>{title}</h1>
                {children}
            </header>
        );
    };
}

export interface NoticeProps {
    tone?: "info" | "warn" | "fail";
    title: string;
    children?: RemixNode;
}

export function Notice(handle: Handle<NoticeProps>) {
    return () => {
        let { tone = "info", title, children } = handle.props;
        return (
            <div data-tone={tone} mix={noticeStyle}>
                <p mix={noticeTitleStyle}>{title}</p>
                {children}
            </div>
        );
    };
}

export function Commands(handle: Handle<{ label: string; lines: string[] }>) {
    return () => {
        let { label, lines } = handle.props;
        return (
            <pre aria-label={label} mix={commandsStyle} tabIndex={0}>
                <code>{lines.join("\n")}</code>
            </pre>
        );
    };
}

export let sectionStyle = css({
    display: "flex",
    flexDirection: "column",
    gap: theme.space.lg,
    minWidth: 0,
    "& > h2": {
        fontSize: theme.fontSize.lg,
        fontWeight: theme.fontWeight.semibold,
        letterSpacing: theme.letterSpacing.tight,
    },
});

export let metaLineStyle = css({
    display: "flex",
    flexWrap: "wrap",
    alignItems: "center",
    gap: `${theme.space.xs} ${theme.space.lg}`,
    color: theme.colors.text.muted,
    fontSize: theme.fontSize.sm,
    "& strong": {
        color: theme.colors.text.primary,
        fontWeight: theme.fontWeight.medium,
    },
});

export let stackStyle = css({
    display: "grid",
    gap: theme.space.xxl,
    minWidth: 0,
});

export let proseStyle = css({
    maxWidth: "72ch",
    color: theme.colors.text.secondary,
    "& code": {
        fontSize: theme.fontSize.sm,
    },
});

export let definitionGridStyle = css({
    display: "grid",
    gridTemplateColumns: "minmax(9rem, max-content) minmax(0, 1fr)",
    gap: `${theme.space.sm} ${theme.space.xl}`,
    fontSize: theme.fontSize.sm,
    "& dt": {
        color: theme.colors.text.muted,
    },
    "& dd": {
        minWidth: 0,
        overflowWrap: "anywhere",
    },
    "& code": {
        fontSize: theme.fontSize.xs,
    },
    "@media (max-width: 40rem)": {
        gridTemplateColumns: "minmax(0, 1fr)",
        "& dd": { marginBottom: theme.space.md },
    },
});

let pageHeaderStyle = css({
    display: "flex",
    flexDirection: "column",
    gap: theme.space.sm,
    "& h1": {
        fontSize: theme.fontSize.xl,
        fontWeight: theme.fontWeight.semibold,
        letterSpacing: theme.letterSpacing.tight,
        lineHeight: theme.lineHeight.tight,
    },
});

let noticeStyle = css({
    display: "flex",
    flexDirection: "column",
    gap: theme.space.sm,
    maxWidth: "80ch",
    padding: `${theme.space.lg} ${theme.space.xl}`,
    border: "1px solid",
    borderRadius: theme.radius.md,
    fontSize: theme.fontSize.sm,
    '&[data-tone="info"]': {
        backgroundColor: theme.surface.lvl1,
        borderColor: theme.colors.border.subtle,
        color: theme.colors.text.secondary,
    },
    '&[data-tone="warn"]': {
        backgroundColor: theme.status.warn.fill,
        borderColor: theme.status.warn.line,
        color: theme.status.warn.text,
    },
    '&[data-tone="fail"]': {
        backgroundColor: theme.status.fail.fill,
        borderColor: theme.status.fail.line,
        color: theme.status.fail.text,
    },
    "& pre": {
        whiteSpace: "pre-wrap",
        overflowWrap: "anywhere",
        fontSize: theme.fontSize.xs,
    },
});

let noticeTitleStyle = css({
    fontWeight: theme.fontWeight.semibold,
});

let commandsStyle = css({
    overflowX: "auto",
    maxWidth: "100%",
    padding: `${theme.space.lg} ${theme.space.xl}`,
    backgroundColor: theme.surface.lvl1,
    border: `1px solid ${theme.colors.border.subtle}`,
    borderRadius: theme.radius.md,
    color: theme.colors.text.primary,
    fontSize: theme.fontSize.sm,
    lineHeight: theme.lineHeight.normal,
});
