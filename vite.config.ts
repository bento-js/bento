import { defineConfig } from "vite-plus";

export default defineConfig({
    test: {
        include: ["tools/**/*.test.ts", ".omp/tools/**/*.test.ts"],
    },
    fmt: {
        // The fixture is an independent project with its own toolchain and lockfile.
        ignorePatterns: ["fixtures/**", "target/**"],
        printWidth: 100,
        tabWidth: 4,
        arrowParens: "avoid",
        sortPackageJson: true,
        sortImports: {
            groups: [
                "type-import",
                ["value-builtin", "value-external"],
                "type-internal",
                "value-internal",
                ["type-parent", "type-sibling", "type-index"],
                ["value-parent", "value-sibling", "value-index"],
                "unknown",
            ],
            partitionByComment: true,
        },
        overrides: [
            {
                files: ["**/*.md", "**/*.mdx"],
                options: { proseWrap: "never" },
            },
            {
                files: ["**/*.jsonc"],
                options: { trailingComma: "none" },
            },
        ],
    },
    lint: {
        ignorePatterns: ["fixtures/**", "target/**"],
        options: {
            typeAware: true,
            typeCheck: true,
        },
        jsPlugins: [
            {
                name: "vite-plus",
                specifier: "vite-plus/oxlint-plugin",
            },
            "eslint-plugin-perfectionist",
            "eslint-plugin-prefer-let",
        ],
        rules: {
            "vite-plus/prefer-vite-plus-imports": "error",
            "typescript/no-floating-promises": "allow",
            "typescript/unbound-method": "allow",
            "perfectionist/sort-jsx-props": "warn",
            "import/extensions": [
                "error",
                "ignorePackages",
                {
                    cjs: "always",
                    cts: "always",
                    js: "always",
                    jsx: "always",
                    mjs: "always",
                    mts: "always",
                    ts: "always",
                    tsx: "always",
                },
            ],
            "eslint/prefer-const": "off",
            "prefer-let/prefer-let": [2, { forceUpperCaseConst: true }],
        },
    },
    // Bento's own test and build tasks join `check` as its code lands.
    run: {
        tasks: {
            check: {
                command: "vp check",
                dependsOn: ["validate", "test", "docs:build"],
            },
            validate: "node tools/validate.ts",
            test: "vp test run",
            // Reads live git and GitHub state, so a replayed result would be stale.
            status: {
                command: "node tools/status.ts",
                cache: false,
            },
            "docs:dev": {
                command: "vitepress dev docs",
                cache: false,
            },
            // Tasks run in a filtered environment; DOCS_BASE must be listed to reach VitePress.
            "docs:build": {
                command: "vitepress build docs",
                cache: { env: ["DOCS_BASE"] },
            },
            "docs:preview": {
                command: "vitepress preview docs",
                dependsOn: ["docs:build"],
                cache: false,
            },
        },
    },
});
