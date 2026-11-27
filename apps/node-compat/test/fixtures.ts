import type { BehaviorResult, Execution, Outcome, RunReport } from "../lib/model.ts";

export function execution(status: Outcome = "passed"): Execution {
    return {
        status,
        command: ["node", "test.js"],
        exitCode: status === "passed" ? 0 : 1,
        signal: null,
        durationMs: 12,
        stdout: "",
        stderr: "",
        reason: null,
        truncated: false,
    };
}

export function behavior(id: string, bento: Outcome, node: Outcome = "passed"): BehaviorResult {
    return { id, module: "buffer", flags: [], node: execution(node), bento: execution(bento) };
}

export function report(overrides: Partial<RunReport> = {}): RunReport {
    return {
        schemaVersion: 1,
        id: "run-1",
        kind: "behavior",
        state: "complete",
        startedAt: "2026-11-27T12:00:00.000Z",
        finishedAt: "2026-11-27T12:01:00.000Z",
        suite: {
            version: "26.12.1",
            revision: "abc",
            source: "nodejs/node",
            scope: "parallel,sequential",
        },
        platform: "darwin",
        arch: "arm64",
        timeoutMs: 10000,
        runtimes: {
            node: {
                version: "26.12.1",
                executable: "/bin/node",
                sha256: "node-sha",
                arguments: [],
            },
            bento: {
                version: "0.0.0",
                executable: "/bin/bento",
                sha256: "bento-sha",
                arguments: [],
            },
        },
        selection: { filter: "buffer", total: 5000, selected: 1 },
        behavior: [behavior("parallel/test-buffer.js", "passed")],
        inventory: [],
        error: null,
        ...overrides,
    };
}
