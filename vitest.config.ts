import { defineConfig } from "vitest/config";

// `mise run test` runs the record tools with `node --test`; these two trees stay on
// Vitest because their suites mock modules with `vi.mock`/`vi.hoisted`, which
// `node:test` has no equivalent for. Naming both here keeps Vitest from collecting
// the `node:test` suites in tools/.
export default defineConfig({
    test: {
        include: [".omp/tools/**/*.test.ts", "bench/**/*.test.ts"],
    },
});
