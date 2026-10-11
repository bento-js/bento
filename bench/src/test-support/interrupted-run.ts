import { appendFile, mkdir, writeFile } from "node:fs/promises";
import path from "node:path";

import type { RunContext, Workload } from "../workload.ts";

import { reportRunFailure } from "../interruption.ts";
import { baseEnvironment } from "../launch-commands.ts";
import { parseOptions } from "../options.ts";
import { installCleanupHandlers, spawnManaged } from "../processes.ts";
import { runWorkload } from "../run-workload.ts";

/**
 * A run shaped like the real harness: three trials of one workload, each starting a long-lived
 * process and cleaning up in a `finally`. Tests interrupt it and inspect what it left behind.
 */
let markers = process.argv[2];
if (markers === undefined) throw new Error("usage: interrupted-run.ts <marker directory>");
let markerDirectory = markers;

let workload: Workload = {
    name: "cold-launch",
    title: "Interrupted run",
    question: "Does an interruption clean up?",
    metrics: [],
    variants: [{ label: "", mode: "development" }],
    requiresApplicationGate: false,
    async runTrial({ trial }) {
        await appendFile(path.join(markerDirectory, "trials"), `${trial}\n`);
        let child = await spawnManaged(
            { command: "sleep", args: ["60"] },
            { cwd: markerDirectory, env: baseEnvironment() },
        );
        console.log(`started ${child.pid}`);
        try {
            await child.closed;
            throw new Error("the process stopped before the trial finished");
        } finally {
            await writeFile(path.join(markerDirectory, "cleaned-up"), "yes");
        }
    },
};

let context: RunContext = {
    options: parseOptions(["--runtimes", "node", "--workloads", "cold-launch", "--trials", "3"]),
    availability: {
        node: { available: true, executable: process.execPath, version: process.version },
        bun: { available: false, reason: "not used" },
        bento: { available: false, reason: "not used" },
    },
    fixtureFor: async () => ({ directory: markerDirectory, databaseUrl: "", remixVersion: "" }),
    compatibility: async () => ({ compatible: true }),
    ohaExecutable: undefined,
    log: () => {},
};

installCleanupHandlers();
await mkdir(path.join(markerDirectory, "db"), { recursive: true });
await mkdir(path.join(markerDirectory, ".bench/seed"), { recursive: true });
try {
    await runWorkload(workload, context);
} catch (error) {
    reportRunFailure(error);
}
