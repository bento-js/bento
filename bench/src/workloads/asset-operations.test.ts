import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { afterAll, beforeAll, describe, expect, test } from "vitest";

import type { RuntimeName, Series } from "../results-model.ts";
import type { RunContext } from "../workload.ts";

import { parseOptions } from "../options.ts";
import { runWorkload } from "../run-workload.ts";
import { CLEAN_SOURCES, createProbeFixture } from "../test-support/probe-fixture.ts";
import { assetOperations } from "./asset-operations.ts";

function measured(runtime: RuntimeName, digests: Record<string, string>): Series {
    return {
        runtime,
        variant: "",
        outcome: { status: "measured", failures: [], samples: [{ trial: 0, digests }] },
    };
}

const SAME = { parse: "aa", resolve: "bb", transform: "cc", minify: "dd", css: "ee" };

test("identical output digests across runtimes are reported as identical work", () => {
    let notes = assetOperations.notes?.([measured("node", SAME), measured("bun", SAME)]);

    expect(notes).toEqual([
        "Every measured runtime produced identical outputs for parse, resolve, transform, minify, css (output digests match).",
    ]);
});

test("a differing digest is reported, naming each runtime's digests", () => {
    let notes = assetOperations.notes?.([
        measured("node", SAME),
        measured("bun", { ...SAME, minify: "zz" }),
    ]);

    expect(notes).toHaveLength(1);
    expect(notes?.[0]).toContain("Output digests differ, so the timed work was not identical");
    expect(notes?.[0]).toContain('"minify":"dd"');
    expect(notes?.[0]).toContain('"minify":"zz"');
});

test("runtimes that did not run contribute no digests", () => {
    let notes = assetOperations.notes?.([
        measured("node", SAME),
        {
            runtime: "bento",
            variant: "",
            outcome: { status: "unavailable", reason: "BENTO_BIN is not set" },
        },
    ]);

    expect(notes?.[0]).toContain("identical outputs");
});

test("no measured runtime means nothing to compare", () => {
    expect(assetOperations.notes?.([])).toEqual([]);
});

const BROKEN_SOURCES = {
    ...CLEAN_SOURCES,
    "app/actions/public/entry.ts": "export const broken = <;",
};
const PARSE_DIAGNOSTIC = expect.stringMatching(
    /^node asset operations probe failed \(exit code 1\):\nparse failed on app\/actions\/public\/entry\.ts: Unexpected token\n$/,
);

describe("classifying a runtime from its probe runs", () => {
    let parent: string;

    beforeAll(async () => {
        parent = await mkdtemp(path.join(tmpdir(), "bento-bench-asset-operations-"));
    });

    afterAll(async () => {
        await rm(parent, { recursive: true, force: true });
    });

    /** Runs one Node trial per entry of `sourcesByTrial`, each in its own fixture with those sources. */
    async function runForNode(sourcesByTrial: Array<Record<string, string>>) {
        let directories = await Promise.all(
            sourcesByTrial.map(sources => createProbeFixture(parent, { sources })),
        );
        // The harness asks for the runtime's fixture once per trial, in trial order.
        let trialsStarted = 0;
        let context: RunContext = {
            options: parseOptions([
                "--runtimes",
                "node",
                "--workloads",
                "asset-operations",
                "--trials",
                String(sourcesByTrial.length),
                "--iterations",
                "2",
            ]),
            availability: {
                node: { available: true, executable: process.execPath, version: process.version },
                bun: { available: false, reason: "not used" },
                bento: { available: false, reason: "not used" },
            },
            fixtureFor: async () => ({
                directory: directories[trialsStarted++]!,
                databaseUrl: "",
                remixVersion: "",
            }),
            compatibility: async () => {
                throw new Error("asset-operations does not use the application's gates");
            },
            ohaExecutable: undefined,
            log: () => {},
        };
        return runWorkload(assetOperations, context);
    }

    test("a runtime whose operations all succeed is measured", async () => {
        let result = await runForNode([CLEAN_SOURCES, CLEAN_SOURCES]);

        expect(result.series).toMatchObject([
            {
                runtime: "node",
                outcome: {
                    status: "measured",
                    failures: [],
                    samples: [{ trial: 0 }, { trial: 1 }],
                },
            },
        ]);
        expect(result.notes[0]).toContain("identical outputs");
    });

    test("an operation's diagnostic fails the runtime with no timing, naming the operation and diagnostic", async () => {
        let result = await runForNode([BROKEN_SOURCES, BROKEN_SOURCES]);

        expect(result.series).toEqual([
            {
                runtime: "node",
                variant: "",
                outcome: {
                    status: "failed",
                    failures: [
                        { trial: 0, message: PARSE_DIAGNOSTIC },
                        { trial: 1, message: PARSE_DIAGNOSTIC },
                    ],
                },
            },
        ]);
        expect(result.notes).toEqual([]);
    });

    test("one trial's diagnostic fails the runtime, discarding the timings of its other trials", async () => {
        let result = await runForNode([CLEAN_SOURCES, BROKEN_SOURCES]);

        expect(result.series).toEqual([
            {
                runtime: "node",
                variant: "",
                outcome: { status: "failed", failures: [{ trial: 1, message: PARSE_DIAGNOSTIC }] },
            },
        ]);
        expect(result.notes).toEqual([]);
    });
});
