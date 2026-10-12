import type { ChildProcessByStdio } from "node:child_process";
import type { Readable } from "node:stream";

import { spawn } from "node:child_process";
import { once } from "node:events";
import { setTimeout as sleep } from "node:timers/promises";

import type { Command } from "./launch-commands.ts";

import { handleInterruption, throwIfInterrupted } from "./interruption.ts";
import { lookup } from "./json-values.ts";

export interface ExitStatus {
    code: number | null;
    signal: NodeJS.Signals | null;
}

export interface ManagedProcess {
    pid: number;
    /** Resolves when the leader process exits. */
    exited: Promise<ExitStatus>;
    /** The exit status once the leader has exited, otherwise undefined. */
    exitStatus(): ExitStatus | undefined;
    /** Resolves when the leader has exited and its output streams have closed. */
    closed: Promise<ExitStatus>;
    /** The most recent output (stdout and stderr interleaved), for failure evidence. */
    output(): string;
    /** The beginning of stdout, up to a fixed cap. */
    stdout(): string;
    /** Stops the whole process group: SIGTERM, then SIGKILL after the grace period. */
    stop(): Promise<ExitStatus>;
}

export interface SpawnOptions {
    cwd: string;
    env: Record<string, string | undefined>;
}

const GRACE_MS = 5_000;
const OUTPUT_TAIL_CHARACTERS = 16_384;
const STDOUT_CAP_CHARACTERS = 4 * 1024 * 1024;

/** Long enough for a first `pnpm install` from the registry, short enough to report a hang. */
const COMMAND_TIMEOUT_MS = 10 * 60_000;

// Every process group the harness has started and not yet confirmed gone.
let liveGroups = new Set<number>();

/** The process could not be started at all, as opposed to starting and then failing. */
export class LaunchError extends Error {}

export async function spawnManaged(
    command: Command,
    options: SpawnOptions,
): Promise<ManagedProcess> {
    throwIfInterrupted();
    let { child, pid } = await launch(command, options);
    liveGroups.add(pid);

    let tail = "";
    let stdout = "";
    let remember = (text: string) => (tail = (tail + text).slice(-OUTPUT_TAIL_CHARACTERS));
    child.on("error", error => remember(`${error.message}\n`));
    child.stdout.on("data", (chunk: Buffer) => {
        let text = chunk.toString("utf8");
        remember(text);
        if (stdout.length < STDOUT_CAP_CHARACTERS) stdout += text;
    });
    child.stderr.on("data", (chunk: Buffer) => remember(chunk.toString("utf8")));

    let exitStatus: ExitStatus | undefined;
    let exited = new Promise<ExitStatus>(resolve => {
        child.once("exit", (code, signal) => {
            exitStatus = { code, signal };
            resolve(exitStatus);
        });
    });

    let closed = new Promise<ExitStatus>(resolve => {
        child.once("close", (code, signal) => resolve({ code, signal }));
    });

    async function stop(): Promise<ExitStatus> {
        signalGroup(pid, "SIGTERM");
        let graceful = await Promise.race([exited, sleep(GRACE_MS).then(() => undefined)]);
        if (graceful === undefined) signalGroup(pid, "SIGKILL");
        let status = graceful ?? (await exited);
        await removeStragglers(pid);
        liveGroups.delete(pid);
        return status;
    }

    return {
        pid,
        exited,
        exitStatus: () => exitStatus,
        closed,
        output: () => tail,
        stdout: () => stdout,
        stop,
    };
}

/**
 * Runs a command to completion and returns its stdout. A non-zero exit, or a run past
 * `timeoutMs`, stops the command's process group and throws with its output.
 */
export async function runToCompletion(
    command: Command,
    options: SpawnOptions & { label: string; timeoutMs?: number },
): Promise<string> {
    let timeoutMs = options.timeoutMs ?? COMMAND_TIMEOUT_MS;
    let child = await spawnManaged(command, options);
    let { promise: expired, resolve: expire } = Promise.withResolvers<"expired">();
    let timer = setTimeout(expire, timeoutMs, "expired");
    let outcome = await Promise.race([child.closed, expired]);
    clearTimeout(timer);
    await child.stop();
    if (outcome === "expired") {
        throw new Error(`${options.label} timed out after ${timeoutMs} ms:\n${child.output()}`);
    }
    if (outcome.code !== 0) {
        throw new Error(`${options.label} failed (${describeExit(outcome)}):\n${child.output()}`);
    }
    return child.stdout();
}

export function describeExit(status: ExitStatus): string {
    return status.signal ? `signal ${status.signal}` : `exit code ${status.code}`;
}

/** Synchronously kills every group still alive; for exit and signal handlers. */
export function killAllNow(): void {
    for (let pid of liveGroups) signalGroup(pid, "SIGKILL");
    liveGroups.clear();
}

export function installCleanupHandlers(): void {
    process.on("exit", killAllNow);
    handleInterruption(killAllNow);
}

async function launch(
    command: Command,
    options: SpawnOptions,
): Promise<{ child: ChildProcessByStdio<null, Readable, Readable>; pid: number }> {
    let child;
    try {
        child = spawn(command.command, command.args, {
            cwd: options.cwd,
            env: options.env,
            detached: true,
            stdio: ["ignore", "pipe", "pipe"],
        });
    } catch (error) {
        throw launchError(command, error);
    }
    if (child.pid !== undefined) return { child, pid: child.pid };
    let [error] = await once(child, "error");
    throw launchError(command, error);
}

function launchError(command: Command, cause: unknown): LaunchError {
    let reason = cause instanceof Error ? cause.message : String(cause);
    return new LaunchError(`Could not start ${command.command}: ${reason}`);
}

function signalGroup(pid: number, signal: NodeJS.Signals): void {
    try {
        process.kill(-pid, signal);
    } catch (error) {
        if (lookup(error, "code") !== "ESRCH") throw error;
    }
}

function groupAlive(pid: number): boolean {
    try {
        process.kill(-pid, 0);
        return true;
    } catch {
        return false;
    }
}

async function removeStragglers(pid: number): Promise<void> {
    if (!groupAlive(pid)) return;
    await sleep(250);
    if (groupAlive(pid)) signalGroup(pid, "SIGKILL");
}
