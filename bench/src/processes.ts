import { spawn } from "node:child_process";
import { setTimeout as sleep } from "node:timers/promises";

import type { Command } from "./launch-commands.ts";

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

// Every process group the harness has started and not yet confirmed gone.
let liveGroups = new Set<number>();

export function spawnManaged(command: Command, options: SpawnOptions): ManagedProcess {
    let child = spawn(command.command, command.args, {
        cwd: options.cwd,
        env: options.env,
        detached: true,
        stdio: ["ignore", "pipe", "pipe"],
    });
    let startError: Error | undefined;
    child.on("error", error => (startError = error));
    if (child.pid === undefined) {
        let reason = startError?.message ?? "unknown error";
        throw new Error(`Could not start ${command.command}: ${reason}`);
    }
    let pid = child.pid;
    liveGroups.add(pid);

    let tail = "";
    let stdout = "";
    child.stdout.on("data", (chunk: Buffer) => {
        let text = chunk.toString("utf8");
        tail = (tail + text).slice(-OUTPUT_TAIL_CHARACTERS);
        if (stdout.length < STDOUT_CAP_CHARACTERS) stdout += text;
    });
    child.stderr.on("data", (chunk: Buffer) => {
        tail = (tail + chunk.toString("utf8")).slice(-OUTPUT_TAIL_CHARACTERS);
    });

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

/** Runs a command to completion and returns its stdout; a non-zero exit throws with its output. */
export async function runToCompletion(
    command: Command,
    options: SpawnOptions & { label: string },
): Promise<string> {
    let child = spawnManaged(command, options);
    let status = await child.closed;
    await child.stop();
    if (status.code !== 0) {
        throw new Error(`${options.label} failed (${describeExit(status)}):\n${child.output()}`);
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
    for (let [signal, code] of [
        ["SIGINT", 130],
        ["SIGTERM", 143],
    ] as const) {
        process.on(signal, () => {
            killAllNow();
            process.exit(code);
        });
    }
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
