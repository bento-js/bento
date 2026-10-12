import type { Compatibility } from "./compatibility.ts";
import type {
    RuntimeName,
    SampleRow,
    Series,
    TrialFailure,
    WorkloadResult,
} from "./results-model.ts";
import type { RunContext, Workload } from "./workload.ts";

import { resetDatabase } from "./database.ts";
import { throwIfInterrupted } from "./interruption.ts";

interface Contender {
    runtime: RuntimeName;
    executable: string;
}

/** Runs every trial of a workload, one runtime at a time, alternating who goes first. */
export async function runWorkload(
    workload: Workload,
    context: RunContext,
): Promise<WorkloadResult> {
    let series: Series[] = [];
    for (let { label, mode } of workload.variants) {
        let contenders: Contender[] = [];
        for (let runtime of context.options.runtimes) {
            let unready = await findObstacle(workload, context, runtime, mode);
            if (unready !== undefined) {
                series.push({ runtime, variant: label, outcome: unready });
            } else {
                contenders.push({ runtime, executable: executableOf(context, runtime) });
            }
        }
        series.push(...(await measure(workload, context, label, mode, contenders)));
    }
    return {
        workload: workload.name,
        title: workload.title,
        question: workload.question,
        metrics: workload.metrics,
        series,
        notes: workload.notes?.(series) ?? [],
    };
}

async function findObstacle(
    workload: Workload,
    context: RunContext,
    runtime: RuntimeName,
    mode: "development" | "production",
): Promise<Series["outcome"] | undefined> {
    let availability = context.availability[runtime];
    if (!availability.available) return { status: "unavailable", reason: availability.reason };
    if (!workload.requiresApplicationGate) return undefined;

    let verdict: Compatibility = await context.compatibility({
        runtime,
        executable: availability.executable,
        fixture: await context.fixtureFor(runtime),
        mode,
    });
    if (verdict.compatible) return undefined;
    context.log(`${runtime} is incompatible at the ${verdict.check} check (${mode})`);
    return {
        status: "incompatible",
        check: verdict.check,
        failures: verdict.failures,
        evidence: verdict.evidence,
    };
}

async function measure(
    workload: Workload,
    context: RunContext,
    variant: string,
    mode: "development" | "production",
    contenders: Contender[],
): Promise<Series[]> {
    let samples = new Map<RuntimeName, SampleRow[]>();
    let failures = new Map<RuntimeName, TrialFailure[]>();
    for (let { runtime } of contenders) {
        samples.set(runtime, []);
        failures.set(runtime, []);
    }

    let trials = context.options.trials[workload.name];
    for (let trial = 0; trial < trials; trial++) {
        for (let { runtime, executable } of rotate(contenders, trial)) {
            context.log(`${workload.name} ${variant} ${runtime} trial ${trial + 1}/${trials}`);
            try {
                let fixture = await context.fixtureFor(runtime);
                await resetDatabase(fixture.directory);
                let row = await workload.runTrial({
                    runtime,
                    executable,
                    fixture,
                    mode,
                    trial,
                    context,
                });
                samples.get(runtime)!.push(row);
            } catch (error) {
                throwIfInterrupted();
                let message = error instanceof Error ? error.message : String(error);
                context.log(`  failed: ${message.split("\n")[0]}`);
                failures.get(runtime)!.push({ trial, message });
            }
        }
    }

    return contenders.map(({ runtime }) => {
        let measured = samples.get(runtime)!;
        let trialFailures = failures.get(runtime)!;
        let disqualified =
            measured.length === 0 ||
            (workload.everyTrialMustSucceed === true && trialFailures.length > 0);
        let outcome: Series["outcome"] = disqualified
            ? { status: "failed", failures: trialFailures }
            : { status: "measured", samples: measured, failures: trialFailures };
        return { runtime, variant, outcome };
    });
}

function executableOf(context: RunContext, runtime: RuntimeName): string {
    let availability = context.availability[runtime];
    if (!availability.available) throw new Error(`${runtime} is not available`);
    return availability.executable;
}

function rotate<Item>(items: Item[], offset: number): Item[] {
    if (items.length === 0) return items;
    let shift = offset % items.length;
    return [...items.slice(shift), ...items.slice(0, shift)];
}
