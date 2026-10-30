import assert from "node:assert/strict";
import { test } from "node:test";

import {
    defaultBranchFacts,
    deriveStatus,
    formatStatus,
    type Facts,
    type Proposal,
    type Step,
} from "./status.ts";

let openPullRequest = {
    number: 1,
    isDraft: true,
    state: "OPEN" as const,
    baseRefName: "main",
    url: "https://github.com/owner/repo/pull/1",
    title: "Feature",
};

function facts(overrides: Partial<Facts> = {}): Facts {
    return {
        branch: "feature",
        isDefaultBranch: false,
        defaultBranch: "main",
        hasUpstream: true,
        unpushedCommits: 0,
        pullRequest: openPullRequest,
        pullRequestState: "known",
        proposal: null,
        ...overrides,
    };
}
function proposal(status: string, clarificationCount = 0, overrides: Partial<Proposal> = {}) {
    return {
        id: "proposal.0001",
        title: "Feature",
        status,
        clarificationCount,
        ...overrides,
    };
}

function assertPhase(input: Facts, expected: Pick<Step, "phase" | "title" | "actor" | "skill">) {
    let status = deriveStatus(input);

    assert.deepEqual(
        {
            phase: status.phase,
            title: status.title,
            actor: status.actor,
            skill: status.skill,
        },
        expected,
    );
    return status;
}

function assertContains(report: string, expected: string): void {
    assert.ok(report.includes(expected), `expected report to contain ${expected}:\n${report}`);
}

test("derives preparation on the default branch", () => {
    assertPhase(facts({ branch: "main", isDefaultBranch: true, pullRequest: null }), {
        phase: "1",
        title: "Preparation",
        actor: "agent",
        skill: null,
    });
});

test("derives preparation for a branch without a pull request", () => {
    assertPhase(facts({ pullRequest: null }), {
        phase: "1",
        title: "Preparation",
        actor: "agent",
        skill: null,
    });
});

test("derives proposal work for a draft pull request without a proposal", () => {
    assertPhase(facts(), {
        phase: "2",
        title: "Proposal",
        actor: "agent",
        skill: ".agents/skills/writing-a-proposal/",
    });
});

for (let { where, state } of [
    {
        where: "on the default branch",
        state: facts({ branch: "main", isDefaultBranch: true, pullRequest: null }),
    },
    { where: "on a branch without a pull request", state: facts({ pullRequest: null }) },
    { where: "on a pull request without a proposal", state: facts() },
]) {
    test(`asks before opening a proposal workspace or drafting a proposal ${where}`, () => {
        let { actor, next } = deriveStatus(state);

        assert.equal(actor, "agent");
        assert.match(next, /ask the human whether they want a proposal/i);
        assert.match(next, /before (creating|pushing|writing)/i);
    });
}

test("keeps a draft proposal in proposal work", () => {
    assertPhase(facts({ proposal: proposal("draft") }), {
        phase: "2",
        title: "Proposal",
        actor: "you",
        skill: null,
    });
});

test("derives implementation for an awaiting proposal", () => {
    assertPhase(facts({ proposal: proposal("awaiting-implementation") }), {
        phase: "3",
        title: "Implementation",
        actor: "agent",
        skill: ".agents/skills/implementing-a-proposal/",
    });
});

test("derives human review for an active-review proposal", () => {
    assertPhase(facts({ proposal: proposal("active-review") }), {
        phase: "4",
        title: "Human review",
        actor: "you",
        skill: null,
    });
});

test("derives revisions for a returned proposal", () => {
    assertPhase(facts({ proposal: proposal("returned-for-revisions") }), {
        phase: "4 → 3",
        title: "Human review → Implementation",
        actor: "agent",
        skill: ".agents/skills/implementing-a-proposal/",
    });
});

test("derives completion for an accepted proposal", () => {
    assertPhase(facts({ proposal: proposal("accepted") }), {
        phase: "5",
        title: "Completion",
        actor: "agent",
        skill: ".agents/skills/completing-a-feature/",
    });
});

test("confirms the status record before readying an accepted draft PR", () => {
    let status = deriveStatus(facts({ proposal: proposal("accepted") }));

    assert.equal(status.phase, "5");
    assert.match(status.next, /record.*confirm.*pull request.*mark.*ready/i);
    assert.doesNotMatch(status.next, /perform.*merge/i);
});

