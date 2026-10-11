import { rm } from "node:fs/promises";
import path from "node:path";

import type { RunRecord, RuntimeName, WorkloadResult } from "./results-model.ts";
import type { RunContext } from "./workload.ts";

import { createCompatibilityGate } from "./compatibility.ts";
import { collectEnvironment } from "./environment.ts";
import { findOnPath } from "./executables.ts";
import { createFixturePool } from "./fixture-pool.ts";
import { parseOptions, USAGE } from "./options.ts";
import { installCleanupHandlers } from "./processes.ts";
import { timestampName, writeResults } from "./results.ts";
import { runWorkload } from "./run-workload.ts";
import { detectRuntimes } from "./runtimes.ts";
import { WORKLOADS } from "./workloads/index.ts";

const REPOSITORY_ROOT = path.resolve(import.meta.dirname, "../..");
const SCRATCH_ROOT = "/tmp/bento-bench/runs";

async function main(): Promise<void> {
    let options = parseOptions(process.argv.slice(2));
    if (options.help) {
        console.log(USAGE);
        return;
    }
    installCleanupHandlers();

    let started = new Date();
    let runDirectory = path.join(SCRATCH_ROOT, timestampName(started));
    let outputDirectory =
        options.outputDirectory ??
        path.join(REPOSITORY_ROOT, "bench/results", timestampName(started));
    let log = (message: string) => console.error(`[bench] ${message}`);

    let availability = await detectRuntimes(process.env);
    let ohaExecutable = await findLoadGenerator();
    if (options.workloads.includes("steady-state") && ohaExecutable === undefined) {
        throw new Error(
            "steady-state needs oha on PATH or in ~/.cargo/bin: cargo install --locked oha",
        );
    }
    for (let runtime of options.runtimes) {
        let status = availability[runtime];
        log(
            status.available
                ? `${runtime}: ${status.executable}`
                : `${runtime} unavailable: ${status.reason}`,
        );
    }

    let fixtures = createFixturePool({
        sourceDirectory: path.join(REPOSITORY_ROOT, "fixtures/remix-guest-book"),
        runDirectory,
    });
    let context: RunContext = {
        options,
        availability,
        fixtureFor: fixtures.fixtureFor,
        compatibility: createCompatibilityGate(),
        ohaExecutable,
        log,
    };

    try {
        let workloads: WorkloadResult[] = [];
        for (let name of options.workloads) {
            log(`workload ${name}`);
            workloads.push(await runWorkload(WORKLOADS[name], context));
        }
        let run: RunRecord = {
            startedAt: started.toISOString(),
            options: { ...options, executables: executablesOf(availability, options.runtimes) },
            environment: await collectEnvironment({
                availability,
                remixVersion: fixtures.prepared()[0]?.remixVersion ?? "not installed",
                fixtureDirectory: fixtures.prepared()[0]?.directory,
                ohaExecutable,
                repositoryRoot: REPOSITORY_ROOT,
            }),
            workloads,
        };
        let summaryPath = await writeResults(outputDirectory, run);
        log(`results written to ${outputDirectory}`);
        console.log(summaryPath);
    } finally {
        if (!options.keepFixtures) await rm(runDirectory, { recursive: true, force: true });
    }
}

async function findLoadGenerator(): Promise<string | undefined> {
    let home = process.env.HOME ?? "";
    let searchPath = `${process.env.PATH ?? ""}${path.delimiter}${path.join(home, ".cargo/bin")}`;
    return findOnPath("oha", { PATH: searchPath });
}

function executablesOf(
    availability: RunContext["availability"],
    runtimes: RuntimeName[],
): Record<string, string> {
    return Object.fromEntries(
        runtimes.map(runtime => {
            let status = availability[runtime];
            return [runtime, status.available ? status.executable : "unavailable"];
        }),
    );
}

if (import.meta.main) {
    main().catch((error: unknown) => {
        console.error(error instanceof Error ? error.message : error);
        process.exitCode = 1;
    });
}
