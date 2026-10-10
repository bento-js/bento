import { css } from "remix/component";

import { theme } from "#app/ui/public/theme.tsx";

/** Stacks a label, its control, and any hint or error text into one field group. */
export let fieldStyle = css({
    display: "flex",
    flexDirection: "column",
    gap: theme.space.md,
});

export let fieldLabelStyle = css({
    color: theme.colors.text.secondary,
    fontSize: theme.fontSize.sm,
    fontWeight: theme.fontWeight.medium,
    lineHeight: theme.lineHeight.tight,
});

export let fieldHintStyle = css({
    color: theme.colors.text.muted,
    fontSize: theme.fontSize.xs,
    fontVariantNumeric: "tabular-nums",
});

export let fieldErrorStyle = css({
    color: theme.colors.text.danger,
    fontSize: theme.fontSize.xs,
    fontWeight: theme.fontWeight.medium,
});
