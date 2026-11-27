import type { TestContext } from "remix/test";

import { mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import * as assert from "remix/assert";
import { it } from "remix/test";

import { BoundedLog } from "./logs.ts";
import { runProcess } from "./process.ts";

async function scratch(t: TestContext): Promise<string> {
    let directory = await mkdtemp(path.join(tmpdir(), "bento-compat-process-"));
    t.after(() => rm(directory, { recursive: true, force: true }));
    return directory;
}

function spawnsGrandchild(pidFile: string, leaderBody: string): string {
    return [
        "const { spawn } = require('node:child_process');",
        "const fs = require('node:fs');",
        "const child = spawn(process.execPath, ['-e', 'setInterval(() => {}, 1000)'], { stdio: 'inherit' });",
        `fs.writeFileSync(${JSON.stringify(pidFile)}, String(child.pid));`,
        leaderBody,
    ].join("\n");
}

async function assertGone(t: TestContext, pidFile: string): Promise<void> {
    let pid = Number(await readFile(pidFile, "utf8"));
    assert.ok(pid > 0);
    await t.waitFor(() => assert.throws(() => process.kill(pid, 0), { code: "ESRCH" }), {
        timeout: 5000,
    });
}

it("keeps short logs intact", () => {
    let log = new BoundedLog(4, 6);
    log.write(Buffer.from("abcdefghij"));
    assert.equal(log.truncated, false);
    assert.equal(log.text(), "abcdefghij");
});

it("bounds long logs to their beginning and end with an explicit marker", () => {
    let log = new BoundedLog(4, 6);
    log.write(Buffer.from("abcdefgh"));
    log.write(Buffer.from("ijklmnop"));
    assert.equal(log.truncated, true);
    assert.equal(log.text(), "abcd\n[… 6 bytes truncated by the collector …]\nklmnop");
});

// The deadline is enforced by killing real OS processes, so fake timers cannot stand in for it.
it(
    "kills the whole process group when a process exceeds its deadline",
    { timeout: 20_000 },
    async t => {
        let directory = await scratch(t);
        let pidFile = path.join(directory, "grandchild.pid");
        let result = await runProcess({
            command: [
                process.execPath,
                "-e",
                spawnsGrandchild(pidFile, "setInterval(() => {}, 1000);"),
            ],
            cwd: directory,
            env: process.env,
            timeoutMs: 2000,
        });
        assert.equal(result.timedOut, true);
        assert.equal(result.exitCode, null);
        assert.equal(result.signal, "SIGKILL");
        await assertGone(t, pidFile);
    },
);

it(
    "does not wait on a descendant that outlives its exited leader",
    { timeout: 20_000 },
    async t => {
        let directory = await scratch(t);
        let pidFile = path.join(directory, "grandchild.pid");
        let result = await runProcess({
            command: [process.execPath, "-e", spawnsGrandchild(pidFile, "process.exit(0);")],
            cwd: directory,
            env: process.env,
            timeoutMs: 60_000,
        });
        assert.equal(result.timedOut, false);
        assert.equal(result.exitCode, 0);
        await assertGone(t, pidFile);
    },
);

it(
    "stops a running process group promptly when collection is interrupted",
    { timeout: 20_000 },
    async t => {
        let directory = await scratch(t);
        let readyFile = path.join(directory, "ready");
        let pidFile = path.join(directory, "grandchild.pid");
        let controller = new AbortController();
        let running = runProcess({
            command: [
                process.execPath,
                "-e",
                spawnsGrandchild(
                    pidFile,
                    `fs.writeFileSync(${JSON.stringify(readyFile)}, ''); setInterval(() => {}, 1000);`,
                ),
            ],
            cwd: directory,
            env: process.env,
            timeoutMs: 60_000,
            signal: controller.signal,
        });
        await t.waitFor(() => readFile(readyFile), { timeout: 10_000 });
        controller.abort();
        let result = await running;
        assert.equal(result.interrupted, true);
        assert.equal(result.timedOut, false);
        await assertGone(t, pidFile);
    },
);

it("reports an executable that cannot start instead of throwing", { timeout: 10_000 }, async t => {
    let directory = await scratch(t);
    let missing = path.join(directory, "missing-runtime");
    let result = await runProcess({
        command: [missing, "x.js"],
        cwd: directory,
        env: process.env,
        timeoutMs: 5000,
    });
    assert.equal(result.exitCode, null);
    assert.match(result.spawnError ?? "", /ENOENT/);
});
