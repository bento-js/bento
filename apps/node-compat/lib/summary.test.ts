import * as assert from "remix/assert";
import { it } from "remix/test";

import { behavior, report } from "../test/fixtures.ts";
import { summarizeRun } from "./summary.ts";

it("does not count a Bento success with a failing Node control as compatibility", () => {
    let summary = summarizeRun(report({ behavior: [behavior("a", "passed", "failed")] }));
    assert.equal(summary.passed, 0);
    assert.equal(summary.baselineFailed, 1);
});

it("keeps missing, skipped, unsupported and timed-out cases in the selected denominator", () => {
    let summary = summarizeRun(
        report({
            state: "interrupted",
            selection: { filter: "", total: 100, selected: 7 },
            behavior: [
                behavior("a", "passed"),
                behavior("b", "failed"),
                behavior("c", "skipped"),
                behavior("d", "unsupported"),
                behavior("e", "timeout"),
            ],
        }),
    );
    assert.equal(summary.selected, 7);
    assert.equal(summary.passed, 1);
    assert.equal(summary.failed, 1);
    assert.equal(summary.skipped, 1);
    assert.equal(summary.unsupported, 1);
    assert.equal(summary.timeout, 1);
    assert.equal(summary.notRun, 2);
});

it("does not turn a present export into a behavioral pass", () => {
    let summary = summarizeRun(
        report({
            kind: "inventory",
            behavior: [],
            inventory: [
                {
                    module: "node:fs",
                    name: "readFile",
                    node: "function",
                    bento: "function",
                    status: "present",
                    error: null,
                },
            ],
        }),
    );
    assert.equal(summary.passed, 0);
    assert.equal(summary.available, 1);
});
