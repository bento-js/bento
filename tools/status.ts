import { execFileSync } from "node:child_process";
import { readdirSync, readFileSync } from "node:fs";
import path from "node:path";

import { parseFrontmatter, withoutCode, type FrontmatterValue } from "./validate.ts";

type Actor = "agent" | "you";

export interface Step {
    phase: string;
    title: string;
    situation: string;
    next: string;
    actor: Actor;
    skill: string | null;
}

export interface PullRequest {
    number: number;
    state: "OPEN" | "CLOSED" | "MERGED";
    baseRefName: string;
    isDraft: boolean;
    url: string;
    title: string;
}

export interface Proposal {
    id?: string;
    title?: string;
    status?: string;
    pullRequest?: string;
    supersedes?: string[];
    clarificationCount: number;
}

type ProposalSummary = Pick<Proposal, "id" | "title">;

export interface Facts {
    branch: string | null;
    defaultBranch: string | null;
    isDefaultBranch: boolean | null;
    hasUpstream: boolean;
    unpushedCommits: number;
    pullRequest: PullRequest | null;
    pullRequestState: "known" | "unknown";
    pullRequestReason?: string | null;
    proposal: Proposal | null;
    proposalSuccessor?: ProposalSummary | null;
    proposalSelection?: {
        otherCandidates: ProposalSummary[];
        pullRequestMismatch: boolean;
    };
}

type Notice =
    | { kind: "clarifications"; count: number }
    | { kind: "unpushed-commits"; count: number }
    | { kind: "draft-active-review" }
    | {
          kind: "ambiguous-proposal-selection";
          selected: string | undefined;
          otherCandidates: ProposalSummary[];
          pullRequestMismatch: boolean;
          proposalPullRequest: string | undefined;
          pullRequestNumber: number | undefined;
      };

export type DerivedStatus = Step & { notices: Notice[]; reason?: string };

interface PullRequestFacts {
    pullRequest: PullRequest | null;
    state: Facts["pullRequestState"];
    reason: string | null;
}

const NON_TERMINAL_STATUSES = new Set([
    "draft",
    "awaiting-implementation",
    "active-review",
    "returned-for-revisions",
    "accepted",
]);
// Ordered: `main` wins when a repository somehow carries both.
const CONVENTIONAL_DEFAULT_BRANCHES: readonly string[] = ["main", "master"];
const SKILLS = {
    proposal: ".agents/skills/writing-a-proposal/",
    implementation: ".agents/skills/implementing-a-proposal/",
    completion: ".agents/skills/completing-a-feature/",
};

function step(
    phase: string,
    title: string,
    situation: string,
    next: string,
    actor: Actor,
    skill: string | null = null,
): Step {
    return { phase, title, situation, next, actor, skill };
}

