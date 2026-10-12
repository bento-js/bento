import { chmod, mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { afterAll, beforeAll, expect, test } from "vite-plus/test";

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

    expect(verdict).toEqual({
        compatible: false,
        check: "startup",
        failures: [expect.stringContaining("ENOEXEC")],
        evidence: [],
    });
});
