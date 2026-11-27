import { spawn } from "node:child_process";

import { errorCode } from "./errors.ts";
import { BoundedLog } from "./logs.ts";

const LOG_HEAD_BYTES = 4 * 1024;
const LOG_TAIL_BYTES = 12 * 1024;
const OUTPUT_DRAIN_MS = 2000;

export interface ProcessRequest {
    command: string[];
    cwd: string;
    env: NodeJS.ProcessEnv;
    timeoutMs: number;
    signal?: AbortSignal;
    stdoutHeadBytes?: number;
}

export interface ProcessResult {
    command: string[];
    exitCode: number | null;
    signal: string | null;
    timedOut: boolean;
    interrupted: boolean;
    spawnError: string | null;
    durationMs: number;
    stdout: string;
    stderr: string;
    truncated: boolean;
}

export function runProcess(request: ProcessRequest): Promise<ProcessResult> {
    let { command, signal } = request;
    let [executable, ...args] = command;
    if (executable === undefined) throw new TypeError("runProcess needs a command");
    let result: ProcessResult = {
        command,
        exitCode: null,
        signal: null,
        timedOut: false,
        interrupted: false,
        spawnError: null,
        durationMs: 0,
        stdout: "",
        stderr: "",
        truncated: false,
    };
    if (signal?.aborted) return Promise.resolve({ ...result, interrupted: true });

    let { promise, resolve } = Promise.withResolvers<ProcessResult>();
    let stdout = new BoundedLog(request.stdoutHeadBytes ?? LOG_HEAD_BYTES, LOG_TAIL_BYTES);
    let stderr = new BoundedLog(LOG_HEAD_BYTES, LOG_TAIL_BYTES);
    let started = performance.now();
    let child = spawn(executable, args, {
        cwd: request.cwd,
        env: request.env,
        detached: true,
        stdio: ["ignore", "pipe", "pipe"],
    });
    let drain: NodeJS.Timeout | undefined;
    let killGroup = () => {
        if (child.pid !== undefined) killProcessGroup(child.pid);
    };
    let deadline = setTimeout(() => {
        result.timedOut = true;
        killGroup();
    }, request.timeoutMs);
    let onAbort = () => {
        result.interrupted = true;
        killGroup();
    };
    signal?.addEventListener("abort", onAbort, { once: true });

    child.stdout.on("data", (chunk: Buffer) => stdout.write(chunk));
    child.stderr.on("data", (chunk: Buffer) => stderr.write(chunk));
    child.on("error", error => {
        result.spawnError = error.message;
    });
    child.on("exit", (code, exitSignal) => {
        result.durationMs = Math.round(performance.now() - started);
        result.exitCode = code;
        result.signal = exitSignal;
        clearTimeout(deadline);
        killGroup();
        drain = setTimeout(() => {
            child.stdout.destroy();
            child.stderr.destroy();
        }, OUTPUT_DRAIN_MS);
    });
    child.on("close", () => {
        clearTimeout(deadline);
        clearTimeout(drain);
        signal?.removeEventListener("abort", onAbort);
        if (result.spawnError !== null) {
            result.exitCode = null;
            result.durationMs = Math.round(performance.now() - started);
        }
        result.stdout = stdout.text();
        result.stderr = stderr.text();
        result.truncated = stdout.truncated || stderr.truncated;
        resolve(result);
    });
    return promise;
}

function killProcessGroup(leader: number): void {
    try {
        process.kill(-leader, "SIGKILL");
    } catch (error) {
        // Darwin reports EPERM for a group whose remaining members are all unreaped zombies.
        let code = errorCode(error);
        if (code !== "ESRCH" && code !== "EPERM") throw error;
    }
}
