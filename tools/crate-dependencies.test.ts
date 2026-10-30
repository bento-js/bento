import assert from "node:assert/strict";
import { test } from "node:test";

import { findDisallowedDependencies, type CargoMetadata } from "./crate-dependencies.ts";

function metadata(
    packages: Array<{
        name: string;
        member?: boolean;
        dependencies: CargoMetadata["packages"][number]["dependencies"];
    }>,
): CargoMetadata {
    return {
        packages: packages.map(pkg => ({
            id: `path+file:///repo/${pkg.name}#0.1.0`,
            name: pkg.name,
            manifest_path: `/repo/crates/${pkg.name}/Cargo.toml`,
            dependencies: pkg.dependencies,
        })),
        workspace_members: packages
            .filter(pkg => pkg.member !== false)
            .map(pkg => `path+file:///repo/${pkg.name}#0.1.0`),
    };
}

test("accepts VoidZero crates and the Deno runtime substrate", () => {
    let violations = findDisallowedDependencies(
        metadata([
            {
                name: "bento_runtime",
                dependencies: [
                    { name: "oxc", rename: null, kind: null },
                    { name: "oxc_resolver", rename: null, kind: null },
                    { name: "deno_runtime", rename: null, kind: null },
                    { name: "node_resolver", rename: null, kind: null },
                ],
            },
        ]),
    );
    assert.deepEqual(violations, []);
});

test("rejects SWC-based crates under every dependency kind", () => {
    let violations = findDisallowedDependencies(
        metadata([
            {
                name: "bento_runtime",
                dependencies: [
                    { name: "deno_ast", rename: null, kind: null },
                    { name: "swc_ecma_parser", rename: null, kind: "dev" },
                    { name: "swc", rename: null, kind: "build" },
                ],
            },
            {
                name: "bento_cli",
                dependencies: [{ name: "deno_graph", rename: null, kind: null }],
            },
        ]),
    );
    assert.deepEqual(
        violations.map(violation => [violation.crate, violation.dependency]),
        [
            ["bento_runtime", "deno_ast"],
            ["bento_runtime", "swc_ecma_parser"],
            ["bento_runtime", "swc"],
            ["bento_cli", "deno_graph"],
        ],
    );
    assert.ok(violations[0].message.includes("policy.0001"));
});

test("sees through a renamed dependency", () => {
    let violations = findDisallowedDependencies(
        metadata([
            {
                name: "bento_compiler",
                dependencies: [{ name: "swc_core", rename: "parser", kind: null }],
            },
        ]),
    );
    assert.deepEqual(
        violations.map(violation => violation.dependency),
        ["swc_core"],
    );
});

test("ignores packages outside the workspace", () => {
    let violations = findDisallowedDependencies(
        metadata([
            {
                name: "deno_node",
                member: false,
                dependencies: [{ name: "deno_ast", rename: null, kind: null }],
            },
        ]),
    );
    assert.deepEqual(violations, []);
});

test("does not mistake lookalike names for SWC crates", () => {
    let violations = findDisallowedDependencies(
        metadata([
            {
                name: "bento_task",
                dependencies: [
                    { name: "swcheck", rename: null, kind: null },
                    { name: "deno_ast_lite", rename: null, kind: null },
                ],
            },
        ]),
    );
    assert.deepEqual(violations, []);
});
