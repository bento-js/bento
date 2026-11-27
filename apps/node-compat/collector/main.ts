import { constants } from "node:os";

import type { ResultStore, RunReport } from "../lib/model.ts";
import type { CorpusCase } from "./corpus.ts";
import type { CollectOptions } from "./options.ts";

import { openStore } from "../data/store.ts";
import { summarizeRun } from "../lib/summary.ts";
import { collectBehavior } from "./behavior.ts";
import { listCorpus, selectCases } from "./corpus.ts";
import { SetupError, describeError } from "./errors.ts";
import { collectInventory, INVENTORY_SUITE } from "./inventory.ts";
import { USAGE, parseOptions } from "./options.ts";
import { createReport, recordRun } from "./report.ts";
import { bentoIdentity, changedRuntimes, cleanEnvironment, nodeIdentity } from "./runtimes.ts";
import { createScratch } from "./scratch.ts";
import { NODE_TAG, SUITE, verifyUpstream } from "./upstream.ts";

interface Session {
    options: CollectOptions;
    store: ResultStore;
    runtimes: RunReport["runtimes"];
    environment: NodeJS.ProcessEnv;
    signal: AbortSignal;
}

interface BehaviorPlan {
    checkout: string;
    total: number;
    cases: CorpusCase[];
}

async function main(argv: string[]): Promise<number> {
    let options = parseOptions(argv);
    if (options === null) {
        process.stdout.write(USAGE);
        return 0;
    }
    let environment = cleanEnvironment(process.env);
    let launch = { cwd: process.cwd(), environment };
    let runtimes = {
        node: await nodeIdentity(process.execPath, launch),
        bento: await bentoIdentity(options.bento, options.bentoArguments, launch),
    };
    console.log(
        `Node control ${runtimes.node.version}: ${runtimes.node.executable} (sha256 ${runtimes.node.sha256})`,
    );
    console.log(
        `Bento ${runtimes.bento.version}: ${runtimes.bento.executable} ${runtimes.bento.arguments.join(" ")} (sha256 ${runtimes.bento.sha256})`,
    );
    let plan = options.kinds.includes("behavior") ? await planBehavior(options.filter) : null;

    let controller = new AbortController();
    let signals = watchSignals(controller);
    let store = await openStore();
    try {
        let session: Session = { options, store, runtimes, environment, signal: controller.signal };
        let runs: (() => Promise<RunReport>)[] = [];
        if (options.kinds.includes("inventory")) runs.push(() => runInventory(session));
        if (plan !== null) runs.push(() => runBehavior(session, plan));
        let failed = false;
        for (let run of runs) {
            if (controller.signal.aborted) break;
            let report = await run();
            printSummary(report);
            failed ||= report.state === "failed";
        }
        return signals.exitCode() ?? (failed ? 1 : 0);
    } finally {
        signals.dispose();
        await store.close();
    }
}

function watchSignals(controller: AbortController) {
    let received: NodeJS.Signals | null = null;
    let onSignal = (signal: NodeJS.Signals) => {
        if (received !== null) process.exit(128 + constants.signals[signal]);
        received = signal;
        console.error(
            `\n${signal}: killing the running test process group and recording the run as interrupted. Repeat to exit immediately.`,
        );
        controller.abort();
    };
    process.on("SIGINT", onSignal);
    process.on("SIGTERM", onSignal);
    return {
        exitCode: (): number | null =>
            received === null ? null : 128 + constants.signals[received],
        dispose: () => {
            process.off("SIGINT", onSignal);
            process.off("SIGTERM", onSignal);
        },
    };
}

async function planBehavior(filter: string | null): Promise<BehaviorPlan> {
    let checkout = await verifyUpstream();
    let corpus = await listCorpus(checkout);
    let cases = selectCases(corpus, filter);
    let selection =
        filter === null
            ? "all selected"
            : `${cases.length} selected by --filter ${JSON.stringify(filter)}`;
    console.log(
        `Behavioral corpus: Node ${NODE_TAG} test/parallel and test/sequential, ${corpus.length} files; ${selection}`,
    );
    return { checkout, total: corpus.length, cases };
}

function runInventory(session: Session): Promise<RunReport> {
    let { options, runtimes, environment, signal } = session;
    let report = createReport({
        kind: "inventory",
        suite: INVENTORY_SUITE,
        runtimes,
        timeoutMs: options.timeoutMs,
        selection: { filter: "", total: 0, selected: 0 },
    });
    console.log(`Availability run ${report.id}: presence and typeof only`);
    let context = { ...runtimes, environment, timeoutMs: options.timeoutMs, signal };
    return recordRun(session.store, report, {
        signal,
        verify: () => changedRuntimes(runtimes),
        collect: current => collectInventory(current, context, console.log),
    });
}

async function runBehavior(session: Session, plan: BehaviorPlan): Promise<RunReport> {
    let { options, runtimes, environment, signal } = session;
    let scratch = await createScratch(plan.checkout);
    try {
        let report = createReport({
            kind: "behavior",
            suite: SUITE,
            runtimes,
            timeoutMs: options.timeoutMs,
            selection: {
                filter: options.filter ?? "",
                total: plan.total,
                selected: plan.cases.length,
            },
        });
        console.log(
            `Behavior run ${report.id}: ${plan.cases.length} files, ${options.timeoutMs} ms per process`,
        );
        let context = {
            ...runtimes,
            workspaces: scratch,
            environment,
            timeoutMs: options.timeoutMs,
            signal,
        };
        return await recordRun(session.store, report, {
            signal,
            verify: () => changedRuntimes(runtimes),
            collect: current => collectBehavior(current, plan.cases, context, console.log),
        });
    } finally {
        await scratch
            .remove()
            .catch(error =>
                console.warn(`warning: could not remove scratch copy: ${describeError(error)}`),
            );
    }
}

function printSummary(report: RunReport): void {
    let summary = summarizeRun(report);
    if (report.kind === "behavior") {
        console.log(
            `Behavior run ${summary.id} ${summary.state}: ${summary.recorded} of ${summary.selected} selected files recorded (corpus ${summary.total}). ` +
                `Node control not passing: ${summary.baselineFailed}. Bento on Node-passing files: ${summary.passed} passed, ` +
                `${summary.failed} failed, ${summary.skipped} skipped, ${summary.unsupported} unsupported, ${summary.timeout} timed out, ` +
                `${summary.crashed} crashed, ${summary.notRun} not run.`,
        );
    } else {
        console.log(
            `Availability run ${summary.id} ${summary.state}: ${summary.recorded} of ${summary.selected} Node baseline entries inspected; ` +
                `${summary.available} present, ${summary.missing} missing, ${summary.typeMismatch} type mismatches, ` +
                `${summary.inspectionErrors} inspection errors. Presence is not behavioral evidence.`,
        );
    }
    if (report.error !== null) console.error(`Run ${report.id} failed: ${report.error}`);
}

try {
    process.exitCode = await main(process.argv.slice(2));
} catch (error) {
    if (!(error instanceof SetupError)) throw error;
    console.error(`error: ${error.message}`);
    process.exitCode = 1;
}