const STEPS = {
    defaultBranch: step(
        "1",
        "Preparation",
        "You are on the default branch.",
        "If considering a proposal, ask the human whether they want a proposal for this change before creating a branch or draft pull request; otherwise handle the correction directly.",
        "agent",
    ),
    noPullRequest: step(
        "1",
        "Preparation",
        "This branch has no pull request.",
        "If considering a proposal, ask the human whether they want a proposal for this change before pushing the branch or opening a draft pull request.",
        "agent",
    ),
    noProposal: step(
        "2",
        "Proposal",
        "The pull request has no proposal.",
        "Ask the human whether they want a proposal for this change before writing one, unless they already answered for this change. If yes, draft and publish it.",
        "agent",
        SKILLS.proposal,
    ),
    draft: step(
        "2",
        "Proposal",
        "The proposal is published and being iterated on.",
        "Edit the proposal directly, or tell the agent what to change. Move it to awaiting-implementation when you are satisfied.",
        "you",
    ),
    "awaiting-implementation": step(
        "3",
        "Implementation",
        "The proposal is approved for implementation.",
        "Write failing tests, then guides, then code, and pass the gates. Self-check against the proposal, get one independent review, then post the readiness report and mark the pull request ready.",
        "agent",
        SKILLS.implementation,
    ),
    "active-review": step(
        "4",
        "Human review",
        "A first full pass at tests, guides, and code is up for review.",
        "Review the diff and the preview. Comment on the pull request, or accept the work.",
        "you",
    ),
    "returned-for-revisions": step(
        "4 → 3",
        "Human review → Implementation",
        "The human returned the pull request for revisions.",
        "Process the feedback; update the proposal first if the design changed.",
        "agent",
        SKILLS.implementation,
    ),
    accepted: step(
        "5",
        "Completion",
        "The feature has been accepted and awaits completion.",
        "Update the vision, record the accepted implementation in this feature pull request, verify that revision, then merge and complete any separately authorized required delivery; clean up afterward.",
        "agent",
        SKILLS.completion,
    ),
    acceptedDraft: step(
        "5",
        "Completion",
        "The feature has been accepted, but its pull request remains draft.",
        "Update the vision, record the accepted implementation and confirm its status in the feature pull request, then mark the PR ready. Verify the latest integration candidate before requesting a merge.",
        "agent",
        SKILLS.completion,
    ),
    acceptedAfterMerge: step(
        "5",
        "Completion review",
        "The feature pull request merged, but the local proposal still says accepted; the merged record has not been checked.",
        "Confirm whether the implemented record is in the merged pull request; if absent, ask the human how to reconcile without a status-only PR. Track any required delivery separately; do not try to edit the merged PR.",
        "agent",
        SKILLS.completion,
    ),
    rejected: step(
        "done",
        "Closed",
        "The work is closed: the proposal was rejected.",
        "Nothing outstanding.",
        "you",
    ),
    withdrawn: step(
        "done",
        "Closed",
        "The work is closed: the proposal was withdrawn.",
        "Nothing outstanding.",
        "you",
    ),
    superseded: step(
        "done",
        "Closed",
        "The work is closed: the proposal was superseded.",
        "Nothing outstanding.",
        "you",
    ),
    draftImplementation: step(
        "5",
        "Completion",
        "The local proposal is implemented, but the feature pull request remains draft.",
        "Confirm the implemented record is in the feature pull request, then mark it ready for review and verify the latest integration candidate before seeking a merge.",
        "agent",
        SKILLS.completion,
    ),
    readyToMerge: step(
        "5",
        "Completion",
        "The local proposal says implemented; its status in the feature pull request has not been verified.",
        "Confirm the implemented record is in this feature pull request before the merge. Verify the latest integration candidate, then merge and complete any separately authorized delivery; clean up afterward.",
        "agent",
        SKILLS.completion,
    ),
    merged: step(
        "5",
        "Implementation merged",
        "The feature pull request is merged; the local implemented status does not prove its record reached the target branch.",
        "Confirm the implemented record is in the merged pull request; if missing, report the inconsistency rather than opening a status-only PR. Track any required release until authorized and delivered, then perform authorized cleanup.",
        "agent",
        SKILLS.completion,
    ),
    closedWithoutMerge: step(
        "5",
        "Completion blocked",
        "The feature pull request closed without merging.",
        "Ask the human whether to reopen the pull request or withdraw the proposal; do not attempt to merge a closed pull request.",
        "you",
    ),
    unknown: step(
        "2",
        "Proposal",
        "The proposal status is not recognized.",
        "Update the proposal status in the pull request.",
        "you",
    ),
};
// Proposal statuses index STEPS directly; a status with no step of its own falls back to unknown.
const STEPS_BY_STATUS: Record<string, Step | undefined> = STEPS;

function unknownPullRequestStep(reason: string): Step & { reason: string } {
    return {
        ...step(
            "1",
            "Preparation",
            `The pull request state is unknown because ${reason}.`,
            "Check gh access and the branch's pull request association before continuing.",
            "agent",
        ),
        reason,
    };
}

