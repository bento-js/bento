import type { Handle, RemixNode } from "remix/component";

import { css } from "remix/component";
import { ImportMap } from "remix/component/server";

import { getAssetEntry } from "#app/middleware/asset-entry.ts";
import { Theme, theme } from "#app/ui/public/theme.tsx";

export interface DocumentProps {
    title?: string;
    children?: RemixNode;
}

export function Document(handle: Handle<DocumentProps>) {
    return () => {
        let { title = "New Remix App", children } = handle.props;
        let { scriptEntry, stylesheetHref } = getAssetEntry();
        let { href, importMap, preloads } = scriptEntry;

        return (
            <html lang="en" mix={css({ backgroundColor: theme.surface.lvl0 })}>
                <head>
                    <meta charSet="utf-8" />
                    <meta content="width=device-width, initial-scale=1" name="viewport" />
                    <title>{title}</title>

                    <link href="/favicon.ico" rel="icon" sizes="32x32" type="image/x-icon" />
                    <link href="/apple-touch-icon.png" rel="apple-touch-icon" sizes="180x180" />

                    <Theme />
                    <link href={stylesheetHref} rel="stylesheet" />
                    <ImportMap value={importMap} />
                    {preloads.map(preloadHref => (
                        <link href={preloadHref} key={preloadHref} rel="modulepreload" />
                    ))}
                    <script src={href} type="module" />
                </head>
                <body>{children}</body>
            </html>
        );
    };
}
