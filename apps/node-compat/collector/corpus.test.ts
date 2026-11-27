import { mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import * as assert from "remix/assert";
import { it } from "remix/test";

import { listCorpus, moduleOf, parseMetadata, selectCases } from "./corpus.ts";
import { SetupError } from "./errors.ts";

it("keeps every // Flags: argument in order, as tools/test.py reads it", () => {
    let source =
        "'use strict';\n// Flags: --expose-internals  --no-warnings --stack-size=50\nrequire('../common');\n";
    assert.deepEqual(parseMetadata(source, "test-a.js"), {
        flags: ["--expose-internals", "--no-warnings", "--stack-size=50"],
        env: {},
    });
});

it("reads // Env: assignments and keeps '=' inside values", () => {
    let metadata = parseMetadata("// Env: A_VAR=1 B_VAR=x=y\n", "test-a.js");
    assert.deepEqual(metadata, { flags: [], env: { A_VAR: "1", B_VAR: "x=y" } });
    assert.throws(() => parseMetadata("// Env: BROKEN\n", "test-b.js"), SetupError);
});

it("lists only top-level test-* JavaScript files in test/parallel and test/sequential", async t => {
    let root = await mkdtemp(path.join(tmpdir(), "bento-compat-corpus-"));
    t.after(() => rm(root, { recursive: true, force: true }));
    let files = [
        "test/parallel/test-buffer-alloc.js",
        "test/parallel/test-esm.mjs",
        "test/parallel/test-cjs.cjs",
        "test/parallel/helper.js",
        "test/parallel/test-types.ts",
        "test/parallel/parallel.status",
        "test/parallel/nested/test-nested.js",
        "test/sequential/test-child-process-exec.js",
        "test/pummel/test-pummel.js",
    ];
    for (let file of files) {
        await mkdir(path.dirname(path.join(root, file)), { recursive: true });
        await writeFile(
            path.join(root, file),
            file.endsWith("alloc.js") ? "// Flags: --expose-gc\n" : "",
        );
    }
    let cases = await listCorpus(root);
    assert.deepEqual(
        cases.map(testCase => [testCase.id, testCase.path, testCase.module]),
        [
            ["parallel/test-buffer-alloc.js", "test/parallel/test-buffer-alloc.js", "buffer"],
            ["parallel/test-cjs.cjs", "test/parallel/test-cjs.cjs", "cjs"],
            ["parallel/test-esm.mjs", "test/parallel/test-esm.mjs", "esm"],
            [
                "sequential/test-child-process-exec.js",
                "test/sequential/test-child-process-exec.js",
                "child_process",
            ],
        ],
    );
    assert.deepEqual(cases[0]?.flags, ["--expose-gc"]);
});

it("groups test files by the longest built-in module name they start with", () => {
    assert.equal(moduleOf("test-child-process-spawn.js"), "child_process");
    assert.equal(moduleOf("test-http2-session.js"), "http2");
    assert.equal(moduleOf("test-https-agent.js"), "https");
    assert.equal(moduleOf("test-http.js"), "http");
    assert.equal(moduleOf("test-fs-promises-file.mjs"), "fs");
    assert.equal(moduleOf("test-string-decoder.js"), "string_decoder");
    assert.equal(moduleOf("test-whatwg-url-parsing.js"), "whatwg");
});

it("rejects a filter that selects nothing instead of recording an empty run", () => {
    let cases = ["parallel/test-buffer.js", "sequential/test-fs.js"].map(id => ({
        id,
        path: `test/${id}`,
        module: "buffer",
        flags: [],
        env: {},
    }));
    assert.equal(selectCases(cases, null).length, 2);
    assert.deepEqual(
        selectCases(cases, "sequential/").map(testCase => testCase.id),
        ["sequential/test-fs.js"],
    );
    assert.throws(() => selectCases(cases, "test-zlib"), SetupError);
});
