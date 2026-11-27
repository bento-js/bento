import type { BehaviorResult, Execution, RunReport, RuntimeIdentity } from "../lib/model.ts";
import type { CorpusCase } from "./corpus.ts";
import type { Workspace } from "./scratch.ts";

import { notExecuted, toExecution } from "./classify.ts";
import { runProcess } from "./process.ts";
import { cleanEnvironment } from "./runtimes.ts";

export interface BehaviorContext {
    node: RuntimeIdentity;
    bento: RuntimeIdentity;
    workspaces: { node: Workspace; bento: Workspace };
    environment: NodeJS.ProcessEnv;
    timeoutMs: number;
    signal: AbortSignal;
}

export async function collectBehavior(
    report: RunReport,
    cases: CorpusCase[],
    context: BehaviorContext,
    progress: (line: string) => void,
): Promise<void> {
    let width = String(cases.length).length;
    for (let [index, testCase] of cases.entries()) {
        let result = await runCase(testCase, context);
        if (result === null) return;
        report.behavior.push(result);
        let position = String(index + 1).padStart(width);
        progress(
            `[${position}/${cases.length}] ${testCase.id}  node: ${result.node.status}  bento: ${result.bento.status}`,
        );
    }
}

export async function runCase(
    testCase: CorpusCase,
    context: BehaviorContext,
): Promise<BehaviorResult | null> {
    let node = await execute(
        context.node,
        context.workspaces.node,
        testCase,
        testCase.flags,
        context,
    );
    if (node === null) return null;
    let bento =
        testCase.flags.length > 0
            ? notExecuted(
                  "unsupported",
                  `bento run accepts no Node.js CLI flags; this test requires ${testCase.flags.join(" ")}`,
              )
            : await execute(context.bento, context.workspaces.bento, testCase, [], context);
    if (bento === null) return null;
    return { id: testCase.id, module: testCase.module, flags: testCase.flags, node, bento };
}

async function execute(
    runtime: RuntimeIdentity,
    workspace: Workspace,
    testCase: CorpusCase,
    flags: string[],
    context: BehaviorContext,
): Promise<Execution | null> {
    let result = await runProcess({
        command: [runtime.executable, ...runtime.arguments, ...flags, testCase.path],
        cwd: workspace.root,
        env: harnessEnvironment(context.environment, workspace, testCase),
        timeoutMs: context.timeoutMs,
        signal: context.signal,
    });
    return result.interrupted ? null : toExecution(result, context.timeoutMs);
}

function harnessEnvironment(
    base: NodeJS.ProcessEnv,
    workspace: Workspace,
    testCase: CorpusCase,
): NodeJS.ProcessEnv {
    return {
        ...cleanEnvironment(base),
        ...testCase.env,
        NODE_TEST_DIR: workspace.testDirectory,
        TEST_SERIAL_ID: "0",
        TEST_THREAD_ID: "0",
        TEST_PARALLEL: testCase.id.startsWith("parallel/") ? "1" : "0",
        GITHUB_STEP_SUMMARY: "",
        NODE_SKIP_FLAG_CHECK: "true",
        NODE_TEST_KNOWN_GLOBALS: "0",
    };
}
