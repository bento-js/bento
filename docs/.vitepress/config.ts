import { extendConfig } from "@voidzero-dev/vitepress-theme/config.js";
import { fileURLToPath } from "node:url";
import { defineConfig, type DefaultTheme } from "vitepress";
import { groupIconMdPlugin, groupIconVitePlugin } from "vitepress-plugin-group-icons";

import { clarificationMarkerPlugin } from "./markdown/clarification.ts";
import { generateSidebar } from "./sidebar.ts";

// RecordHeader.vue resolves record IDs to page links through this extra theme key.
interface BentoThemeConfig extends DefaultTheme.Config {
    recordLinks: Record<string, string>;
}

let sidebar = generateSidebar();

// Preview deployments land on a subpath (/<repo>/pr-preview/pr-N/). Without a matching base every
// asset URL resolves against the domain root and the deployed site loads blank.
let base = process.env.DOCS_BASE ?? "/";

// The VoidZero header hardcodes its home link to "/", which escapes a subpath base.
let baseAwareVoidZeroHeader = {
    name: "base-aware-voidzero-header",
    enforce: "pre" as const,
    transform(code: string, id: string) {
        if (!id.includes("@voidzero-dev/vitepress-theme/src/components/oss/Header.vue")) {
            return null;
        }

        return code
            .replace(
                "import { useData, useRoute } from 'vitepress'",
                "import { useData, useRoute, withBase } from 'vitepress'",
            )
            .replace(
                '<a href="/" class="flex flex-col items-start justify-center -mx-2 px-2">',
                '<a :href="withBase(\'/\')" class="flex flex-col items-start justify-center -mx-2 px-2">',
            );
    },
};

let config = defineConfig<BentoThemeConfig>({
    base,
    title: "Bento",
    description: "A TypeScript runtime that belongs to the application using it.",
    srcDir: "..",
    srcExclude: [
        "**/node_modules/**",
        ".agents/**",
        ".git/**",
        ".github/**",
        ".omp/**",
        "tools/**",
        "docs/.vitepress/**",
        "crates/**",
        "examples/**",
        "fixtures/**",
        "target/**",
        "bench/**",
        "docs/public/**",
        "README.md",
        "PROCESS.md",
        "AGENTS.md",
    ],
    rewrites: {
        "docs/index.md": "index.md",
    },
    cleanUrls: true,
    markdown: {
        theme: { dark: "github-dark", light: "github-light" },
        config(md) {
            md.use(groupIconMdPlugin);
            md.use(clarificationMarkerPlugin);
        },
    },
    vite: {
        // `srcDir: ".."` moves VitePress's default public directory to the repo root, so
        // docs/public was never copied into the build. Point Vite back at it explicitly.
        publicDir: fileURLToPath(new URL("../public", import.meta.url)),
        plugins: [baseAwareVoidZeroHeader, groupIconVitePlugin()],
    },
    themeConfig: {
        logo: "/bento.png",
        outline: { level: "deep" },
        socialLinks: [{ icon: "github", link: "https://github.com/markmals/bento" }],
        nav: [
            { text: "Vision", link: "/VISION" },
            { text: "Architecture", link: "/ARCHITECTURE" },
            {
                text: "Decisions",
                link: sidebar.firstDecisionLink ?? "/decisions/README",
                activeMatch: "/decisions/",
            },
            {
                text: "Policies",
                link: sidebar.firstPolicyLink ?? "/policies/README",
                activeMatch: "/policies/",
            },
            {
                text: "Proposals",
                link: sidebar.firstProposalLink ?? "/proposals/README",
                activeMatch: "/proposals/",
            },
            {
                text: "Guides",
                link: sidebar.firstGuideLink ?? "/guides/README",
                activeMatch: "/guides/",
            },
        ],
        sidebar: {
            "/decisions/": sidebar.decisions,
            "/policies/": sidebar.policies,
            "/proposals/": sidebar.proposals,
            "/guides/": sidebar.guides,
        },
        recordLinks: sidebar.recordLinks,
        search: { provider: "local" },
    },
    head: [
        // VitePress prefixes themeConfig paths with `base`, but not raw `head` hrefs.
        ["link", { rel: "icon", type: "image/png", href: `${base}bento.png` }],
        ["link", { rel: "preconnect", href: "https://fonts.googleapis.com" }],
        [
            "link",
            { rel: "preconnect", href: "https://fonts.gstatic.com", crossOrigin: "anonymous" },
        ],
        [
            "link",
            {
                rel: "stylesheet",
                href: "https://fonts.googleapis.com/css2?family=Montserrat:ital,wght@0,600;0,700;0,800;0,900;1,600;1,700;1,800;1,900&display=swap",
            },
        ],
        [
            "link",
            {
                rel: "stylesheet",
                href: "https://fonts.googleapis.com/css2?family=JetBrains+Mono:ital,wght@0,400;0,500;0,600;0,700;1,400;1,500;1,600;1,700&display=swap",
            },
        ],
        [
            "link",
            {
                rel: "stylesheet",
                href: "https://fonts.googleapis.com/css2?family=Inter:wght@100;200;300;400;500;600;700;800;900&display=swap",
            },
        ],
    ],
});

export default extendConfig(config);
