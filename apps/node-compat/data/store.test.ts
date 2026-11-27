import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import * as assert from "remix/assert";
import { it } from "remix/test";

import { behavior, report } from "../test/fixtures.ts";
import { openStore } from "./store.ts";

it("retains completed and interrupted reports across restart without duplicating an updated run", async t => {
    let directory = await mkdtemp(path.join(tmpdir(), "bento-compat-store-"));
    t.after(() => rm(directory, { recursive: true, force: true }));
    let filename = path.join(directory, "results.sqlite");
    let store = await openStore(filename);
    let first = report({ state: "running", finishedAt: null, behavior: [] });
    await store.save(first);
    await store.save(report());
    await store.save(
        report({
            id: "run-2",
            state: "interrupted",
            startedAt: "2026-11-27T13:00:00.000Z",
            behavior: [behavior("a", "timeout")],
        }),
    );
    await store.close();
    let reopened = await openStore(filename);
    t.after(() => reopened.close());
    let runs = await reopened.list();
    assert.deepEqual(
        runs.map(run => [run.id, run.state]),
        [
            ["run-2", "interrupted"],
            ["run-1", "complete"],
        ],
    );
    assert.deepEqual(await reopened.get("run-1"), report());
    assert.equal(await reopened.get("missing"), null);
});