test("checks a merged PR before reconciling a locally accepted proposal", () => {
    let input = facts({
        proposal: proposal("accepted"),
        pullRequest: { ...openPullRequest, isDraft: false, state: "MERGED" },
    });
    let status = assertPhase(input, {
        phase: "5",
        title: "Completion review",
        actor: "agent",
        skill: ".agents/skills/completing-a-feature/",
    });

    assert.match(status.next, /confirm.*merged.*if absent.*reconcil/i);
    assert.doesNotMatch(status.next, /perform.*merge|record.*in this feature pull request/i);
    assertContains(formatStatus(input, status), "PR #1 (merged)");
});

test("keeps merged implementation in completion while release remains separate", () => {
    let input = facts({
        proposal: proposal("implemented"),
        pullRequest: { ...openPullRequest, isDraft: false, state: "MERGED" },
    });
    let status = assertPhase(input, {
        phase: "5",
        title: "Implementation merged",
        actor: "agent",
        skill: ".agents/skills/completing-a-feature/",
    });

    assertContains(formatStatus(input, status), "PR #1 (merged)");
    assert.match(status.next, /required release/);
    assert.doesNotMatch(status.next, /perform.*merge/i);
});

test("does not treat an unpushed implementation record as merged", () => {
    let input = facts({
        unpushedCommits: 1,
        proposal: proposal("implemented"),
        pullRequest: { ...openPullRequest, isDraft: false, state: "MERGED" },
    });
    let status = deriveStatus(input);
    let report = formatStatus(input, status);

    assert.equal(status.phase, "5");
    assertContains(report, "Unpushed commits: 1");
    assert.match(report, /confirm.*implemented record.*merged/i);
});

test("does not claim integration when a PR merged into a non-default branch", () => {
    let input = facts({
        proposal: proposal("implemented"),
        pullRequest: {
            ...openPullRequest,
            isDraft: false,
            state: "MERGED",
            baseRefName: "staging",
        },
    });
    let status = deriveStatus(input);

    assert.equal(status.phase, "5");
    assert.equal(status.actor, "you");
    assert.match(status.next, /staging.*main/i);
    assert.notEqual(status.title, "Implementation merged");
});

test("does not claim integration when the target branch cannot be established", () => {
    let status = deriveStatus(
        facts({
            defaultBranch: null,
            proposal: proposal("implemented"),
            pullRequest: { ...openPullRequest, isDraft: false, state: "MERGED" },
        }),
    );

    assert.equal(status.phase, "5");
    assert.notEqual(status.title, "Implementation merged");
    assert.match(status.next, /default branch/i);
});

test("derives a closed state for a rejected proposal", () => {
    assertPhase(facts({ proposal: proposal("rejected") }), {
        phase: "done",
        title: "Closed",
        actor: "you",
        skill: null,
    });
});

test("derives a closed state for a withdrawn proposal", () => {
    assertPhase(facts({ proposal: proposal("withdrawn") }), {
        phase: "done",
        title: "Closed",
        actor: "you",
        skill: null,
    });
});

test("derives a closed state with its successor for a superseded proposal", () => {
    let input = facts({
        proposal: proposal("superseded"),
        proposalSuccessor: { id: "proposal.0002", title: "Replacement" },
    });
    let status = assertPhase(input, {
        phase: "done",
        title: "Closed",
        actor: "you",
        skill: null,
    });

    assert.match(formatStatus(input, status), /proposal\.0002/);
});

test("routes an implemented open PR to completion instead of declaring inconsistency", () => {
    let status = assertPhase(
        facts({
            proposal: proposal("implemented"),
            pullRequest: { ...openPullRequest, isDraft: false },
        }),
        {
            phase: "5",
            title: "Completion",
            actor: "agent",
            skill: ".agents/skills/completing-a-feature/",
        },
    );

    assert.deepEqual(status.notices, []);
    assert.match(status.next, /confirm.*implemented record.*pull request.*before.*merge/i);
});

test("confirms the status record before readying an implemented draft PR", () => {
    let status = deriveStatus(facts({ proposal: proposal("implemented") }));

    assert.equal(status.phase, "5");
    assert.match(status.next, /confirm.*implemented record.*pull request.*mark.*ready/i);
    assert.doesNotMatch(status.next, /perform.*merge/i);
});

