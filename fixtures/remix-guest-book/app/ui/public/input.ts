import { css } from "remix/component";

import { theme } from "#app/ui/public/theme.tsx";

/** Styles native text controls: `<input>` and `<textarea>`. */
export let inputStyle = css({
    display: "block",
    width: "100%",
    minHeight: theme.control.height.md,
    paddingBlock: theme.space.md,
    paddingInline: theme.space.lg,
    border: `1px solid ${theme.colors.border.default}`,
    borderRadius: theme.radius.md,
    backgroundColor: theme.surface.lvl0,
    color: theme.colors.text.primary,
    fontSize: theme.fontSize.sm,
    lineHeight: "18px",
    boxShadow: `${theme.shadow.xs}, inset 0 1px 0 light-dark(rgb(255 255 255 / 0.7), rgb(255 255 255 / 0.04))`,
    transition: "border-color 150ms ease-out, outline-color 150ms ease-out",
    "&::placeholder": {
        color: theme.colors.text.muted,
    },
    "&:hover:not(:disabled)": {
        borderColor: theme.colors.border.strong,
    },
    "&:focus-visible": {
        borderColor: theme.colors.focus.ring,
        outline: `2px solid ${theme.colors.focus.ring}`,
        outlineOffset: "-1px",
    },
    '&[aria-invalid="true"]': {
        borderColor: theme.colors.border.danger,
        outlineColor: theme.colors.border.danger,
    },
    "&:disabled": {
        cursor: "not-allowed",
        opacity: 0.6,
    },
});
