import { spawn } from "node:child_process";
import { once } from "node:events";
import { chmod, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { createInterface } from "node:readline";
import { afterAll, beforeAll, describe, expect, test } from "vitest";

import { baseEnvironment } from "./launch-commands.ts";
import { LaunchError, runToCompletion, spawnManaged } from "./processes.ts";

const INTERRUPTED_RUN = path.join(import.meta.dirname, "test-support/interrupted-run.ts");

let directory: string;

beforeAll(async () => {
    directory = await mkdtemp(path.join(tmpdir(), "bento-bench-processes-"));
});

afterAll(async () => {
    await rm(directory, { recursive: true, force: true });
});

function isAlive(pid: number): boolean {
    try {
        process.kill(pid, 0);
        return true;
    } catch {
        return false;
    }
}

describe("a command that cannot be launched", () => {
    async function launchFailure(file: string, contents: string | undefined, mode: number) {
        let executable = path.join(directory, file);
        if (contents !== undefined) {
            await writeFile(executable, contents);
            await chmod(executable, mode);
        }
        let error = await spawnManaged(
            { command: executable, args: [] },
            { cwd: directory, env: baseEnvironment() },
        ).catch((caught: unknown) => caught);
        return { executable, error };
    }

    test("a missing file is reported with its path and ENOENT", async () => {
        let { executable, error } = await launchFailure("missing", undefined, 0o755);

        expect(error).toBeInstanceOf(LaunchError);
        expect(error).toMatchObject({ message: expect.stringContaining(executable) });
        expect(error).toMatchObject({ message: expect.stringContaining("ENOENT") });
    });

    test("a file without execute permission is reported as EACCES", async () => {
        let { error } = await launchFailure("not-executable", "#!/bin/sh\n", 0o644);

        expect(error).toBeInstanceOf(LaunchError);
        expect(error).toMatchObject({ message: expect.stringContaining("EACCES") });
    });

    test("a file that is not a program is reported as ENOEXEC", async () => {
        let { executable, error } = await launchFailure("not-a-program", "not a program\n", 0o755);

        expect(error).toBeInstanceOf(LaunchError);
        expect(error).toMatchObject({ message: expect.stringContaining(executable) });
        expect(error).toMatchObject({ message: expect.stringContaining("ENOEXEC") });
    });
});

describe("a command with a deadline", () => {
    test("is stopped with its whole process group and reported with its output", async () => {
        let pidFile = path.join(directory, "hung.pid");
        let script = `echo partial output; echo $$ > ${pidFile}; exec sleep 60`;

        // The deadline is the behavior under test, so it needs the real clock.
        let failure = await runToCompletion(
            { command: "/bin/sh", args: ["-c", script] },
            { cwd: directory, env: baseEnvironment(), label: "hung probe", timeoutMs: 300 },
        ).catch((caught: unknown) => caught);

        expect(failure).toMatchObject({
            message: expect.stringContaining("hung probe timed out after 300 ms"),
        });
        expect(failure).toMatchObject({ message: expect.stringContaining("partial output") });
        expect(isAlive(Number(await readFile(pidFile, "utf8")))).toBe(false);
    });

    test("that finishes in time returns its output", async () => {
        let output = await runToCompletion(
            { command: "/bin/sh", args: ["-c", "echo done"] },
            { cwd: directory, env: baseEnvironment(), label: "quick command", timeoutMs: 5_000 },
        );

        expect(output).toBe("done\n");
    });
});

describe("interrupting a run", () => {
    test.each([
        ["SIGINT", 130],
        ["SIGTERM", 143],
    ] as const)(
        "%s stops the processes, runs the cleanup, and starts nothing new",
        async (signal, exitCode) => {
            let markers = await mkdtemp(path.join(directory, "interrupted-"));
            let run = spawn(process.execPath, [INTERRUPTED_RUN, markers], {
                stdio: ["ignore", "pipe", "pipe"],
            });
            let stderr = "";
            run.stderr.on("data", (chunk: Buffer) => (stderr += chunk.toString("utf8")));

            let [startedLine] = await once(createInterface({ input: run.stdout }), "line");
            let childPid = Number(String(startedLine).replace("started ", ""));
            expect(isAlive(childPid)).toBe(true);

            run.kill(signal);
            let [code] = await once(run, "close");

            expect(code).toBe(exitCode);
            expect(isAlive(childPid)).toBe(false);
            expect(await readFile(path.join(markers, "cleaned-up"), "utf8")).toBe("yes");
            expect(await readFile(path.join(markers, "trials"), "utf8")).toBe("0\n");
            expect(stderr).toContain(`interrupted by ${signal}`);
        },
    );
});