for (let proposalStatus of ["accepted", "implemented"]) {
    test(`stops for a human decision when the ${proposalStatus} proposal's PR closes without merging`, () => {
        let input = facts({
            proposal: proposal(proposalStatus),
            pullRequest: { ...openPullRequest, isDraft: false, state: "CLOSED" },
        });
        let status = deriveStatus(input);

        assert.equal(status.phase, "5");
        assert.equal(status.actor, "you");
        assert.match(status.next, /reopen|withdraw/);
        assertContains(formatStatus(input, status), "PR #1 (closed)");
    });
}

test("flags a draft pull request when the proposal is in active review", () => {
    let status = deriveStatus(facts({ proposal: proposal("active-review") }));

    assert.deepEqual(status.notices, [{ kind: "draft-active-review" }]);
});

test("surfaces a heuristic proposal selection and its other candidates", () => {
    let input = facts({
        proposal: proposal("draft", 0, { pullRequest: "2" }),
        proposalSelection: {
            otherCandidates: [{ id: "proposal.0002", title: "Other feature" }],
            pullRequestMismatch: true,
        },
    });
    let status = deriveStatus(input);

    assert.deepEqual(status.notices, [
        {
            kind: "ambiguous-proposal-selection",
            selected: "proposal.0001",
            otherCandidates: [{ id: "proposal.0002", title: "Other feature" }],
            pullRequestMismatch: true,
            proposalPullRequest: "2",
            pullRequestNumber: 1,
        },
    ]);
    assert.match(formatStatus(input, status), /proposal\.0002/);
});

test("surfaces unresolved clarification markers and unpushed commits", () => {
    let status = deriveStatus(facts({ proposal: proposal("draft", 2), unpushedCommits: 3 }));

    assert.deepEqual(status.notices, [
        { kind: "clarifications", count: 2 },
        { kind: "unpushed-commits", count: 3 },
    ]);
});

test("reports an unavailable pull request state instead of assuming no pull request", () => {
    let status = assertPhase(
        facts({
            pullRequest: null,
            pullRequestState: "unknown",
            pullRequestReason: "gh is unavailable",
        }),
        {
            phase: "1",
            title: "Preparation",
            actor: "agent",
            skill: null,
        },
    );

    assert.equal(status.reason, "gh is unavailable");
});

test("reports an unauthenticated pull request state", () => {
    let status = assertPhase(
        facts({
            pullRequest: null,
            pullRequestState: "unknown",
            pullRequestReason: "gh is not authenticated",
        }),
        {
            phase: "1",
            title: "Preparation",
            actor: "agent",
            skill: null,
        },
    );

    assert.equal(status.reason, "gh is not authenticated");
});

test("reports an unknown branch role instead of recommending a pull request", () => {
    assertPhase(facts({ pullRequest: null, isDefaultBranch: null }), {
        phase: "unknown",
        title: "Branch role unknown",
        actor: "agent",
        skill: null,
    });
});

test("takes the default branch from origin/HEAD before anything else", () => {
    assert.deepEqual(defaultBranchFacts("feature", "refs/remotes/origin/trunk", "main", "main"), {
        defaultBranch: "trunk",
        isDefaultBranch: false,
    });
});

test("recognizes a local default branch when origin/HEAD and gh are unavailable", () => {
    assert.deepEqual(defaultBranchFacts("feature/x", null, null, "main"), {
        defaultBranch: "main",
        isDefaultBranch: false,
    });
    assert.deepEqual(defaultBranchFacts("main", null, null, "main"), {
        defaultBranch: "main",
        isDefaultBranch: true,
    });
});

test("gives a branch cut from a local main the Phase 1 advice, not an unknown role", () => {
    let { isDefaultBranch } = defaultBranchFacts("feature/x", null, null, "main");

    assertPhase(facts({ branch: "feature/x", isDefaultBranch, pullRequest: null }), {
        phase: "1",
        title: "Preparation",
        actor: "agent",
        skill: null,
    });
});

test("leaves the branch role unknown when no default branch can be found", () => {
    assert.deepEqual(defaultBranchFacts("feature/x", null, null, null), {
        defaultBranch: null,
        isDefaultBranch: null,
    });
});

test("reserves unknown routing for an invalid proposal status", () => {
    assertPhase(facts({ proposal: proposal("invalid") }), {
        phase: "2",
        title: "Proposal",
        actor: "you",
        skill: null,
    });
});
