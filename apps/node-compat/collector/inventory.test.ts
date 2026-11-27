import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import * as assert from "remix/assert";
import { it } from "remix/test";

import type { ProcessResult } from "./process.ts";

import { baselineModules, compareModule, probe, readProbe } from "./inventory.ts";

function processResult(overrides: Partial<ProcessResult>): ProcessResult {
    return {
        command: ["node", "./probe.mjs"],
        exitCode: 0,
        signal: null,
        timedOut: false,
        interrupted: false,
        spawnError: null,
        durationMs: 1,
        stdout: "",
        stderr: "",
        truncated: false,
        ...overrides,
    };
}

it("compares export names and typeof only", () => {
    let node = {
        ok: true as const,
        symbols: [
            ["readFile", "function"],
            ["constants", "object"],
            ["promises", "object"],
        ] as [string, string][],
        errors: [],
    };
    let bento = {
        ok: true as const,
        symbols: [
            ["readFile", "function"],
            ["constants", "function"],
            ["extra", "function"],
        ] as [string, string][],
        errors: [],
    };
    assert.deepEqual(compareModule("node:fs", node, bento), [
        {
            module: "node:fs",
            name: "constants",
            node: "object",
            bento: "function",
            status: "type-mismatch",
            error: null,
        },
        {
            module: "node:fs",
            name: "promises",
            node: "object",
            bento: null,
            status: "missing",
            error: null,
        },
        {
            module: "node:fs",
            name: "readFile",
            node: "function",
            bento: "function",
            status: "present",
            error: null,
        },
    ]);
});

it("keeps Bento import and inspection failures distinct from missing exports", () => {
    let node = {
        ok: true as const,
        symbols: [
            ["a", "function"],
            ["b", "object"],
        ] as [string, string][],
        errors: [],
    };
    let failed = compareModule("node:x", node, {
        ok: false,
        missingModule: false,
        message: "Error: Cannot load node:x",
    });
    assert.deepEqual(
        failed.map(entry => [entry.name, entry.status, entry.bento]),
        [
            ["a", "error", null],
            ["b", "error", null],
        ],
    );
    assert.match(failed[0]?.error ?? "", /Cannot load node:x/);
    let partial = compareModule("node:x", node, {
        ok: true,
        symbols: [["a", "function"]],
        errors: [["b", "TypeError: getter threw"]],
    });
    assert.deepEqual(
        partial.map(entry => [entry.name, entry.status]),
        [
            ["a", "present"],
            ["b", "error"],
        ],
    );
});

it("marks every export missing when Bento lacks the built-in module", () => {
    let node = {
        ok: true as const,
        symbols: [
            ["dlopen", "function"],
            ["dlclose", "function"],
        ] as [string, string][],
        errors: [],
    };
    let entries = compareModule("node:ffi", node, {
        ok: false,
        missingModule: true,
        message: 'TypeError: Cannot resolve "node:ffi"',
    });
    assert.deepEqual(
        entries.map(entry => [entry.name, entry.status, entry.bento, entry.error]),
        [
            ["dlclose", "missing", null, null],
            ["dlopen", "missing", null, null],
        ],
    );
});

it("records a Node baseline failure as an inspection error rather than an empty module", () => {
    let entries = compareModule(
        "node:x",
        { ok: false, missingModule: false, message: "exit code 1" },
        null,
    );
    assert.equal(entries.length, 1);
    assert.equal(entries[0]?.status, "error");
    assert.equal(entries[0]?.bento, null);
    assert.match(entries[0]?.error ?? "", /Node baseline.*exit code 1/);
});

it("accepts a probe only from a zero exit that printed its result", () => {
    let line = `__BENTO_COMPAT_INVENTORY__${JSON.stringify({ symbols: [["join", "function"]], errors: [] })}`;
    assert.deepEqual(readProbe(processResult({ stdout: `warning\n${line}\n` })), {
        ok: true,
        symbols: [["join", "function"]],
        errors: [],
    });
    assert.equal(
        readProbe(processResult({ stdout: `${line}\n`, exitCode: 1, stderr: "late crash" })).ok,
        false,
    );
    assert.equal(readProbe(processResult({ stdout: "nothing\n" })).ok, false);
    assert.equal(
        readProbe(processResult({ exitCode: null, signal: "SIGKILL", timedOut: true })).ok,
        false,
    );
});

it(
    "inspects each built-in in its own Node process, including prefix-only modules",
    { timeout: 60_000 },
    async t => {
        let directory = await mkdtemp(path.join(tmpdir(), "bento-compat-inventory-"));
        t.after(() => rm(directory, { recursive: true, force: true }));
        let options = {
            directory,
            environment: process.env,
            timeoutMs: 20_000,
            signal: new AbortController().signal,
        };
        let modules = await baselineModules([process.execPath], options);
        for (let name of [
            "node:fs",
            "node:test",
            "node:test/reporters",
            "node:sqlite",
            "node:sea",
        ]) {
            assert.ok(modules.includes(name), name);
        }
        assert.ok(!modules.some(name => name.startsWith("node:_")));
        let pathModule = await probe([process.execPath], "node:path", options);
        assert.ok(
            pathModule?.ok &&
                pathModule.symbols.some(([name, type]) => name === "join" && type === "function"),
        );
        let unknown = await probe([process.execPath], "node:not-a-builtin", options);
        assert.deepEqual(unknown && !unknown.ok ? unknown.missingModule : null, true);
        let globals = await probe([process.execPath], null, options);
        assert.ok(
            globals?.ok &&
                globals.symbols.some(([name, type]) => name === "process" && type === "object"),
        );
    },
);
