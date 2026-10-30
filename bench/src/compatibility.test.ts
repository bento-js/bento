import { chmod, mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { afterAll, beforeAll, expect, test } from "vitest";

import { checkCompatibility } from "./compatibility.ts";

let directory: string;

beforeAll(async () => {
    directory = await mkdtemp(path.join(tmpdir(), "bento-bench-compatibility-"));
    await mkdir(path.join(directory, "db"));
    await mkdir(path.join(directory, ".bench/seed"), { recursive: true });
});

afterAll(async () => {
    await rm(directory, { recursive: true, force: true });
});

test("a runtime that cannot be launched is incompatible at startup, with the real error", async () => {
    let executable = path.join(directory, "bento");
    await writeFile(executable, "not a program\n");
    await chmod(executable, 0o755);

    let verdict = await checkCompatibility({
        runtime: "bento",
        executable,
        fixture: { directory, databaseUrl: "db/bench.db", remixVersion: "3.0.0" },
        mode: "production",
    });

    // The real error is the platform's. macOS reports ENOEXEC for a file that is not a
    // program; glibc retries an ENOEXEC exec through /bin/sh, so on Linux the file starts
    // as a shell script and fails with the shell's exit code instead. Either way the
    // verdict is incompatible at startup and carries the reason the runtime gave.
    let reason = process.platform === "darwin" ? "ENOEXEC" : "exit code 127";

    expect(verdict).toEqual({
        compatible: false,
        check: "startup",
        failures: [expect.stringContaining(reason)],
        evidence: [],
    });
});
