import { expect, test } from "vite-plus/test";

import type { RuntimeName, Series } from "../results-model.ts";

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
