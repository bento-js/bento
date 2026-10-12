import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import path from "node:path";

import type { RuntimeName } from "./results-model.ts";

import { capture, findOnPath, isExecutableFile } from "./executables.ts";

export type RuntimeAvailability =
    | { available: true; executable: string; version: string }
    | { available: false; reason: string };

export async function detectRuntimes(
    environment: NodeJS.ProcessEnv,
): Promise<Record<RuntimeName, RuntimeAvailability>> {
    return {
        node: { available: true, executable: process.execPath, version: process.version },
        bun: await detectBun(environment),
        bento: await detectBento(environment),
    };
}

async function detectBun(environment: NodeJS.ProcessEnv): Promise<RuntimeAvailability> {
    let executable = await findOnPath("bun", environment);
    if (executable === undefined) return { available: false, reason: "bun was not found on PATH" };
    return { available: true, executable, version: await capture(executable, ["--version"]) };
}

/** Bento is the executable `BENTO_BIN` names; it is never searched for or substituted. */
export async function detectBento(environment: NodeJS.ProcessEnv): Promise<RuntimeAvailability> {
    let configured = environment.BENTO_BIN;
    if (!configured) {
        return {
            available: false,
            reason: "BENTO_BIN is not set; build the bento executable and point BENTO_BIN at it",
        };
    }
    let executable = path.resolve(configured);
    if (!(await isExecutableFile(executable))) {
        return { available: false, reason: `BENTO_BIN=${configured} is not an executable file` };
    }
    let digest = createHash("sha256")
        .update(await readFile(executable))
        .digest("hex");
    return { available: true, executable, version: `sha256:${digest.slice(0, 12)}` };
}
