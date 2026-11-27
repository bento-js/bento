import type { TestContext } from "remix/test";

import * as assert from "remix/assert";
import { describe, it } from "remix/test";

import { createAppRouter } from "#app/router.ts";

import type { RunReport } from "../../lib/model.ts";

import { openStore } from "../../data/store.ts";
import { behavior, execution, report } from "../../test/fixtures.ts";

const ORIGIN = "http://127.0.0.1:4318";
const HOSTILE_ID = "parallel/test-<script>alert(1)</script>.js";

async function serve(t: TestContext, reports: RunReport[] = []) {
    let results = await openStore(":memory:");
    t.after(() => results.close());
    for (let run of reports) await results.save(run);
    let router = createAppRouter({ results });
    return (path: string) => router.fetch(new URL(path, ORIGIN));
}

function hostileReport(): RunReport {
    return report({
        behavior: [
            {
                id: HOSTILE_ID,
                module: "<b>buffer</b>",
                flags: ["--expose-internals"],
                node: { ...execution("passed"), stdout: "<script>alert(2)</script>" },
                bento: {
                    ...execution("failed"),
                    command: ["/bin/bento", "run", HOSTILE_ID],
                    stderr: "</pre><img src=x onerror=alert(3)>",
                    reason: "<i>assertion</i>",
                    truncated: true,
                },
            },
        ],
    });
}

describe("dashboard routes", () => {
    it("shows real collection commands and no results before any run is saved", async t => {
        let get = await serve(t);

        let response = await get("/");

        assert.equal(response.status, 200);
        let html = await response.text();
        assert.match(html, /mise run compat:collect -- --bento \.\.\/\.\.\/target\/debug\/bento/);
        assert.doesNotMatch(html, /<table/);
    });

    it("escapes hostile test paths, logs, reasons, and search text", async t => {
        let get = await serve(t, [hostileReport()]);

        let listing = await get(`/?q=${encodeURIComponent("<script>alert(1)</script>")}`);
        let detail = await get(`/runs/run-1/test?${new URLSearchParams({ id: HOSTILE_ID })}`);

        assert.equal(detail.status, 200);
        assert.match(detail.headers.get("Content-Security-Policy") ?? "", /script-src 'none'/);
        for (let html of [await listing.text(), await detail.text()]) {
            assert.match(html, /&lt;script&gt;alert\(1\)&lt;\/script&gt;/);
            assert.doesNotMatch(html, /<script>alert|<img src=x|<b>buffer|<i>assertion/);
        }
    });

    it("shows both invocations, flags, and truncation on the test page", async t => {
        let get = await serve(t, [hostileReport()]);

        let html = await (
            await get(`/runs/run-1/test?${new URLSearchParams({ id: HOSTILE_ID })}`)
        ).text();

        assert.match(html, /node test\.js/);
        assert.match(html, /\/bin\/bento run (?:'|&#39;)parallel\/test-&lt;script&gt;/);
        assert.match(html, /--expose-internals/);
        assert.match(html, /Truncated at the collector(?:'|&#39;)s output limit/);
    });

    it("returns 404 for unknown runs and test files", async t => {
        let get = await serve(t, [report()]);

        for (let path of [
            "/runs/missing",
            "/runs/missing/test?id=parallel/test-buffer.js",
            "/runs/run-1/test?id=parallel/test-unknown.js",
            "/runs/run-1/test",
            "/compare?current=missing&previous=run-1",
        ]) {
            let response = await get(path);
            assert.equal(response.status, 404, path);
            assert.match(await response.text(), /not found/i, path);
        }
    });

    it("does not list a Bento pass whose Node control failed as passed", async t => {
        let get = await serve(t, [
            report({
                selection: { filter: "buffer", total: 5000, selected: 2 },
                behavior: [
                    behavior("control-broken.js", "passed", "failed"),
                    behavior("ok.js", "passed"),
                ],
            }),
        ]);

        let passed = await (await get("/?status=passed")).text();
        let control = await (await get("/?status=control")).text();

        assert.match(passed, /ok\.js/);
        assert.doesNotMatch(passed, /control-broken\.js/);
        assert.match(control, /control-broken\.js/);
        assert.match(control, /Not counted: Node control failed/);
    });

    it("keeps unrecorded files of an interrupted run in the denominator", async t => {
        let get = await serve(t, [
            report({
                state: "interrupted",
                selection: { filter: "", total: 5000, selected: 3 },
                behavior: [behavior("a.js", "passed")],
            }),
        ]);

        let html = await (await get("/runs/run-1")).text();

        assert.match(html, /Collection was interrupted/);
        assert.match(html, /2 of 3 selected files have no result/);
        assert.doesNotMatch(html, /status=not-run/);
        assert.match(html, /Not compared: only complete runs are compared/);
    });

    it("refuses to report regressions between runs with different selections", async t => {
        let get = await serve(t, [
            report({ id: "before", behavior: [behavior("a.js", "passed")] }),
            report({
                id: "after",
                startedAt: "2026-11-28T12:00:00.000Z",
                selection: { filter: "assert", total: 5000, selected: 1 },
                behavior: [behavior("a.js", "failed")],
            }),
        ]);

        let html = await (await get("/compare?current=after&previous=before")).text();

        assert.match(html, /These runs are not comparable/);
        assert.match(html, /The test selection changed\./);
        assert.doesNotMatch(html, /Regressions \(/);
    });

    it("compares the latest run with the newest comparable earlier run", async t => {
        let get = await serve(t, [
            report({ id: "comparable", behavior: [behavior("a.js", "passed")] }),
            report({
                id: "different-deadline",
                startedAt: "2026-11-27T13:00:00.000Z",
                timeoutMs: 20_000,
                behavior: [behavior("a.js", "passed")],
            }),
            report({
                id: "latest",
                startedAt: "2026-11-27T14:00:00.000Z",
                behavior: [behavior("a.js", "failed")],
            }),
        ]);

        let html = await (await get("/")).text();

        assert.match(html, /Compared with the run from/);
        assert.match(html, /1 regression/);
        assert.match(html, /previous=comparable/);
    });
});
