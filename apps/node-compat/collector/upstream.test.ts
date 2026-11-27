import type { TestContext } from "remix/test";

import { execFileSync } from "node:child_process";
import { access, mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import * as assert from "remix/assert";
import { it } from "remix/test";

import { SetupError } from "./errors.ts";
import { createScratch, SCRATCH_MANIFEST } from "./scratch.ts";
import { verifyCheckout } from "./upstream.ts";

function git(cwd: string, ...args: string[]): string {
    let environment = { ...process.env };
    for (let name of Object.keys(environment)) {
        if (name.startsWith("GIT_")) delete environment[name];
    }
    let isolated = [
        "-c",
        "core.hooksPath=/dev/null",
        "-c",
        "commit.gpgsign=false",
        "-c",
        "user.name=Collector Test",
        "-c",
        "user.email=collector@example.invalid",
    ];
    return execFileSync("git", [...isolated, ...args], {
        cwd,
        encoding: "utf8",
        env: { ...environment, GIT_CONFIG_GLOBAL: "/dev/null" },
    }).trim();
}

async function checkout(t: TestContext) {
    let directory = await mkdtemp(path.join(tmpdir(), "bento-compat-upstream-"));
    t.after(() => rm(directory, { recursive: true, force: true }));
    for (let file of ["test/parallel/test-a.js", "test/sequential/test-b.js", "doc/outside.md"]) {
        await mkdir(path.dirname(path.join(directory, file)), { recursive: true });
        await writeFile(path.join(directory, file), `// ${file}\n`);
    }
    git(directory, "init", "--quiet");
    git(directory, "add", ".");
    git(directory, "commit", "--quiet", "-m", "pin");
    git(directory, "sparse-checkout", "set", "--cone", "test");
    return { directory, commit: git(directory, "rev-parse", "HEAD") };
}

it("accepts only the pinned, unmodified checkout", { timeout: 30_000 }, async t => {
    let { directory, commit } = await checkout(t);
    await verifyCheckout(directory, commit, ["test"]);
    await assert.rejects(verifyCheckout(directory, "0".repeat(40), ["test"]), SetupError);
    await assert.rejects(verifyCheckout(directory, commit, ["test", "lib"]), SetupError);
    await writeFile(path.join(directory, "test/parallel/test-new.js"), "");
    await assert.rejects(verifyCheckout(directory, commit, ["test"]), SetupError);
    await rm(path.join(directory, "test/parallel/test-new.js"));
    await writeFile(path.join(directory, "test/parallel/test-a.js"), "patched\n");
    await assert.rejects(verifyCheckout(directory, commit, ["test"]), SetupError);
});

it("verifies the intended checkout when invoked inside another repository's hook", async t => {
    let { directory, commit } = await checkout(t);
    let source = `import { verifyCheckout } from ${JSON.stringify(new URL("./upstream.ts", import.meta.url).href)};
await verifyCheckout(${JSON.stringify(directory)}, ${JSON.stringify(commit)}, ["test"]);`;
    execFileSync(process.execPath, ["--input-type=module", "--eval", source], {
        env: {
            ...process.env,
            GIT_DIR: path.join(directory, "not-a-repository"),
            GIT_WORK_TREE: path.join(directory, "not-a-worktree"),
            GIT_INDEX_FILE: path.join(directory, "not-an-index"),
        },
    });
});

it(
    "runs tests from separate scratch copies that leave the cached checkout untouched",
    { timeout: 30_000 },
    async t => {
        let { directory, commit } = await checkout(t);
        let scratch = await createScratch(directory);
        t.after(() => scratch.remove());
        for (let workspace of [scratch.node, scratch.bento]) {
            assert.equal(
                await readFile(path.join(workspace.root, "test/parallel/test-a.js"), "utf8"),
                "// test/parallel/test-a.js\n",
            );
            assert.equal(
                await readFile(path.join(workspace.root, "test/parallel/package.json"), "utf8"),
                SCRATCH_MANIFEST,
            );
            assert.equal(
                await readFile(path.join(workspace.root, "test/sequential/package.json"), "utf8"),
                SCRATCH_MANIFEST,
            );
            await assert.rejects(access(path.join(workspace.root, ".git")));
            await access(workspace.testDirectory);
        }
        assert.notEqual(scratch.node.root, scratch.bento.root);
        await verifyCheckout(directory, commit, ["test"]);
    },
);
