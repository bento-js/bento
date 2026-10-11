export const RUNTIME_NAMES = ["node", "bun", "bento"] as const;
export type RuntimeName = (typeof RUNTIME_NAMES)[number];

export const WORKLOAD_NAMES = [
    "cold-launch",
    "assets",
    "asset-operations",
    "steady-state",
    "watch-reload",
    "memory",
] as const;
export type WorkloadName = (typeof WORKLOAD_NAMES)[number];

export type Mode = "development" | "production";

export interface HttpEvidence {
    method: string;
    url: string;
    status: number;
    contentType: string | null;
    location: string | null;
    bodyBytes: number;
    bodyExcerpt: string;
}

export interface Metric {
    key: string;
    label: string;
    unit: string;
}

/** One trial: numeric fields named by a workload's metrics, plus any raw detail worth keeping. */
export type SampleRow = Record<string, unknown> & { trial: number };

export interface TrialFailure {
    trial: number;
    message: string;
}

export type SeriesOutcome =
    | { status: "measured"; samples: SampleRow[]; failures: TrialFailure[] }
    | { status: "failed"; failures: TrialFailure[] }
    | { status: "incompatible"; check: string; failures: string[]; evidence: HttpEvidence[] }
    | { status: "unavailable"; reason: string };

export interface Series {
    runtime: RuntimeName;
    variant: string;
    outcome: SeriesOutcome;
}

export interface WorkloadResult {
    workload: WorkloadName;
    title: string;
    question: string;
    metrics: Metric[];
    series: Series[];
    notes: string[];
}

export interface Environment {
    hardware: string;
    os: string;
    runtimes: Record<string, string>;
    tools: Record<string, string>;
    revisions: Record<string, string>;
    remixVersion: string;
}

export interface RunRecord {
    startedAt: string;
    options: Record<string, unknown>;
    environment: Environment;
    workloads: WorkloadResult[];
}
