import type { BehaviorResult, RunReport } from "./model.ts";

export interface RunComparison {
    comparable: boolean;
    reason: string | null;
    regressions: BehaviorResult[];
    improvements: BehaviorResult[];
}

export function compareRuns(current: RunReport, previous: RunReport): RunComparison {
    let reason = incomparableReason(current, previous);
    let comparison: RunComparison = {
        comparable: reason === null,
        reason,
        regressions: [],
        improvements: [],
    };
    if (reason !== null) return comparison;
    let previousTests = new Map(previous.behavior.map(test => [test.id, test]));
    for (let test of current.behavior) {
        let before = previousTests.get(test.id)!;
        if (test.node.status !== "passed" || before.node.status !== "passed") continue;
        if (before.bento.status === "passed" && test.bento.status !== "passed") {
            comparison.regressions.push(test);
        } else if (before.bento.status !== "passed" && test.bento.status === "passed") {
            comparison.improvements.push(test);
        }
    }
    return comparison;
}

function incomparableReason(current: RunReport, previous: RunReport): string | null {
    if (current.kind !== "behavior" || previous.kind !== "behavior") {
        return "Only behavioral runs can be compared.";
    }
    if (current.state !== "complete" || previous.state !== "complete") {
        return "Both runs must be complete.";
    }
    if (
        current.suite.revision !== previous.suite.revision ||
        current.suite.scope !== previous.suite.scope
    ) {
        return "The upstream corpus changed.";
    }
    if (current.platform !== previous.platform || current.arch !== previous.arch) {
        return "The operating system or architecture changed.";
    }
    if (current.timeoutMs !== previous.timeoutMs) return "The execution deadline changed.";
    if (current.selection.filter !== previous.selection.filter)
        return "The test selection changed.";
    if (JSON.stringify(current.runtimes.node) !== JSON.stringify(previous.runtimes.node)) {
        return "The Node control or its invocation changed.";
    }
    if (
        JSON.stringify(current.runtimes.bento.arguments) !==
        JSON.stringify(previous.runtimes.bento.arguments)
    ) {
        return "The Bento launch arguments changed.";
    }
    let ids = new Set(previous.behavior.map(test => test.id));
    let currentIds = new Set(current.behavior.map(test => test.id));
    if (
        current.behavior.length !== current.selection.selected ||
        previous.behavior.length !== previous.selection.selected ||
        ids.size !== previous.behavior.length ||
        currentIds.size !== current.behavior.length
    )
        return "A run has missing or duplicate test results.";
    if (currentIds.size !== ids.size || current.behavior.some(test => !ids.has(test.id))) {
        return "The recorded test files changed.";
    }
    return null;
}
