import { stripVTControlCharacters } from "node:util";

import type { Execution, Outcome } from "../lib/model.ts";
import type { ProcessResult } from "./process.ts";

interface Classification {
    status: Outcome;
    reason: string | null;
}

const RUST_PANIC = /^thread '[^']*' panicked at /m;
const EMPTY_PLAN = /^1\.\.0\b(.*)$/m;
const SKIP_DIRECTIVE = /#\s*skip\S*\s*(.*)$/i;
const SUMMARY_LINE = /^(?:#|ℹ) (tests|pass|skipped) (\d+)\s*$/gm;

export function classify(result: ProcessResult, timeoutMs: number): Classification {
    if (result.spawnError !== null)
        return { status: "crashed", reason: `Could not start: ${result.spawnError}` };
    if (result.timedOut) {
        return {
            status: "timeout",
            reason: `Did not exit within ${timeoutMs} ms; its process group was killed`,
        };
    }
    if (result.signal !== null)
        return { status: "crashed", reason: `Terminated by ${result.signal}` };
    if (result.exitCode !== 0) {
        if (result.exitCode === 101 && RUST_PANIC.test(result.stderr)) {
            return { status: "crashed", reason: "The runtime panicked (exit code 101)" };
        }
        return { status: "failed", reason: `Exited with code ${result.exitCode}` };
    }
    return classifySuccessfulExit(stripVTControlCharacters(result.stdout));
}

function classifySuccessfulExit(stdout: string): Classification {
    let plan = EMPTY_PLAN.exec(stdout);
    if (plan !== null) {
        let directive = SKIP_DIRECTIVE.exec(plan[1] ?? "");
        let reason =
            directive?.[1]?.trim() ||
            (directive ? "Skipped without a reason" : "Empty test plan (1..0)");
        return { status: "skipped", reason };
    }
    let summary = new Map<string, number>();
    for (let [, key, value] of stdout.matchAll(SUMMARY_LINE)) summary.set(key!, Number(value));
    let tests = summary.get("tests");
    if (tests === 0) return { status: "skipped", reason: "node:test reported zero tests" };
    if (tests !== undefined && summary.get("skipped") === tests) {
        return { status: "skipped", reason: `node:test skipped all ${tests} tests` };
    }
    return { status: "passed", reason: null };
}

export function toExecution(result: ProcessResult, timeoutMs: number): Execution {
    let { status, reason } = classify(result, timeoutMs);
    return {
        status,
        command: result.command,
        exitCode: result.exitCode,
        signal: result.signal,
        durationMs: result.durationMs,
        stdout: result.stdout,
        stderr: result.stderr,
        reason,
        truncated: result.truncated,
    };
}

export function notExecuted(status: Outcome, reason: string): Execution {
    return {
        status,
        command: [],
        exitCode: null,
        signal: null,
        durationMs: 0,
        stdout: "",
        stderr: "",
        reason,
        truncated: false,
    };
}