function unknownBranchRoleStep(): Step {
    return step(
        "unknown",
        "Branch role unknown",
        "The default branch cannot be determined, so this branch's role is unknown.",
        "Determine the default branch before taking pull request actions.",
        "agent",
    );
}

function supersededStep(successor: ProposalSummary | null | undefined): Step {
    if (!successor) return STEPS.superseded;
    return step(
        "done",
        "Closed",
        `The work is closed. Its successor is ${successor.id} — ${successor.title}.`,
        "Nothing outstanding. Continue with the successor if further work is needed.",
        "you",
    );
}

/**
 * Which branch this repository treats as its default, and whether HEAD is on it.
 *
 * The sources are tried in descending order of authority: `origin/HEAD`, then
 * `gh repo view`, then a conventional branch that exists locally, then the name of
 * the current branch. The last two keep a repository that has no remote yet — a
 * fresh `git init -b main` — from reading as "role unknown": the local probe covers
 * a feature branch cut from `main`, and the current-branch case covers `main`
 * itself before its first commit creates `refs/heads/main`.
 */
export function defaultBranchFacts(
    branch: string | null,
    defaultReference: string | null,
    ghDefaultBranch: string | null,
    localDefaultBranch: string | null,
): { defaultBranch: string | null; isDefaultBranch: boolean | null } {
    let defaultBranch =
        defaultReference?.split("/").at(-1) ??
        ghDefaultBranch ??
        localDefaultBranch ??
        (branch !== null && CONVENTIONAL_DEFAULT_BRANCHES.includes(branch) ? branch : null);
    return {
        defaultBranch,
        isDefaultBranch: defaultBranch ? branch === defaultBranch : null,
    };
}

function commandOutput(command: string, arguments_: string[], root: string): string | null {
    try {
        return execFileSync(command, arguments_, {
            cwd: root,
            encoding: "utf8",
            stdio: ["ignore", "pipe", "ignore"],
        }).trim();
    } catch {
        return null;
    }
}

function commandSucceeds(command: string, arguments_: string[], root: string): boolean {
    try {
        execFileSync(command, arguments_, { cwd: root, stdio: "ignore" });
        return true;
    } catch {
        return false;
    }
}

/** A conventional default branch that exists in this repository, `main` first. */
function localDefaultBranch(root: string): string | null {
    return (
        CONVENTIONAL_DEFAULT_BRANCHES.find(candidate =>
            commandSucceeds("git", ["rev-parse", "--verify", `refs/heads/${candidate}`], root),
        ) ?? null
    );
}

function collectBranch(root: string) {
    let branch = commandOutput("git", ["rev-parse", "--abbrev-ref", "HEAD"], root);
    let defaultReference = commandOutput("git", ["symbolic-ref", "refs/remotes/origin/HEAD"], root);
    let ghDefaultBranch = defaultReference
        ? null
        : commandOutput(
              "gh",
              ["repo", "view", "--json", "defaultBranchRef", "--jq", ".defaultBranchRef.name"],
              root,
          );
    let localDefault =
        defaultReference === null && ghDefaultBranch === null ? localDefaultBranch(root) : null;
    return {
        branch,
        ...defaultBranchFacts(branch, defaultReference, ghDefaultBranch, localDefault),
    };
}

function collectUpstream(root: string): { hasUpstream: boolean; unpushedCommits: number } {
    let upstream = commandOutput(
        "git",
        ["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{upstream}"],
        root,
    );
    let count = upstream
        ? commandOutput("git", ["rev-list", "--count", "@{upstream}..HEAD"], root)
        : null;
    return {
        hasUpstream: upstream !== null,
        unpushedCommits: Number.isSafeInteger(Number(count)) ? Number(count) : 0,
    };
}

