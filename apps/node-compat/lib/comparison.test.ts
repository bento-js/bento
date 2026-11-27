import * as assert from "remix/assert";
import { it } from "remix/test";

import { behavior, report } from "../test/fixtures.ts";
import { compareRuns } from "./comparison.ts";

it("finds regressions and improvements by test identity rather than row order", () => {
    let selection = { filter: "buffer", total: 5000, selected: 2 };
    let previous = report({
        selection,
        behavior: [behavior("a", "passed"), behavior("b", "failed")],
    });
    let current = report({
        selection,
        behavior: [behavior("b", "passed"), behavior("a", "timeout")],
    });
    let changes = compareRuns(current, previous);
    assert.equal(changes.comparable, true);
    assert.deepEqual(
        changes.regressions.map(test => test.id),
        ["a"],
    );
    assert.deepEqual(
        changes.improvements.map(test => test.id),
        ["b"],
    );
});

it("refuses to call a changed corpus or interrupted run a regression", () => {
    let previous = report();
    for (let current of [
        report({ state: "interrupted" }),
        report({ suite: { ...previous.suite, revision: "def" } }),
        report({ behavior: [behavior("new-test", "failed")] }),
        report({ platform: "linux" }),
        report({ selection: { ...previous.selection, filter: "assert" } }),
        report({ timeoutMs: 20000 }),
        report({ selection: { ...previous.selection, selected: 2 } }),
        report({
            selection: { ...previous.selection, selected: 2 },
            behavior: [behavior("a", "passed"), behavior("a", "passed")],
        }),
    ]) {
        assert.equal(compareRuns(current, previous).comparable, false);
    }
});

it("does not attribute a broken Node baseline to a Bento regression", () => {
    let changes = compareRuns(
        report({ behavior: [behavior("a", "failed", "failed")] }),
        report({ behavior: [behavior("a", "passed")] }),
    );
    assert.equal(changes.regressions.length, 0);
});
