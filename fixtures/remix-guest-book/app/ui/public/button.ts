import type { CSSMixinDescriptor } from "remix/component";

import { css } from "remix/component";

import { theme } from "#app/ui/public/theme.tsx";

type ActionColors = typeof theme.colors.action.primary;

export type ButtonTone = keyof typeof theme.colors.action;

export interface ButtonOptions {
    tone?: ButtonTone;
}

let baseStyle = css({
    display: "inline-flex",
    alignItems: "center",
    justifyContent: "center",
    gap: theme.space.md,
    minHeight: theme.control.height.md,
    paddingInline: theme.space.xl,
    border: "1px solid transparent",
    borderRadius: theme.radius.md,
    fontSize: theme.fontSize.sm,
    fontWeight: theme.fontWeight.medium,
    lineHeight: 1,
    whiteSpace: "nowrap",
    textDecoration: "none",
    cursor: "pointer",
    userSelect: "none",
    transition:
        "background-color 150ms ease-out, border-color 150ms ease-out, box-shadow 150ms ease-out",
    "&:focus-visible": {
        outline: `2px solid ${theme.colors.focus.ring}`,
        outlineOffset: "2px",
    },
    "&:disabled": {
        cursor: "not-allowed",
        opacity: 0.6,
    },
});

function toneStyle(colors: ActionColors) {
    return css({
        backgroundColor: colors.background,
        borderColor: colors.border,
        color: colors.foreground,
        boxShadow: `${theme.shadow.xs}, inset 0 1px 0 rgb(255 255 255 / 0.12)`,
        "&:hover:not(:disabled)": {
            backgroundColor: colors.backgroundHover,
        },
        "&:active:not(:disabled)": {
            backgroundColor: colors.backgroundActive,
            boxShadow: "inset 0 1px 2px rgb(0 0 0 / 0.2)",
        },
    });
}

let toneStyles: Record<ButtonTone, CSSMixinDescriptor> = {
    primary: toneStyle(theme.colors.action.primary),
    secondary: toneStyle(theme.colors.action.secondary),
    danger: toneStyle(theme.colors.action.danger),
};

/** Styles a native `<button>`, or a link that acts as one, in one of the theme's action tones. */
export function button({ tone = "secondary" }: ButtonOptions = {}) {
    return [baseStyle, toneStyles[tone]];
}
