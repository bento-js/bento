export type Outcome =
    | "passed"
    | "failed"
    | "skipped"
    | "timeout"
    | "unsupported"
    | "crashed"
    | "not-run";

export interface Execution {
    status: Outcome;
    command: string[];
    exitCode: number | null;
    signal: string | null;
    durationMs: number;
    stdout: string;
    stderr: string;
    reason: string | null;
    truncated: boolean;
}

export interface BehaviorResult {
    id: string;
    module: string;
    flags: string[];
    node: Execution;
    bento: Execution;
}

export interface InventoryResult {
    module: string;
    name: string;
    node: string;
    bento: string | null;
    status: "present" | "missing" | "type-mismatch" | "error";
    error: string | null;
}

export interface RuntimeIdentity {
    version: string;
    executable: string;
    sha256: string;
    arguments: string[];
}

export interface RunReport {
    schemaVersion: 1;
    id: string;
    kind: "behavior" | "inventory";
    state: "running" | "complete" | "interrupted" | "failed";
    startedAt: string;
    finishedAt: string | null;
    suite: {
        version: string;
        revision: string;
        source: string;
        scope: string;
    };
    platform: string;
    arch: string;
    timeoutMs: number;
    runtimes: { node: RuntimeIdentity; bento: RuntimeIdentity };
    selection: { filter: string; total: number; selected: number };
    behavior: BehaviorResult[];
    inventory: InventoryResult[];
    error: string | null;
}

export interface RunSummary {
    id: string;
    kind: RunReport["kind"];
    state: RunReport["state"];
    startedAt: string;
    finishedAt: string | null;
    nodeVersion: string;
    bentoVersion: string;
    suiteRevision: string;
    platform: string;
    arch: string;
    filter: string;
    total: number;
    selected: number;
    recorded: number;
    passed: number;
    failed: number;
    skipped: number;
    unsupported: number;
    timeout: number;
    crashed: number;
    notRun: number;
    baselineFailed: number;
    available: number;
    typeMismatch: number;
    inspectionErrors: number;
    missing: number;
}

export interface ResultStore {
    list(): Promise<RunSummary[]>;
    get(id: string): Promise<RunReport | null>;
    save(report: RunReport): Promise<void>;
    close(): Promise<void>;
}
