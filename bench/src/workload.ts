import type { Compatibility, CompatibilityRequest } from "./compatibility.ts";
import type { Fixture } from "./fixture.ts";
import type { BenchOptions } from "./options.ts";
import type {
    Metric,
    Mode,
    RuntimeName,
    SampleRow,
    Series,
    WorkloadName,
} from "./results-model.ts";
import type { RuntimeAvailability } from "./runtimes.ts";

export interface RunContext {
    options: BenchOptions;
    availability: Record<RuntimeName, RuntimeAvailability>;
    /** A fresh disposable fixture per runtime, prepared on first use. */
    fixtureFor: (runtime: RuntimeName) => Promise<Fixture>;
    /** Runs the acceptance gates once per runtime and mode. */
    compatibility: (request: CompatibilityRequest) => Promise<Compatibility>;
    ohaExecutable: string | undefined;
    log: (message: string) => void;
}

export interface TrialInput {
    runtime: RuntimeName;
    executable: string;
    fixture: Fixture;
    mode: Mode;
    trial: number;
    context: RunContext;
}

export interface Workload {
    name: WorkloadName;
    title: string;
    question: string;
    metrics: Metric[];
    /** `label` heads a table in the summary; an empty label means the workload has no variants. */
    variants: Array<{ label: string; mode: Mode }>;
    /** Whether a runtime must pass the application's acceptance gates before being timed. */
    requiresApplicationGate: boolean;
    /**
     * Whether one failed trial fails the runtime, discarding its other trials' timings. Set when
     * each trial checks the correctness of the work it times, so that a failure means the runtime
     * did the work wrongly rather than that one attempt went wrong.
     */
    everyTrialMustSucceed?: boolean;
    /** Runs one measured trial against a freshly reset database. */
    runTrial(input: TrialInput): Promise<SampleRow>;
    notes?(series: Series[]): string[];
}

export const BOTH_MODES: Workload["variants"] = [
    { label: "development", mode: "development" },
    { label: "production", mode: "production" },
];
