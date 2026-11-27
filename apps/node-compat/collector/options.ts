import { parseArgs } from "node:util";

import type { RunReport } from "../lib/model.ts";

import { SetupError, describeError } from "./errors.ts";

export const DEFAULT_TIMEOUT_MS = 120_000;
const MAX_TIMEOUT_MS = 2_147_483_647;

export const USAGE = `Usage: node collector/main.ts [options]

  --kind all|behavior|inventory  Evidence to collect (default: all)
  --filter SUBSTRING             Run only behavioral test IDs containing SUBSTRING
  --bento PATH                   Bento executable to measure (default: bento on PATH)
  --bento-arg=ARG                Extra \`bento run\` argument; repeat for several
  --timeout-ms MS                Per-process deadline (default: ${DEFAULT_TIMEOUT_MS})
  -h, --help                     Show this help
`;

const KINDS = new Map<string, RunReport["kind"][]>([
    ["all", ["inventory", "behavior"]],
    ["behavior", ["behavior"]],
    ["inventory", ["inventory"]],
]);

export interface CollectOptions {
    kinds: RunReport["kind"][];
    filter: string | null;
    bento: string;
    bentoArguments: string[];
    timeoutMs: number;
}

export function parseOptions(argv: string[]): CollectOptions | null {
    let values;
    try {
        ({ values } = parseArgs({
            args: argv,
            strict: true,
            allowPositionals: false,
            options: {
                kind: { type: "string" },
                filter: { type: "string" },
                bento: { type: "string" },
                "bento-arg": { type: "string", multiple: true },
                "timeout-ms": { type: "string" },
                help: { type: "boolean", short: "h" },
            },
        }));
    } catch (error) {
        throw new SetupError(`${describeError(error)}\n\n${USAGE}`);
    }
    if (values.help) return null;
    let kinds = KINDS.get(values.kind ?? "all");
    if (kinds === undefined)
        throw new SetupError(
            `--kind must be all, behavior, or inventory, not ${JSON.stringify(values.kind)}`,
        );
    let filter = values.filter ?? null;
    if (filter === "")
        throw new SetupError("--filter needs a non-empty substring of a behavioral test ID");
    if (filter !== null && !kinds.includes("behavior")) {
        throw new SetupError(
            "--filter selects behavioral tests; the availability inventory always covers the full Node baseline",
        );
    }
    return {
        kinds: [...kinds],
        filter,
        bento: values.bento ?? "bento",
        bentoArguments: values["bento-arg"] ?? [],
        timeoutMs: parseTimeout(values["timeout-ms"]),
    };
}

function parseTimeout(value: string | undefined): number {
    if (value === undefined) return DEFAULT_TIMEOUT_MS;
    let timeoutMs = /^[1-9]\d*$/.test(value) ? Number(value) : Number.NaN;
    if (!(timeoutMs <= MAX_TIMEOUT_MS)) {
        throw new SetupError(
            `--timeout-ms must be a whole number of milliseconds from 1 to ${MAX_TIMEOUT_MS}, not ${JSON.stringify(value)}`,
        );
    }
    return timeoutMs;
}
