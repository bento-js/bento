import { parseArgs } from "node:util";

import type { RuntimeName, WorkloadName } from "./results-model.ts";

import { RUNTIME_NAMES, WORKLOAD_NAMES } from "./results-model.ts";

export interface BenchOptions {
    help: boolean;
    runtimes: RuntimeName[];
    workloads: WorkloadName[];
    trials: Record<WorkloadName, number>;
    steadyState: { durationSeconds: number; warmupSeconds: number; concurrency: number };
    iterations: number;
    outputDirectory: string | undefined;
    keepFixtures: boolean;
}

const DEFAULT_TRIALS: Record<WorkloadName, number> = {
    "cold-launch": 15,
    assets: 10,
    "asset-operations": 10,
    "steady-state": 5,
    "watch-reload": 10,
    memory: 10,
};

export const USAGE = `Usage: node bench/src/main.ts [options]

  --runtimes <list>    Comma-separated: ${RUNTIME_NAMES.join(", ")} (default: all)
  --workloads <list>   Comma-separated: ${WORKLOAD_NAMES.join(", ")} (default: all)
  --trials <n>         Trials per workload (default: 15 cold launches, 5 steady-state runs, 10 others)
  --duration <s>       Measured seconds per steady-state scenario (default: 10)
  --warmup <s>         Warmup seconds per steady-state scenario (default: 3)
  --concurrency <n>    Steady-state connections (default: 16)
  --iterations <n>     Repeat passes for asset operations and repeat asset requests (default: 10)
  --output <dir>       Results directory (default: bench/results/<UTC timestamp>)
  --keep-fixtures      Keep the disposable fixture copies under /tmp/bento-bench
  --help               Show this message`;

export function parseOptions(argv: string[]): BenchOptions {
    let { values } = parseArgs({
        args: argv,
        options: {
            runtimes: { type: "string" },
            workloads: { type: "string" },
            trials: { type: "string" },
            duration: { type: "string" },
            warmup: { type: "string" },
            concurrency: { type: "string" },
            iterations: { type: "string" },
            output: { type: "string" },
            "keep-fixtures": { type: "boolean" },
            help: { type: "boolean" },
        },
        strict: true,
    });

    let trialOverride =
        values.trials === undefined ? undefined : positiveInteger("--trials", values.trials);
    return {
        help: values.help ?? false,
        runtimes: parseRuntimes(values.runtimes),
        workloads: parseWorkloads(values.workloads),
        trials: Object.fromEntries(
            WORKLOAD_NAMES.map(name => [name, trialOverride ?? DEFAULT_TRIALS[name]]),
        ) as Record<WorkloadName, number>,
        steadyState: {
            durationSeconds: positiveInteger("--duration", values.duration ?? "10"),
            warmupSeconds: positiveInteger("--warmup", values.warmup ?? "3"),
            concurrency: positiveInteger("--concurrency", values.concurrency ?? "16"),
        },
        iterations: positiveInteger("--iterations", values.iterations ?? "10"),
        outputDirectory: values.output,
        keepFixtures: values["keep-fixtures"] ?? false,
    };
}

function parseRuntimes(list: string | undefined): RuntimeName[] {
    if (list === undefined) return [...RUNTIME_NAMES];
    return list.split(",").map(name => {
        let runtime = RUNTIME_NAMES.find(known => known === name);
        if (runtime === undefined) {
            throw new Error(`Unknown runtime "${name}"; expected node, bun, or bento`);
        }
        return runtime;
    });
}

function parseWorkloads(list: string | undefined): WorkloadName[] {
    if (list === undefined) return [...WORKLOAD_NAMES];
    return list.split(",").map(name => {
        let workload = WORKLOAD_NAMES.find(known => known === name);
        if (workload === undefined) {
            throw new Error(
                `Unknown workload "${name}"; expected one of ${WORKLOAD_NAMES.join(", ")}`,
            );
        }
        return workload;
    });
}

function positiveInteger(flag: string, text: string): number {
    let value = Number(text);
    if (!Number.isInteger(value) || value <= 0)
        throw new Error(`${flag} must be a positive integer`);
    return value;
}