function collectPullRequest(root: string, branch: string | null): PullRequestFacts {
    if (commandOutput("gh", ["--version"], root) === null) {
        return { pullRequest: null, state: "unknown", reason: "gh is unavailable" };
    }
    if (!commandSucceeds("gh", ["auth", "status"], root)) {
        return { pullRequest: null, state: "unknown", reason: "gh is not authenticated" };
    }
    if (!branch) return { pullRequest: null, state: "unknown", reason: "branch is unknown" };
    let source = commandOutput(
        "gh",
        [
            "pr",
            "list",
            "--state",
            "all",
            "--head",
            branch,
            "--limit",
            "2",
            "--json",
            "number,isDraft,url,title,state,baseRefName",
        ],
        root,
    );
    if (source === null)
        return { pullRequest: null, state: "unknown", reason: "gh could not list pull requests" };
    try {
        let pullRequests = JSON.parse(source) as PullRequest[];
        if (!Array.isArray(pullRequests)) throw new Error("Invalid pull request list");
        if (pullRequests.length > 1)
            return {
                pullRequest: null,
                state: "unknown",
                reason: `Multiple pull requests found for branch ${branch}; inspect their proposal links`,
            };
        let pullRequest = pullRequests[0];
        if (!pullRequest) return { pullRequest: null, state: "known", reason: null };
        return Number.isInteger(pullRequest.number) &&
            typeof pullRequest.baseRefName === "string" &&
            (pullRequest.state === "OPEN" ||
                pullRequest.state === "CLOSED" ||
                pullRequest.state === "MERGED")
            ? { pullRequest, state: "known", reason: null }
            : {
                  pullRequest: null,
                  state: "unknown",
                  reason: "gh returned invalid pull request data",
              };
    } catch {
        return {
            pullRequest: null,
            state: "unknown",
            reason: "gh returned invalid pull request data",
        };
    }
}

// Status reporting reads possibly malformed records; a field with the wrong shape reads as absent.
function scalar(value: FrontmatterValue | undefined): string | undefined {
    return typeof value === "string" ? value : undefined;
}

