import { expect, test } from "vite-plus/test";

import { deriveStatus, formatStatus, type Facts, type Proposal, type Step } from "./status.ts";

let openPullRequest = {
    number: 1,
    isDraft: true,
    url: "https://github.com/owner/repo/pull/1",
    title: "Feature",
    isMerged: false,
};

function facts(overrides: Partial<Facts> = {}): Facts {
    return {
        branch: "feature",
        isDefaultBranch: false,
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

function assertPhase(input: Facts, expected: Pick<Step, "phase" | "actor" | "skill">) {
    let status = deriveStatus(input);

    expect(status).toMatchObject(expected);
    return status;
}

test("derives preparation on the default branch", () => {
    assertPhase(facts({ branch: "main", isDefaultBranch: true, pullRequest: null }), {
        phase: "1",
        actor: "agent",
        skill: null,
    });
});

test("derives preparation for a branch without a pull request", () => {
    assertPhase(facts({ pullRequest: null }), {
        phase: "1",
        actor: "agent",
        skill: null,
    });
});

test("derives proposal work for a draft pull request without a proposal", () => {
    assertPhase(facts(), {
        phase: "2",
        actor: "agent",
        skill: ".agents/skills/writing-a-proposal/",
    });
});

test("keeps a draft proposal with its approval owner", () => {
    assertPhase(facts({ proposal: proposal("draft") }), {
        phase: "2",
        actor: "approval owner",
        skill: null,
    });
});

test("derives implementation for an awaiting proposal", () => {
    assertPhase(facts({ proposal: proposal("awaiting-implementation") }), {
        phase: "3",
        actor: "agent",
        skill: ".agents/skills/implementing-a-proposal/",
    });
});

test("derives acceptance review for an active-review proposal", () => {
    assertPhase(facts({ proposal: proposal("active-review") }), {
        phase: "4",
        actor: "approval owner",
        skill: null,
    });
});

test("derives revisions for a returned proposal", () => {
    assertPhase(facts({ proposal: proposal("returned-for-revisions") }), {
        phase: "4 → 3",
        actor: "agent",
        skill: ".agents/skills/implementing-a-proposal/",
    });
});

test("derives completion for an accepted proposal", () => {
    assertPhase(facts({ proposal: proposal("accepted") }), {
        phase: "5",
        actor: "agent",
        skill: ".agents/skills/completing-a-feature/",
    });
});

test("derives done for an implemented merged pull request", () => {
    assertPhase(
        facts({
            proposal: proposal("implemented"),
            pullRequest: { ...openPullRequest, isMerged: true },
        }),
        {
            phase: "done",
            actor: "you",
            skill: null,
        },
    );
});

test("derives a closed state for a rejected proposal", () => {
    assertPhase(facts({ proposal: proposal("rejected") }), {
        phase: "done",
        actor: "you",
        skill: null,
    });
});

test("derives a closed state for a withdrawn proposal", () => {
    assertPhase(facts({ proposal: proposal("withdrawn") }), {
        phase: "done",
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
        actor: "you",
        skill: null,
    });

    expect(formatStatus(input, status)).toMatch(/proposal\.0002/);
});

test("names an implemented proposal with an unmerged pull request as inconsistent", () => {
    let status = assertPhase(facts({ proposal: proposal("implemented") }), {
        phase: "3 → 4",
        actor: "you",
        skill: null,
    });

    expect(status.notices).toStrictEqual([{ kind: "implemented-unmerged" }]);
});

test("flags a draft pull request when the proposal is in active review", () => {
    let status = deriveStatus(facts({ proposal: proposal("active-review") }));

    expect(status.notices).toStrictEqual([{ kind: "draft-active-review" }]);
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

    expect(status.notices).toStrictEqual([
        {
            kind: "ambiguous-proposal-selection",
            selected: "proposal.0001",
            otherCandidates: [{ id: "proposal.0002", title: "Other feature" }],
            pullRequestMismatch: true,
            proposalPullRequest: "2",
            pullRequestNumber: 1,
        },
    ]);
    expect(formatStatus(input, status)).toMatch(/proposal\.0002/);
});

test("surfaces unresolved clarification markers and unpushed commits", () => {
    let status = deriveStatus(facts({ proposal: proposal("draft", 2), unpushedCommits: 3 }));

    expect(status.notices).toStrictEqual([
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
            actor: "agent",
            skill: null,
        },
    );

    expect(status.reason).toBe("gh is unavailable");
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
            actor: "agent",
            skill: null,
        },
    );

    expect(status.reason).toBe("gh is not authenticated");
});

test("reports an unknown branch role instead of recommending a pull request", () => {
    assertPhase(facts({ pullRequest: null, isDefaultBranch: null }), {
        phase: "unknown",
        actor: "agent",
        skill: null,
    });
});

test("reserves unknown routing for an invalid proposal status", () => {
    assertPhase(facts({ proposal: proposal("invalid") }), {
        phase: "2",
        actor: "you",
        skill: null,
    });
});
