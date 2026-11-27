import * as assert from "remix/assert";
import { it } from "remix/test";

import type { ResultStore, RunReport } from "../lib/model.ts";

import { behavior } from "../test/fixtures.ts";
import { createReport, recordRun } from "./report.ts";
import { SUITE } from "./upstream.ts";

function memoryStore(): ResultStore & { saves: RunReport[] } {
    let saves: RunReport[] = [];
    return {
        saves,
        list: async () => [],
        get: async () => null,
        save: async report => {
            saves.push(structuredClone(report));
        },
        close: async () => {},
    };
}

function newReport(): RunReport {
    return createReport({
        kind: "behavior",
        suite: SUITE,
        timeoutMs: 1000,
        runtimes: {
            node: { version: "26.12.1", executable: "/bin/node", sha256: "a", arguments: [] },
            bento: { version: "0.1.0", executable: "/bin/bento", sha256: "b", arguments: ["run"] },
        },
        selection: { filter: "buffer", total: 10, selected: 2 },
    });
}

it("saves a run when it starts and once when it finishes, not after each case", async () => {
    let store = memoryStore();
    let report = await recordRun(store, newReport(), {
        signal: new AbortController().signal,
        verify: async () => null,
        collect: async current => {
            current.behavior.push(behavior("parallel/test-buffer-a.js", "passed"));
            current.behavior.push(behavior("parallel/test-buffer-b.js", "failed"));
        },
    });
    assert.deepEqual(
        store.saves.map(saved => [saved.state, saved.behavior.length, saved.finishedAt === null]),
        [
            ["running", 0, true],
            ["complete", 2, false],
        ],
    );
    assert.equal(report.state, "complete");
    assert.equal(report.timeoutMs, 1000);
});

it("records an interrupted run as interrupted rather than complete", async () => {
    let store = memoryStore();
    let controller = new AbortController();
    let report = await recordRun(store, newReport(), {
        signal: controller.signal,
        verify: async () => null,
        collect: async current => {
            current.behavior.push(behavior("parallel/test-buffer-a.js", "passed"));
            controller.abort();
        },
    });
    assert.equal(report.state, "interrupted");
    assert.equal(store.saves.length, 2);
});

it("records a collection error as a failed run with its message", async () => {
    let store = memoryStore();
    let report = await recordRun(store, newReport(), {
        signal: new AbortController().signal,
        verify: async () => null,
        collect: async () => {
            throw new Error("scratch disk full");
        },
    });
    assert.equal(report.state, "failed");
    assert.match(report.error ?? "", /scratch disk full/);
    assert.equal(store.saves.at(-1)?.state, "failed");
});

it("does not present a run as one build when an executable changed during collection", async () => {
    let store = memoryStore();
    let report = await recordRun(store, newReport(), {
        signal: new AbortController().signal,
        verify: async () => "The Bento executable changed during collection",
        collect: async current => {
            current.behavior.push(behavior("parallel/test-buffer-a.js", "passed"));
        },
    });
    assert.equal(report.state, "failed");
    assert.match(report.error ?? "", /Bento executable changed/);
});
