import type { Handle } from "remix/component";

import { css } from "remix/component";

import type { GuestBookEntry } from "#app/data/guest-book-store.ts";

import { theme } from "#app/ui/public/theme.tsx";

export interface EntryListProps {
    entries: GuestBookEntry[];
}

export function EntryList(handle: Handle<EntryListProps>) {
    return () => {
        let { entries } = handle.props;

        if (entries.length === 0) {
            return <p mix={emptyStyle}>No one has signed yet. Be the first!</p>;
        }

        return (
            <ul mix={listStyle}>
                {entries.map(entry => (
                    <li key={entry.id} mix={entryStyle}>
                        <p mix={nameStyle}>{entry.name}</p>
                        <p mix={messageStyle}>{entry.message}</p>
                    </li>
                ))}
            </ul>
        );
    };
}

let listStyle = css({
    display: "flex",
    flexDirection: "column",
    gap: theme.space.lg,
});

let entryStyle = css({
    padding: theme.space.lg,
    borderRadius: theme.radius.lg,
    backgroundColor: theme.surface.lvl1,
    border: `1px solid ${theme.colors.border.subtle}`,
    overflowWrap: "anywhere",
});

let nameStyle = css({
    fontWeight: theme.fontWeight.medium,
    color: theme.colors.text.primary,
});

let messageStyle = css({
    marginTop: theme.space.sm,
    color: theme.colors.text.muted,
    fontSize: theme.fontSize.sm,
    whiteSpace: "pre-line",
});

let emptyStyle = css({
    padding: theme.space.xl,
    borderRadius: theme.radius.lg,
    border: `1px dashed ${theme.colors.border.default}`,
    color: theme.colors.text.muted,
    fontSize: theme.fontSize.sm,
    textAlign: "center",
});
