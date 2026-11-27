import { defineConfig } from "vitest/config";

// `mise run test` runs tools/ with `node --test`; the benchmark harness stays on Vitest
// because its suites mock modules with `vi.mock`/`vi.hoisted`, which `node:test` has no
// equivalent for. Naming it here keeps Vitest from collecting the `node:test` suites in tools/.
export default defineConfig({
    test: {
        include: ["bench/**/*.test.ts"],
    },
});
