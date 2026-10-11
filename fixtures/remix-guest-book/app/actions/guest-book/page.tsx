import type { Handle } from "remix/component";
import type { Issue } from "remix/data-schema";

import { css } from "remix/component";

import type { GuestBookFormValues } from "#app/actions/guest-book/schema.ts";
import type { GuestBookEntry } from "#app/data/guest-book-store.ts";

import { EntryList } from "#app/actions/guest-book/entry-list.tsx";
import { SignForm } from "#app/actions/guest-book/sign-form.tsx";
import { Document } from "#app/ui/document.tsx";
import { theme } from "#app/ui/public/theme.tsx";

export interface GuestBookPageProps {
    entries: GuestBookEntry[];
    issues?: readonly Issue[];
    values?: GuestBookFormValues;
}

export function GuestBookPage(handle: Handle<GuestBookPageProps>) {
    return () => {
        let { entries, issues, values } = handle.props;

        return (
            <Document>
                <main mix={pageStyle}>
                    <header mix={headerStyle}>
                        <picture>
                            <source
                                media="(prefers-color-scheme: dark)"
                                srcSet="/remix-3-logo-dark.svg"
                            />
                            <img
                                alt="Remix 3"
                                mix={css({ height: "2.5rem" })}
                                src="/remix-3-logo-light.svg"
                            />
                        </picture>
                        <h1 mix={titleStyle}>Welcome to Remix 3</h1>
                    </header>

                    <nav mix={navStyle}>
                        <ResourceLink href="https://github.com/remix-run/remix" label="GitHub" />
                        <ResourceLink href="https://discord.gg/xwx7mMzVkA" label="Discord" />
                    </nav>

                    <section aria-labelledby="guest-book-heading" mix={sectionStyle}>
                        <div mix={entriesStyle}>
                            <h2 id="guest-book-heading" mix={headingStyle}>
                                Guest Book
                            </h2>
                            <EntryList entries={entries} />
                        </div>
                        <SignForm
                            formKey={`sign-${entries.length}`}
                            issues={issues}
                            values={values}
                        />
                    </section>
                </main>
            </Document>
        );
    };
}

interface ResourceLinkProps {
    href: string;
    label: string;
}

function ResourceLink(handle: Handle<ResourceLinkProps>) {
    return () => {
        let { href, label } = handle.props;

        return (
            <a href={href} mix={resourceLinkStyle} target="_blank">
                {label}
            </a>
        );
    };
}

let pageStyle = css({
    display: "flex",
    flexDirection: "column",
    alignItems: "center",
    minHeight: "100vh",
    fontFamily: theme.fontFamily.sans,
    color: theme.colors.text.primary,
    padding: "4rem 1rem",
});

let headerStyle = css({
    display: "flex",
    flexDirection: "column",
    alignItems: "center",
    gap: theme.space.xl,
    maxWidth: "32rem",
});

let titleStyle = css({
    fontSize: "2.25rem",
    fontWeight: theme.fontWeight.bold,
    letterSpacing: theme.letterSpacing.tight,
});

let navStyle = css({
    display: "flex",
    gap: theme.space.xxl,
    marginTop: "2rem",
});

let sectionStyle = css({
    marginTop: "3rem",
    width: "100%",
    maxWidth: "28rem",
    display: "flex",
    flexDirection: "column",
    gap: "2rem",
});

let entriesStyle = css({
    display: "flex",
    flexDirection: "column",
    gap: "0.5rem",
});

let headingStyle = css({
    fontSize: theme.fontSize.xl,
    fontWeight: theme.fontWeight.semibold,
});

let resourceLinkStyle = css({
    color: theme.colors.text.link,
    fontSize: theme.fontSize.md,
    fontWeight: theme.fontWeight.medium,
    "&:hover": {
        color: "light-dark(#1e40af, #93c5fd)",
    },
});
