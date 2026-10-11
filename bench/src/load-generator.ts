import { lookup, readNumber } from "./json-values.ts";
import { baseEnvironment } from "./launch-commands.ts";
import { runToCompletion } from "./processes.ts";

export interface LoadSpec {
    ohaExecutable: string;
    url: string;
    method: "GET" | "POST";
    /** `application/x-www-form-urlencoded` body for POST. */
    form?: string;
    concurrency: number;
    durationSeconds: number;
}

export interface LoadResult {
    requests: number;
    requestsPerSecond: number;
    successRate: number;
    statusCodes: Record<string, number>;
    errors: Record<string, number>;
    latencyMs: { p50: number; p90: number; p95: number; p99: number };
    bytesPerRequest: number;
}

const SECONDS_TO_MS = 1_000;

/**
 * Drives `oha` for a fixed duration at fixed concurrency. Ongoing requests are awaited at the
 * deadline (`-w`) so that every request the server received is also counted.
 */
export async function runLoad(spec: LoadSpec): Promise<LoadResult> {
    let args = [
        "--no-tui",
        "--output-format",
        "json",
        "-c",
        String(spec.concurrency),
        "-z",
        `${spec.durationSeconds}s`,
        "-w",
        ...(spec.method === "POST"
            ? ["-m", "POST", "-T", "application/x-www-form-urlencoded", "-d", spec.form ?? ""]
            : []),
        spec.url,
    ];
    let stdout = await runToCompletion(
        { command: spec.ohaExecutable, args },
        { cwd: process.cwd(), env: baseEnvironment(), label: `oha ${spec.method} ${spec.url}` },
    );
    return parseLoadResult(JSON.parse(stdout));
}

export function parseLoadResult(report: unknown): LoadResult {
    let statusCodes = counts(lookup(report, "statusCodeDistribution"));
    return {
        requests: Object.values(statusCodes).reduce((total, count) => total + count, 0),
        requestsPerSecond: readNumber(report, "summary", "requestsPerSec"),
        successRate: readNumber(report, "summary", "successRate"),
        statusCodes,
        errors: counts(lookup(report, "errorDistribution")),
        latencyMs: {
            p50: readNumber(report, "latencyPercentiles", "p50") * SECONDS_TO_MS,
            p90: readNumber(report, "latencyPercentiles", "p90") * SECONDS_TO_MS,
            p95: readNumber(report, "latencyPercentiles", "p95") * SECONDS_TO_MS,
            p99: readNumber(report, "latencyPercentiles", "p99") * SECONDS_TO_MS,
        },
        bytesPerRequest: readNumber(report, "summary", "sizePerRequest"),
    };
}

function counts(value: unknown): Record<string, number> {
    if (typeof value !== "object" || value === null) return {};
    return Object.fromEntries(
        Object.entries(value).filter(
            (entry): entry is [string, number] => typeof entry[1] === "number",
        ),
    );
}
