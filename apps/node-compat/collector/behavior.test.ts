import type { TestContext } from "remix/test";

import { access, chmod, mkdir, mkdtemp, readFile, realpath, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import * as assert from "remix/assert";
import { it } from "remix/test";

import type { BehaviorContext } from "./behavior.ts";
import type { CorpusCase } from "./corpus.ts";

import { runCase } from "./behavior.ts";

async function fixture(t: TestContext, file: string, source: string) {
    let directory = await mkdtemp(path.join(tmpdir(), "bento-compat-behavior-"));
    t.after(() => rm(directory, { recursive: true, force: true }));
    let workspaces = {
        node: await workspace(directory, "node"),
        bento: await workspace(directory, "bento"),
    };
    for (let { root } of Object.values(workspaces)) {
        await writeFile(path.join(root, "test", "parallel", file), source);
    }
    let invocations = path.join(directory, "bento-invocations.log");
    let bento = path.join(directory, "fake-bento");
    await writeFile(
        bento,
        `#!/bin/sh\necho "$@" >> ${JSON.stringify(invocations)}\nshift\nBENTO_FAKE=1 exec ${JSON.stringify(process.execPath)} "$@"\n`,
    );
    await chmod(bento, 0o755);
    let controller = new AbortController();
    let context: BehaviorContext = {
        node: { version: "26.12.1", executable: process.execPath, sha256: "node", arguments: [] },
        bento: { version: "test", executable: bento, sha256: "bento", arguments: ["run"] },
        workspaces,
        environment: { ...process.env, NODE_OPTIONS: "--no-such-node-option" },
        timeoutMs: 20_000,
        signal: controller.signal,
    };
    return { context, controller, invocations, workspaces };
}

async function workspace(directory: string, name: string) {
    let root = path.join(directory, name);
    let testDirectory = path.join(directory, `${name}-tmp`);
    await mkdir(path.join(root, "test", "parallel"), { recursive: true });
    await mkdir(testDirectory);
    return { root, testDirectory };
}

function testCase(
    file: string,
    flags: string[] = [],
    env: Record<string, string> = {},
): CorpusCase {
    return { id: `parallel/${file}`, path: `test/parallel/${file}`, module: "gc", flags, env };
}

it(
    "gives Node its flags and marks Bento unsupported without running it",
    { timeout: 30_000 },
    async t => {
        let source =
            "// Flags: --expose-gc\nif (typeof globalThis.gc !== 'function') process.exit(1);\n";
        let { context, invocations } = await fixture(t, "test-gc.js", source);
        let result = await runCase(testCase("test-gc.js", ["--expose-gc"]), context);
        assert.ok(result);
        assert.equal(result.node.status, "passed");
        assert.deepEqual(result.node.command, [
            process.execPath,
            "--expose-gc",
            "test/parallel/test-gc.js",
        ]);
        assert.deepEqual(result.flags, ["--expose-gc"]);
        assert.equal(result.bento.status, "unsupported");
        assert.deepEqual(result.bento.command, []);
        assert.match(result.bento.reason ?? "", /--expose-gc/);
        await assert.rejects(access(invocations));
    },
);

it(
    "runs each runtime in its own scratch copy with the upstream harness environment",
    { timeout: 30_000 },
    async t => {
        let source = [
            "if (process.env.COMPAT_MARK !== 'yes') process.exit(2);",
            "if (process.env.NODE_SKIP_FLAG_CHECK !== 'true') process.exit(3);",
            "if (process.env.NODE_OPTIONS !== undefined) process.exit(4);",
            "if (process.env.NODE_TEST_KNOWN_GLOBALS !== '0') process.exit(5);",
            "console.log(JSON.stringify([process.cwd(), process.env.NODE_TEST_DIR]));",
        ].join("\n");
        let { context, invocations, workspaces } = await fixture(t, "test-env.js", source);
        let result = await runCase(testCase("test-env.js", [], { COMPAT_MARK: "yes" }), context);
        assert.ok(result);
        assert.equal(result.node.status, "passed", result.node.stderr);
        assert.equal(result.bento.status, "passed", result.bento.stderr);
        assert.deepEqual(JSON.parse(result.node.stdout), [
            await realpath(workspaces.node.root),
            workspaces.node.testDirectory,
        ]);
        assert.deepEqual(JSON.parse(result.bento.stdout), [
            await realpath(workspaces.bento.root),
            workspaces.bento.testDirectory,
        ]);
        assert.deepEqual(result.bento.command, [
            context.bento.executable,
            "run",
            "test/parallel/test-env.js",
        ]);
        assert.equal((await readFile(invocations, "utf8")).trim(), "run test/parallel/test-env.js");
    },
);

it(
    "records a Node skip and a Bento failure as their own outcomes",
    { timeout: 30_000 },
    async t => {
        let source =
            "if (process.env.BENTO_FAKE) throw new Error('bento-only failure');\nconsole.log('1..0 # Skipped: no ipv6');\n";
        let { context } = await fixture(t, "test-skip.js", source);
        let result = await runCase(testCase("test-skip.js"), context);
        assert.ok(result);
        assert.deepEqual([result.node.status, result.node.reason], ["skipped", "no ipv6"]);
        assert.equal(result.bento.status, "failed");
        assert.match(result.bento.stderr, /bento-only failure/);
    },
);

it("discards a case interrupted before both runtimes finish", { timeout: 30_000 }, async t => {
    let { context, controller, workspaces } = await fixture(
        t,
        "test-hang.js",
        "require('fs').writeFileSync(require('path').join(process.env.NODE_TEST_DIR, 'ready'), '');\nsetInterval(() => {}, 1000);\n",
    );
    let running = runCase(testCase("test-hang.js"), context);
    await t.waitFor(() => readFile(path.join(workspaces.node.testDirectory, "ready")), {
        timeout: 10_000,
    });
    controller.abort();
    assert.equal(await running, null);
});
