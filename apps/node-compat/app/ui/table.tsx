import type { Handle, RemixNode } from "remix/component";

import { css } from "remix/component";

import { formatCount } from "#app/ui/format.ts";
import { theme } from "#app/ui/theme.tsx";

export interface TableRegionProps {
    labelledBy: string;
    tall?: boolean;
    children?: RemixNode;
}

export function TableRegion(handle: Handle<TableRegionProps>) {
    return () => {
        let { labelledBy, tall = false, children } = handle.props;
        return (
            <div
                aria-labelledby={labelledBy}
                data-tall={tall ? "true" : undefined}
                mix={regionStyle}
                role="region"
                tabIndex={0}
            >
                {children}
            </div>
        );
    };
}

export interface CountProps {
    value: number;
    href?: string;
    tone?: "fail" | "warn";
}

export function Count(handle: Handle<CountProps>) {
    return () => {
        let { value, href, tone } = handle.props;
        let text = formatCount(value);
        let flagged = value > 0 ? tone : undefined;
        return (
            <td class="num" data-flag={flagged} data-zero={value === 0 ? "true" : undefined}>
                {href !== undefined && value > 0 ? <a href={href}>{text}</a> : text}
            </td>
        );
    };
}

let regionStyle = css({
    overflow: "auto",
    border: `1px solid ${theme.colors.border.subtle}`,
    borderRadius: theme.radius.md,
    '&[data-tall="true"]': {
        maxHeight: "min(70vh, 48rem)",
    },
});

export let dataTableStyle = css({
    width: "100%",
    fontSize: theme.fontSize.sm,
    fontVariantNumeric: "tabular-nums",
    "& caption": {
        captionSide: "top",
        padding: `${theme.space.md} ${theme.space.lg}`,
        textAlign: "left",
        color: theme.colors.text.muted,
        fontSize: theme.fontSize.xs,
        borderBottom: `1px solid ${theme.colors.border.subtle}`,
    },
    "& th, & td": {
        padding: `6px ${theme.space.lg}`,
        borderBottom: `1px solid ${theme.colors.border.subtle}`,
        textAlign: "left",
        verticalAlign: "top",
    },
    "& tbody tr:last-child th, & tbody tr:last-child td": {
        borderBottom: "none",
    },
    "& thead th": {
        position: "sticky",
        top: 0,
        zIndex: 1,
        backgroundColor: theme.surface.lvl1,
        borderBottom: `1px solid ${theme.colors.border.default}`,
        color: theme.colors.text.secondary,
        fontSize: theme.fontSize.xs,
        fontWeight: theme.fontWeight.semibold,
        whiteSpace: "nowrap",
    },
    "& tbody th": {
        fontWeight: theme.fontWeight.normal,
    },
    "& tbody tr:hover": {
        backgroundColor: theme.surface.lvl1,
    },
    '& tr[aria-current="true"]': {
        backgroundColor: theme.surface.lvl2,
    },
    "& .num": {
        textAlign: "right",
        whiteSpace: "nowrap",
    },
    '& td[data-zero="true"]': {
        color: theme.colors.text.muted,
    },
    '& td[data-flag="fail"]': {
        color: theme.status.fail.text,
        fontWeight: theme.fontWeight.semibold,
    },
    '& td[data-flag="warn"]': {
        color: theme.status.warn.text,
        fontWeight: theme.fontWeight.semibold,
    },
    "& td[data-flag] a": {
        color: "inherit",
    },
    "& .mono": {
        fontFamily: theme.fontFamily.mono,
        fontSize: theme.fontSize.xs,
    },
    "& .wrap": {
        overflowWrap: "anywhere",
        minWidth: "14rem",
    },
    "& .muted": {
        color: theme.colors.text.muted,
    },
    "& tbody a": {
        textDecoration: "none",
    },
    "& tbody a:hover, & tbody a:focus-visible": {
        textDecoration: "underline",
    },
    "& td.num a": {
        textDecoration: "underline",
    },
});
