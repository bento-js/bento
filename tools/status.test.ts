import { expect, test } from "vite-plus/test";

import { deriveStatus, formatStatus, type Facts, type Proposal, type Step } from "./status.ts";

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

test("confirms the accepted record before readying a draft PR", () => {
    let status = deriveStatus(facts({ proposal: proposal("accepted") }));

    expect(status.phase).toBe("5");
    expect(status.next).toMatch(/record.*confirm.*pull request.*mark.*ready/i);
});

test("inspects the merged revision before reconciling a locally accepted proposal", () => {
    let input = facts({
        proposal: proposal("accepted"),
        pullRequest: { ...openPullRequest, isDraft: false, state: "MERGED" },
    });
    let status = deriveStatus(input);

    expect(status.phase).toBe("5");
    expect(status.next).toMatch(/confirm.*merged.*if absent.*reconcil/i);
    expect(status.next).not.toMatch(/perform.*merge|record.*in this feature pull request/i);
    expect(formatStatus(input, status)).toContain("PR #1 (merged)");
});

test("keeps a merged implementation in completion while required delivery remains separate", () => {
    let input = facts({
        proposal: proposal("implemented"),
        pullRequest: { ...openPullRequest, isDraft: false, state: "MERGED" },
    });
    let status = assertPhase(input, {
        phase: "5",
        actor: "agent",
        skill: ".agents/skills/completing-a-feature/",
    });

    expect(status.next).toMatch(/required release|required delivery/i);
    expect(formatStatus(input, status)).toContain("PR #1 (merged)");
});

test("does not treat a default-branch merge as proof it was the mandated target", () => {
    let status = deriveStatus(
        facts({
            proposal: proposal("implemented"),
            pullRequest: { ...openPullRequest, isDraft: false, state: "MERGED" },
        }),
    );

    expect(status.phase).toBe("5");
    expect(status.next).toMatch(/confirm.*authorized.*target/i);
    expect(status.next).not.toMatch(/^Complete the required delivery/i);
});

test("inspects an accepted record even when its PR merged into an authorized staging branch", () => {
    let status = deriveStatus(
        facts({
            proposal: proposal("accepted"),
            pullRequest: {
                ...openPullRequest,
                isDraft: false,
                state: "MERGED",
                baseRefName: "proposal/weekend-remix-guest-book",
            },
        }),
    );

    expect(status.phase).toBe("5");
    expect(status.next).toMatch(/inspect|confirm.*merged.*record/i);
    expect(status.next).toMatch(/authorized target/i);
});

test("checks an open PR's merge target before recommending integration", () => {
    let status = deriveStatus(
        facts({
            proposal: proposal("implemented"),
            pullRequest: { ...openPullRequest, isDraft: false, baseRefName: "main" },
        }),
    );

    expect(status.next).toMatch(/confirm.*authorized.*target/i);
});

test("asks whether staging was the authorized merge target", () => {
    let status = deriveStatus(
        facts({
            proposal: proposal("implemented"),
            pullRequest: {
                ...openPullRequest,
                isDraft: false,
                state: "MERGED",
                baseRefName: "staging",
            },
        }),
    );

    expect(status.phase).toBe("5");
    expect(status.actor).toBe("approval owner");
    expect(status.next).toMatch(/staging.*authorized target/i);
});

test("does not assume a merged PR reached the authorized target when the default is unknown", () => {
    let status = deriveStatus(
        facts({
            defaultBranch: null,
            proposal: proposal("implemented"),
            pullRequest: { ...openPullRequest, isDraft: false, state: "MERGED" },
        }),
    );

    expect(status.phase).toBe("5");
    expect(status.next).toMatch(/authorized target/i);
    expect(status.title).not.toBe("Implementation merged");
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

test("routes an implemented draft PR to completion instead of treating it as inconsistent", () => {
    let status = assertPhase(facts({ proposal: proposal("implemented") }), {
        phase: "5",
        actor: "agent",
        skill: ".agents/skills/completing-a-feature/",
    });

    expect(status.notices).toStrictEqual([]);
    expect(status.next).toMatch(/confirm.*implemented record.*pull request.*mark.*ready/i);
});

test("requires record verification before merging an implemented open PR", () => {
    let status = deriveStatus(
        facts({
            proposal: proposal("implemented"),
            pullRequest: { ...openPullRequest, isDraft: false },
        }),
    );

    expect(status.phase).toBe("5");
    expect(status.next).toMatch(/confirm.*implemented record.*pull request.*before.*merge/i);
});

test.each(["accepted", "implemented"])(
    "stops for the approval owner's decision when a %s PR closes without merging",
    proposalStatus => {
        let input = facts({
            proposal: proposal(proposalStatus),
            pullRequest: { ...openPullRequest, isDraft: false, state: "CLOSED" },
        });
        let status = deriveStatus(input);

        expect(status.phase).toBe("5");
        expect(status.actor).toBe("approval owner");
        expect(status.next).toMatch(/reopen|withdraw/);
        expect(formatStatus(input, status)).toContain("PR #1 (closed)");
    },
);

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
