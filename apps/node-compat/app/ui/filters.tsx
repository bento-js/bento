import type { Handle } from "remix/component";

import { css } from "remix/component";

import type { Page, ResultQuery } from "#app/ui/query.ts";

import { formatCount, withSearch } from "#app/ui/format.ts";
import { queryParams } from "#app/ui/query.ts";
import { theme } from "#app/ui/theme.tsx";

export interface FilterOption {
    value: string;
    label: string;
}

export interface FilterFormProps {
    action: string;
    label: string;
    query: ResultQuery<string>;
    modules: string[];
    statuses: FilterOption[];
    searchLabel: string;
}

export function FilterForm(handle: Handle<FilterFormProps>) {
    return () => {
        let { action, label, query, modules, statuses, searchLabel } = handle.props;
        let moduleOptions =
            query.module === "" || modules.includes(query.module)
                ? modules
                : [query.module, ...modules];
        let active = query.module !== "" || query.status !== "" || query.q !== "";
        return (
            <form action={action} aria-label={label} method="get" mix={formStyle} role="search">
                <div mix={fieldStyle}>
                    <label htmlFor="filter-module">Module</label>
                    <select id="filter-module" name="module" value={query.module}>
                        <option value="">All modules</option>
                        {moduleOptions.map(module => (
                            <option key={module} value={module}>
                                {module}
                            </option>
                        ))}
                    </select>
                </div>
                <div mix={fieldStyle}>
                    <label htmlFor="filter-status">Status</label>
                    <select id="filter-status" name="status" value={query.status}>
                        {statuses.map(option => (
                            <option key={option.value} value={option.value}>
                                {option.label}
                            </option>
                        ))}
                    </select>
                </div>
                <div data-grow="true" mix={fieldStyle}>
                    <label htmlFor="filter-search">{searchLabel}</label>
                    <input id="filter-search" name="q" type="search" value={query.q} />
                </div>
                <div mix={actionsStyle}>
                    <button type="submit">Apply filters</button>
                    {active ? <a href={action}>Clear filters</a> : null}
                </div>
            </form>
        );
    };
}

export interface PaginationProps {
    action: string;
    query: ResultQuery<string>;
    page: Page<unknown>;
    noun: string;
}

export function Pagination(handle: Handle<PaginationProps>) {
    return () => {
        let { action, query, page, noun } = handle.props;
        let summary =
            page.total === 0
                ? `No ${noun} match these filters.`
                : `Showing ${formatCount(page.start + 1)}–${formatCount(page.start + page.items.length)} of ${formatCount(page.total)} ${noun}`;
        return (
            <nav aria-label={`${noun} pages`} mix={paginationStyle}>
                <p>{summary}</p>
                {page.pages > 1 ? (
                    <ul>
                        <li>
                            {page.page > 1 ? (
                                <a
                                    href={withSearch(action, queryParams(query, page.page - 1))}
                                    rel="prev"
                                >
                                    Previous
                                </a>
                            ) : null}
                        </li>
                        <li aria-current="page">
                            Page {page.page} of {page.pages}
                        </li>
                        <li>
                            {page.page < page.pages ? (
                                <a
                                    href={withSearch(action, queryParams(query, page.page + 1))}
                                    rel="next"
                                >
                                    Next
                                </a>
                            ) : null}
                        </li>
                    </ul>
                ) : null}
            </nav>
        );
    };
}

let formStyle = css({
    display: "flex",
    flexWrap: "wrap",
    alignItems: "flex-end",
    gap: theme.space.lg,
    "& select, & input": {
        height: theme.control.height,
        minWidth: "10rem",
        maxWidth: "100%",
        padding: `0 ${theme.space.md}`,
        backgroundColor: theme.surface.lvl0,
        border: `1px solid ${theme.colors.border.default}`,
        borderRadius: theme.radius.md,
        fontSize: theme.fontSize.md,
    },
    "& select:hover, & input:hover": {
        borderColor: theme.colors.border.strong,
    },
    "& input": {
        width: "100%",
    },
});

let fieldStyle = css({
    display: "flex",
    flexDirection: "column",
    gap: theme.space.xs,
    "& label": {
        color: theme.colors.text.secondary,
        fontSize: theme.fontSize.xs,
        fontWeight: theme.fontWeight.medium,
    },
    '&[data-grow="true"]': {
        flex: "1 1 16rem",
    },
});

let actionsStyle = css({
    display: "flex",
    alignItems: "center",
    gap: theme.space.lg,
    "& button": {
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
    "& a": {
        fontSize: theme.fontSize.sm,
    },
});

let paginationStyle = css({
    display: "flex",
    flexWrap: "wrap",
    alignItems: "center",
    justifyContent: "space-between",
    gap: theme.space.lg,
    color: theme.colors.text.muted,
    fontSize: theme.fontSize.sm,
    "& ul": {
        display: "flex",
        alignItems: "center",
        gap: theme.space.xl,
    },
    "& li:empty": {
        display: "none",
    },
});
