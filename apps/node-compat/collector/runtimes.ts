import { createHash } from "node:crypto";
import { constants, createReadStream } from "node:fs";
import { access, realpath, stat } from "node:fs/promises";
import path from "node:path";

import type { RunReport, RuntimeIdentity } from "../lib/model.ts";

import { SetupError, describeError } from "./errors.ts";
import { runProcess } from "./process.ts";
import { NODE_TAG, NODE_VERSION } from "./upstream.ts";

const VERSION_PROBE_TIMEOUT_MS = 30_000;
const BENTO_VERSION = /^bento (\S+)$/;
const NODE_OVERRIDES = ["NODE_OPTIONS", "NODE_PATH", "NODE_REPL_EXTERNAL_MODULE"];

export interface LaunchEnvironment {
    cwd: string;
    environment: NodeJS.ProcessEnv;
}

export function cleanEnvironment(base: NodeJS.ProcessEnv): NodeJS.ProcessEnv {
    let environment = { ...base };
    for (let name of NODE_OVERRIDES) delete environment[name];
    return environment;
}

export async function nodeIdentity(
    executable: string,
    launch: LaunchEnvironment,
): Promise<RuntimeIdentity> {
    let resolved = await resolveExecutable(executable, launch);
    let reported = await probeVersion(resolved, launch);
    if (reported !== NODE_TAG) {
        throw new SetupError(
            `The Node control must be ${NODE_TAG}, but ${resolved} reports ${JSON.stringify(reported)}. Run the collector through Mise.`,
        );
    }
    return {
        version: NODE_VERSION,
        executable: resolved,
        sha256: await sha256File(resolved),
        arguments: [],
    };
}

export async function bentoIdentity(
    command: string,
    bentoArguments: string[],
    launch: LaunchEnvironment,
): Promise<RuntimeIdentity> {
    let resolved = await resolveExecutable(command, launch);
    let reported = await probeVersion(resolved, launch);
    let version = BENTO_VERSION.exec(reported)?.[1];
    if (version === undefined) {
        throw new SetupError(
            `${resolved} --version printed ${JSON.stringify(reported)}, not "bento <version>"`,
        );
    }
    return {
        version,
        executable: resolved,
        sha256: await sha256File(resolved),
        arguments: ["run", ...bentoArguments],
    };
}

export async function changedExecutable(
    label: string,
    identity: RuntimeIdentity,
): Promise<string | null> {
    let current: string;
    try {
        current = await sha256File(identity.executable);
    } catch (error) {
        return `The ${label} executable ${identity.executable} became unreadable during collection: ${describeError(error)}`;
    }
    if (current === identity.sha256) return null;
    return `The ${label} executable ${identity.executable} changed during collection (sha256 ${identity.sha256} became ${current}); results may mix builds`;
}

export async function changedRuntimes(runtimes: RunReport["runtimes"]): Promise<string | null> {
    let changes = await Promise.all([
        changedExecutable("Node", runtimes.node),
        changedExecutable("Bento", runtimes.bento),
    ]);
    let messages = changes.filter(change => change !== null);
    return messages.length === 0 ? null : messages.join("; ");
}

async function resolveExecutable(command: string, launch: LaunchEnvironment): Promise<string> {
    let candidates = command.includes("/")
        ? [path.resolve(launch.cwd, command)]
        : (launch.environment.PATH ?? "")
              .split(path.delimiter)
              .filter(Boolean)
              .map(directory => path.join(directory, command));
    let failures: string[] = [];
    for (let candidate of candidates) {
        try {
            let resolved = await realpath(candidate);
            if (!(await stat(resolved)).isFile()) throw new Error(`${resolved} is not a file`);
            await access(resolved, constants.X_OK);
            return resolved;
        } catch (error) {
            failures.push(describeError(error));
        }
    }
    let searched = command.includes("/") ? "" : " on PATH";
    throw new SetupError(
        `No executable file found for ${JSON.stringify(command)}${searched}: ${failures.at(-1) ?? "PATH is empty"}`,
    );
}

async function probeVersion(executable: string, launch: LaunchEnvironment): Promise<string> {
    let result = await runProcess({
        command: [executable, "--version"],
        cwd: launch.cwd,
        env: launch.environment,
        timeoutMs: VERSION_PROBE_TIMEOUT_MS,
    });
    if (result.exitCode !== 0) {
        let outcome = result.timedOut
            ? "timed out"
            : (result.spawnError ?? `exited with ${result.exitCode ?? result.signal}`);
        throw new SetupError(
            `${executable} --version ${outcome}: ${result.stderr.trim() || result.stdout.trim()}`,
        );
    }
    return result.stdout.trim();
}

async function sha256File(file: string): Promise<string> {
    let hash = createHash("sha256");
    for await (let chunk of createReadStream(file)) hash.update(chunk);
    return hash.digest("hex");
}
