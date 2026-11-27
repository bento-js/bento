import * as assert from "remix/assert";
import { it } from "remix/test";

import type { ProcessResult } from "./process.ts";

import { classify } from "./classify.ts";

function result(overrides: Partial<ProcessResult> = {}): ProcessResult {
    return {
        command: ["node", "test/parallel/test-a.js"],
        exitCode: 0,
        signal: null,
        timedOut: false,
        interrupted: false,
        spawnError: null,
        durationMs: 5,
        stdout: "",
        stderr: "",
        truncated: false,
        ...overrides,
    };
}

it("passes a silent zero exit because upstream assertion scripts need not print TAP", () => {
    assert.deepEqual(classify(result(), 1000), { status: "passed", reason: null });
});

it("classifies common.skip output as skipped, not passed", () => {
    let outcome = classify(
        result({ stdout: "NOTE: setup\n1..0 # Skipped: missing ipv6 support\n" }),
        1000,
    );
    assert.deepEqual(outcome, { status: "skipped", reason: "missing ipv6 support" });
});

it("classifies an empty TAP plan as skipped", () => {
    let outcome = classify(result({ stdout: "1..0\n" }), 1000);
    assert.equal(outcome.status, "skipped");
    assert.match(outcome.reason ?? "", /empty test plan/i);
});

it("classifies node:test runs that skipped every test as skipped, in spec and TAP output", () => {
    let spec =
        "\u001b[34mℹ tests 2\u001b[39m\nℹ suites 0\nℹ pass 0\nℹ fail 0\nℹ cancelled 0\nℹ skipped 2\nℹ todo 0\n";
    let tap =
        "ok 1 - a # SKIP nope\n1..1\n# tests 1\n# suites 0\n# pass 0\n# fail 0\n# skipped 1\n# todo 0\n";
    let empty = "ℹ tests 0\nℹ suites 0\nℹ pass 0\nℹ fail 0\nℹ skipped 0\n";
    assert.equal(classify(result({ stdout: spec }), 1000).status, "skipped");
    assert.equal(classify(result({ stdout: tap }), 1000).status, "skipped");
    assert.deepEqual(classify(result({ stdout: empty }), 1000), {
        status: "skipped",
        reason: "node:test reported zero tests",
    });
});

it("passes node:test runs that executed some tests while skipping others", () => {
    let stdout =
        "ℹ tests 3\nℹ suites 0\nℹ pass 2\nℹ fail 0\nℹ cancelled 0\nℹ skipped 1\nℹ todo 0\n";
    assert.equal(classify(result({ stdout }), 1000).status, "passed");
});

it("never turns a non-zero exit into a skip", () => {
    let outcome = classify(result({ exitCode: 1, stdout: "1..0 # Skipped: no network\n" }), 1000);
    assert.deepEqual(outcome, { status: "failed", reason: "Exited with code 1" });
});

it("keeps timeouts, signals, panics and launch failures distinct from failures", () => {
    let timeout = classify(result({ exitCode: null, signal: "SIGKILL", timedOut: true }), 2500);
    assert.deepEqual(timeout, {
        status: "timeout",
        reason: "Did not exit within 2500 ms; its process group was killed",
    });
    let signal = classify(result({ exitCode: null, signal: "SIGSEGV" }), 1000);
    assert.deepEqual(signal, { status: "crashed", reason: "Terminated by SIGSEGV" });
    let panic = classify(
        result({
            exitCode: 101,
            stderr: "thread 'main' panicked at crates/x/src/lib.rs:1:1:\nboom\n",
        }),
        1000,
    );
    assert.equal(panic.status, "crashed");
    let launch = classify(result({ exitCode: null, spawnError: "spawn /missing ENOENT" }), 1000);
    assert.deepEqual(launch, {
        status: "crashed",
        reason: "Could not start: spawn /missing ENOENT",
    });
});
