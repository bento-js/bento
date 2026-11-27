import type { Handle, RemixNode } from "remix/component";

import { css } from "remix/component";

import { routes } from "#app/routes.ts";
import { Theme, theme } from "#app/ui/theme.tsx";
import { stylesheetHref } from "#app/utils/assets.ts";

export type Section = "behavior" | "inventory" | "history" | "compare";

export interface DocumentProps {
    title: string;
    section?: Section;
    children?: RemixNode;
}

const NAVIGATION: { section: Section; label: string; href: string }[] = [
    { section: "behavior", label: "Behavior", href: routes.home.href() },
    { section: "inventory", label: "Availability", href: routes.inventory.href() },
    { section: "history", label: "History", href: routes.runs.index.href() },
    { section: "compare", label: "Compare", href: routes.compare.href() },
];

export function Document(handle: Handle<DocumentProps>) {
    return () => {
        let { title, section, children } = handle.props;
        return (
            <html lang="en">
                <head>
                    <meta charSet="utf-8" />
                    <meta content="width=device-width, initial-scale=1" name="viewport" />
                    <title>{`${title} · Bento Node compatibility`}</title>
                    <Theme />
                    <link href={stylesheetHref} rel="stylesheet" />
                </head>
                <body>
                    <a href="#main" mix={skipLinkStyle}>
                        Skip to content
                    </a>
                    <header mix={headerStyle}>
                        <div mix={barStyle}>
                            <a href={routes.home.href()} mix={brandStyle}>
                                Bento <span mix={brandDetailStyle}>Node compatibility</span>
                            </a>
                            <nav aria-label="Dashboard">
                                <ul mix={navListStyle}>
                                    {NAVIGATION.map(item => (
                                        <li key={item.section}>
                                            <a
                                                aria-current={
                                                    item.section === section ? "page" : undefined
                                                }
                                                href={item.href}
                                            >
                                                {item.label}
                                            </a>
                                        </li>
                                    ))}
                                </ul>
                            </nav>
                        </div>
                    </header>
                    <main id="main" mix={mainStyle} tabIndex={-1}>
                        {children}
                    </main>
                </body>
            </html>
        );
    };
}

let skipLinkStyle = css({
    position: "absolute",
    left: theme.space.md,
    top: "-3rem",
    zIndex: 10,
    padding: `${theme.space.sm} ${theme.space.lg}`,
    backgroundColor: theme.surface.lvl0,
    border: `1px solid ${theme.colors.border.default}`,
    borderRadius: theme.radius.md,
    "&:focus": { top: theme.space.md },
});

let headerStyle = css({
    borderBottom: `1px solid ${theme.colors.border.subtle}`,
    backgroundColor: theme.surface.lvl1,
});

let barStyle = css({
    display: "flex",
    flexWrap: "wrap",
    alignItems: "center",
    justifyContent: "space-between",
    gap: `${theme.space.md} ${theme.space.xxl}`,
    maxWidth: "90rem",
    margin: "0 auto",
    padding: `${theme.space.lg} ${theme.space.xxl}`,
    "@media (max-width: 40rem)": {
        padding: `${theme.space.md} ${theme.space.xl}`,
    },
});

let brandStyle = css({
    color: theme.colors.text.primary,
    fontWeight: theme.fontWeight.semibold,
    textDecoration: "none",
    "&:hover": { color: theme.colors.text.primary },
});

let brandDetailStyle = css({
    color: theme.colors.text.muted,
    fontWeight: theme.fontWeight.normal,
});

let navListStyle = css({
    display: "flex",
    flexWrap: "wrap",
    gap: theme.space.sm,
    "& a": {
        display: "inline-block",
        padding: `${theme.space.sm} ${theme.space.lg}`,
        borderRadius: theme.radius.md,
        color: theme.colors.text.secondary,
        textDecoration: "none",
    },
    "& a:hover": {
        color: theme.colors.text.primary,
        backgroundColor: theme.surface.lvl2,
    },
    '& a[aria-current="page"]': {
        color: theme.colors.text.link,
        backgroundColor: theme.surface.lvl0,
        boxShadow: `inset 0 0 0 1px ${theme.colors.border.default}`,
        fontWeight: theme.fontWeight.medium,
    },
});

let mainStyle = css({
    display: "flex",
    flexDirection: "column",
    gap: theme.space.xxl,
    maxWidth: "90rem",
    margin: "0 auto",
    padding: `${theme.space.xxl} ${theme.space.xxl} ${theme.space.xxxl}`,
    "&:focus": { outline: "none" },
    "@media (max-width: 40rem)": {
        padding: `${theme.space.xl} ${theme.space.xl} ${theme.space.xxl}`,
    },
});