function readProposal(root: string, name: string): Proposal | null {
    try {
        let source = readFileSync(path.join(root, "proposals", name), "utf8");
        let { frontmatter } = parseFrontmatter(source);
        if (!frontmatter) return null;
        return {
            id: scalar(frontmatter.id),
            title: scalar(frontmatter.title),
            status: scalar(frontmatter.status),
            pullRequest: scalar(frontmatter["pull-request"]),
            supersedes: Array.isArray(frontmatter.supersedes) ? frontmatter.supersedes : undefined,
            clarificationCount: (withoutCode(source).match(/\[NEEDS CLARIFICATION:/g) ?? []).length,
        };
    } catch {
        return null;
    }
}

function collectProposals(root: string): Proposal[] {
    try {
        return readdirSync(path.join(root, "proposals"), { withFileTypes: true })
            .filter(
                entry => entry.isFile() && entry.name.endsWith(".md") && entry.name !== "README.md",
            )
            .map(entry => readProposal(root, entry.name))
            .filter(proposal => proposal !== null);
    } catch {
        return [];
    }
}

function proposalNumber(proposal: Proposal): number {
    return Number(proposal.id?.match(/(\d+)$/)?.[1] ?? -1);
}

function matchesPullRequest(proposal: Proposal, pullRequest: PullRequest | null): boolean {
    if (!pullRequest) return false;
    let number = String(pullRequest.number);
    return (
        proposal.pullRequest === number ||
        Boolean(proposal.pullRequest?.endsWith(`/pull/${number}`))
    );
}

function selectProposal(proposals: Proposal[], pullRequest: PullRequest | null) {
    let ordered = [...proposals].sort(
        (left, right) => proposalNumber(right) - proposalNumber(left),
    );
    let matched = ordered.find(proposal => matchesPullRequest(proposal, pullRequest));
    let nonTerminal = ordered.filter(
        ({ status }) => status !== undefined && NON_TERMINAL_STATUSES.has(status),
    );
    let proposal = matched ?? nonTerminal[0] ?? null;
    let otherCandidates = matched ? [] : nonTerminal.filter(candidate => candidate !== proposal);
    return {
        proposal,
        otherCandidates,
        pullRequestMismatch: Boolean(
            proposal &&
            pullRequest &&
            proposal.pullRequest &&
            !matchesPullRequest(proposal, pullRequest),
        ),
    };
}

function successorFor(proposals: Proposal[], proposal: Proposal): Proposal | undefined {
    let { id } = proposal;
    if (id === undefined) return undefined;
    return [...proposals]
        .sort((left, right) => proposalNumber(right) - proposalNumber(left))
        .find(({ supersedes }) => supersedes?.includes(id));
}

export function gatherFacts(root = process.cwd()): Facts {
    let branch = collectBranch(root);
    let upstream = collectUpstream(root);
    let pullRequestFacts = collectPullRequest(root, branch.branch);
    let proposals = collectProposals(root);
    let selection = selectProposal(proposals, pullRequestFacts.pullRequest);
    let proposalSuccessor =
        selection.proposal?.status === "superseded"
            ? successorFor(proposals, selection.proposal)
            : null;
    return {
        branch: branch.branch,
        isDefaultBranch: branch.isDefaultBranch,
        defaultBranch: branch.defaultBranch,
        hasUpstream: upstream.hasUpstream,
        unpushedCommits: upstream.unpushedCommits,
        pullRequest: pullRequestFacts.pullRequest,
        pullRequestState: pullRequestFacts.state,
        pullRequestReason: pullRequestFacts.reason,
        proposal: selection.proposal,
        proposalSuccessor,
        proposalSelection: {
            otherCandidates: selection.otherCandidates.map(({ id, title }) => ({ id, title })),
            pullRequestMismatch: selection.pullRequestMismatch,
        },
    };
}

function noticesFor(facts: Facts): Notice[] {
    let { proposal, pullRequest, proposalSelection } = facts;
    let notices: Notice[] = [];
    if (proposal && proposal.clarificationCount > 0)
        notices.push({ kind: "clarifications", count: proposal.clarificationCount });
    if (facts.unpushedCommits > 0)
        notices.push({ kind: "unpushed-commits", count: facts.unpushedCommits });
    if (proposal?.status === "active-review" && pullRequest?.isDraft)
        notices.push({ kind: "draft-active-review" });
    if (
        proposal &&
        proposalSelection &&
        (proposalSelection.otherCandidates.length > 0 || proposalSelection.pullRequestMismatch)
    ) {
        notices.push({
            kind: "ambiguous-proposal-selection",
            selected: proposal.id,
            otherCandidates: proposalSelection.otherCandidates,
            pullRequestMismatch: proposalSelection.pullRequestMismatch,
            proposalPullRequest: proposal.pullRequest,
            pullRequestNumber: pullRequest?.number,
        });
    }
    return notices;
}

function result(facts: Facts, status: Step): DerivedStatus {
    return { ...status, notices: noticesFor(facts) };
}

export function deriveStatus(facts: Facts): DerivedStatus {
    if (facts.isDefaultBranch) return result(facts, STEPS.defaultBranch);
    if (facts.isDefaultBranch === null) return result(facts, unknownBranchRoleStep());
    if (facts.pullRequestState === "unknown") {
        return result(
            facts,
            unknownPullRequestStep(facts.pullRequestReason ?? "gh could not determine it"),
        );
    }
    if (!facts.pullRequest) return result(facts, STEPS.noPullRequest);
    if (!facts.proposal) return result(facts, STEPS.noProposal);
    let { status } = facts.proposal;
    if (
        (status === "accepted" || status === "implemented") &&
        facts.pullRequest.state === "MERGED" &&
        (!facts.defaultBranch || facts.pullRequest.baseRefName !== facts.defaultBranch)
    ) {
        let destination = facts.defaultBranch;
        return result(
            facts,
            step(
                "5",
                "Integration incomplete",
                "The feature pull request is merged, but integration into the default branch is not established.",
                destination
                    ? `The pull request merged into ${facts.pullRequest.baseRefName}, not ${destination}. Ask the human how to integrate the accepted change into ${destination}; do not claim delivery.`
                    : "Confirm the default branch before deciding whether the pull request merged into it; do not claim delivery.",
                "you",
            ),
        );
    }
    if (status === "implemented") {
        let completionStep =
            facts.pullRequest.state === "MERGED"
                ? STEPS.merged
                : facts.pullRequest.state === "CLOSED"
                  ? STEPS.closedWithoutMerge
                  : facts.pullRequest.isDraft
                    ? STEPS.draftImplementation
                    : STEPS.readyToMerge;
        return result(facts, completionStep);
    }
    if (status === "accepted" && facts.pullRequest.state === "MERGED")
        return result(facts, STEPS.acceptedAfterMerge);
    if (status === "accepted" && facts.pullRequest.state === "CLOSED")
        return result(facts, STEPS.closedWithoutMerge);
    if (status === "accepted" && facts.pullRequest.isDraft)
        return result(facts, STEPS.acceptedDraft);
    if (status === "superseded") return result(facts, supersededStep(facts.proposalSuccessor));
    return result(
        facts,
        (status === undefined ? undefined : STEPS_BY_STATUS[status]) ?? STEPS.unknown,
    );
}

function noticeLine(notice: Notice): string {
    if (notice.kind === "clarifications") return `Clarifications: ${notice.count} unresolved`;
    if (notice.kind === "unpushed-commits") return `Unpushed commits: ${notice.count}`;
    if (notice.kind === "draft-active-review")
        return "Warning: proposal is active-review, but the pull request remains draft.";
    let candidates = notice.otherCandidates.map(({ id, title }) => `${id} (${title})`).join(", ");
    if (notice.pullRequestMismatch) {
        let mismatch = `selected ${notice.selected} references ${notice.proposalPullRequest}, not current PR #${notice.pullRequestNumber}`;
        return candidates
            ? `Warning: ${mismatch}. Other candidates: ${candidates}.`
            : `Warning: ${mismatch}.`;
    }
    return `Warning: heuristic selected ${notice.selected}. Other candidates: ${candidates}.`;
}

export function formatStatus(facts: Facts, derived: DerivedStatus = deriveStatus(facts)): string {
    let proposal = facts.proposal
        ? `${facts.proposal.id}  ${facts.proposal.title}  ·  ${facts.proposal.status}`
        : "proposal — none";
    let pullRequest = facts.pullRequest
        ? `PR #${facts.pullRequest.number} (${facts.pullRequest.isDraft && facts.pullRequest.state === "OPEN" ? "draft" : facts.pullRequest.state.toLowerCase()})  ${facts.pullRequest.url}`
        : facts.pullRequestState === "unknown"
          ? `PR — unknown (${facts.pullRequestReason ?? "gh could not determine the pull request"})`
          : "PR — none";
    return [
        proposal,
        pullRequest,
        `branch ${facts.branch ?? "unavailable"}`,
        "",
        `Phase ${derived.phase} — ${derived.title}`,
        `  ${derived.situation}`,
        "",
        `Next (${derived.actor}): ${derived.next}`,
        ...(derived.skill ? [`              Skill: ${derived.skill}`] : []),
        ...derived.notices.map(noticeLine),
    ].join("\n");
}

function main(): void {
    let facts = gatherFacts();
    let derived = deriveStatus(facts);
    console.log(
        process.argv.includes("--json") ? JSON.stringify(derived) : formatStatus(facts, derived),
    );
}

if (import.meta.main) main();
