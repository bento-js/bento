import { execFileSync } from "node:child_process";
import { readdirSync, readFileSync } from "node:fs";
import path from "node:path";

import { parseFrontmatter, withoutCode, type FrontmatterValue } from "./validate.ts";

// Status cannot establish whether approval belongs to the human or a delegated strategist.
type Actor = "agent" | "you" | "approval owner";

export interface Step {
    phase: string;
    title: string;
    situation: string;
    next: string;
    actor: Actor;
    skill: string | null;
}

export type PullRequestState = "OPEN" | "MERGED" | "CLOSED";

export interface PullRequest {
    number: number;
    isDraft: boolean;
    state: PullRequestState;
    baseRefName: string;
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
    isDefaultBranch: boolean | null;
    defaultBranch: string | null;
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

interface Repository {
    nameWithOwner: string | null;
    defaultBranch: string | null;
}

interface ListedPullRequest {
    pullRequest: PullRequest;
    headRepository: string;
    headRefOid: string;
}

const NON_TERMINAL_STATUSES = new Set([
    "draft",
    "awaiting-implementation",
    "active-review",
    "returned-for-revisions",
    "accepted",
]);
const CONVENTIONAL_DEFAULT_BRANCHES = new Set(["main", "master"]);
const ORIGIN_HEAD_PREFIX = "refs/remotes/origin/";
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
        "If considering a proposal, check whether an active build mandate covers this change: under one, record that authority and open the workspace; otherwise ask the human before creating a branch or draft pull request. Handle an unambiguous correction directly.",
        "agent",
    ),
    noPullRequest: step(
        "1",
        "Preparation",
        "This branch has no pull request.",
        "If considering a proposal, settle the authority first — an active build mandate covering this change, or the human's answer — then push the branch and open a draft pull request.",
        "agent",
    ),
    noProposal: step(
        "2",
        "Proposal",
        "The pull request has no proposal.",
        "Settle the authority — an active build mandate covering this change, or the human's consent unless they already answered for it — then draft and publish the proposal, recording that authority in its Authorization section.",
        "agent",
        SKILLS.proposal,
    ),
    draft: step(
        "2",
        "Proposal",
        "The proposal is published and being iterated on.",
        "Iterate the proposal in the pull request. The approval owner moves it to awaiting-implementation once the design is settled and no clarification marker remains.",
        "approval owner",
    ),
    "awaiting-implementation": step(
        "3",
        "Implementation",
        "The proposal is approved for implementation.",
        "Write failing tests, then guides, then code and gates. Complete cross-artifact and adversarial reviews, post the readiness report, and mark the pull request ready.",
        "agent",
        SKILLS.implementation,
    ),
    "active-review": step(
        "4",
        "Acceptance review",
        "A first full pass at tests, guides, and code is up for review.",
        "Review the diff, the preview, and the independent findings, then return the work for revisions or accept it. The strategist may accept only within an active mandate's delegated acceptance and its evidence requirements.",
        "approval owner",
    ),
    "returned-for-revisions": step(
        "4 → 3",
        "Acceptance review → Implementation",
        "The pull request was returned for revisions.",
        "Process the feedback; update the proposal first if the design changed.",
        "agent",
        SKILLS.implementation,
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
    unknown: step(
        "2",
        "Proposal",
        "The proposal status is not recognized.",
        "Update the proposal status in the pull request.",
        "you",
    ),
};
// Proposal statuses index STEPS directly; `accepted` and `implemented` route on the pull request
// instead, and a status with no step of its own falls back to unknown.
const STEPS_BY_STATUS: Record<string, Step | undefined> = STEPS;

const COMPLETION = {
    acceptedDraft: step(
        "5",
        "Completion",
        "The feature has been accepted and awaits its implementation record.",
        "Update the vision where completed work changes it, record `implemented` in this feature pull request, and confirm both that revision reached the pull request and the authorized merge target recorded in the proposal or mandate, before you mark it ready.",
        "agent",
        SKILLS.completion,
    ),
    acceptedReady: step(
        "5",
        "Completion",
        "The feature has been accepted and awaits its implementation record.",
        "Update the vision where completed work changes it, record `implemented` in this feature pull request, and confirm both that revision reached the pull request and the authorized merge target recorded in the proposal or mandate, before the merge. Any required delivery stays separately authorized.",
        "agent",
        SKILLS.completion,
    ),
    implementedDraft: step(
        "5",
        "Completion",
        "The implementation is recorded and the pull request is still a draft.",
        "Confirm the implemented record is in the pull request revision rather than only your checkout and the authorized merge target recorded in the proposal or mandate, then mark the pull request ready for review ahead of that merge.",
        "agent",
        SKILLS.completion,
    ),
    implementedReady: step(
        "5",
        "Completion",
        "The implementation is recorded and the pull request is open for integration.",
        "Confirm the implemented record is in the pull request revision rather than only your checkout, confirm the authorized merge target recorded in the proposal or mandate, and check the integration candidate, before the merge.",
        "agent",
        SKILLS.completion,
    ),
    merged: step(
        "5",
        "Implementation merged",
        "The accepted implementation merged into the default branch.",
        "Confirm the implemented record is in the merged revision; if absent, ask the approval owner how to reconcile it without a status-only pull request. Confirm the authorized merge target recorded in the proposal or mandate — reaching the default branch is not that authority. Once both are confirmed, complete the required delivery under its separate authorization, update dependent projects, then perform the authorized cleanup. A merge-only change needs no release.",
        "agent",
        SKILLS.completion,
    ),
    mergedWhileAccepted: step(
        "5",
        "Merged revision review",
        "The pull request merged while the local proposal still reads accepted.",
        "Confirm whether the implemented record is present in the merged revision; if absent, ask the approval owner how to reconcile it. Never amend a merged pull request or open a status-only pull request for the record.",
        "agent",
        SKILLS.completion,
    ),
};

function unknownPullRequestStep(reason: string): Step & { reason: string } {
    return {
        ...step(
            "1",
            "Preparation",
            `The pull request state is unknown: ${reason}.`,
            "Resolve the pull request state — restore gh access, or identify which pull request records this branch — before acting on it.",
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

// A mandate may authorize a target other than the default branch, so an unexpected base is a
// question for the approval owner rather than a mistake status can diagnose.
function mergedOffDefaultStep(
    status: "accepted" | "implemented",
    baseRefName: string,
    defaultBranch: string | null,
): Step {
    let situation =
        defaultBranch === null
            ? `The pull request merged into ${baseRefName}, and the default branch cannot be determined.`
            : `The pull request merged into ${baseRefName}, not the default branch ${defaultBranch}.`;
    if (status === "accepted")
        return step(
            "5",
            "Merged revision review",
            `${situation} The local proposal still reads accepted.`,
            `Inspect the merged revision for the implemented record, and ask the approval owner whether ${baseRefName} is the authorized target for this work — an active mandate may name one other than the default branch. If the record is absent, ask how to reconcile it; never amend a merged pull request or open a status-only pull request for the record.`,
            "agent",
            SKILLS.completion,
        );
    return step(
        "5",
        "Merge target review",
        situation,
        `Confirm the implemented record is in the merged revision; if absent, ask the approval owner how to reconcile it without a status-only pull request. Ask whether ${baseRefName} is the authorized target for this work — an active mandate may name one other than the default branch — then continue completion once both are confirmed.`,
        "approval owner",
    );
}

function completionStep(
    status: "accepted" | "implemented",
    pullRequest: PullRequest,
    defaultBranch: string | null,
): Step {
    let { baseRefName } = pullRequest;
    if (pullRequest.state === "CLOSED")
        return step(
            "5",
            "Pull request closed",
            `The pull request was closed without merging while the proposal reads ${status}.`,
            "Ask the approval owner whether to reopen the pull request and finish the authorized merge, or to withdraw the proposal.",
            "approval owner",
        );
    if (pullRequest.state === "MERGED") {
        if (defaultBranch !== null && baseRefName === defaultBranch)
            return status === "accepted" ? COMPLETION.mergedWhileAccepted : COMPLETION.merged;
        return mergedOffDefaultStep(status, baseRefName, defaultBranch);
    }
    if (status === "accepted")
        return pullRequest.isDraft ? COMPLETION.acceptedDraft : COMPLETION.acceptedReady;
    return pullRequest.isDraft ? COMPLETION.implementedDraft : COMPLETION.implementedReady;
}

function defaultBranchFacts(
    branch: string | null,
    defaultReference: string | null,
    ghDefaultBranch: string | null,
): { defaultBranch: string | null; isDefaultBranch: boolean | null } {
    let remoteDefault =
        defaultReference !== null && defaultReference.startsWith(ORIGIN_HEAD_PREFIX)
            ? defaultReference.slice(ORIGIN_HEAD_PREFIX.length) || null
            : null;
    // origin/HEAD may stay stale after a default-branch change; only GitHub's branch establishes
    // a merge target. Local hints only settle the Preparation branch role.
    let role =
        ghDefaultBranch ??
        remoteDefault ??
        (branch !== null && CONVENTIONAL_DEFAULT_BRANCHES.has(branch) ? branch : null);
    return {
        defaultBranch: ghDefaultBranch,
        isDefaultBranch: role === null ? null : branch === role,
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

function nonEmpty(value: unknown): string | null {
    return typeof value === "string" && value !== "" ? value : null;
}

// GitHub holds both identities this report depends on: the default branch, which origin/HEAD can
// report stale, and the repository name a pull request record has to match to be this work.
function collectRepository(root: string): Repository {
    let unidentified: Repository = { nameWithOwner: null, defaultBranch: null };
    let source = commandOutput(
        "gh",
        ["repo", "view", "--json", "defaultBranchRef,nameWithOwner"],
        root,
    );
    if (source === null) return unidentified;
    let view: unknown;
    try {
        view = JSON.parse(source);
    } catch {
        return unidentified;
    }
    if (typeof view !== "object" || view === null) return unidentified;
    let { defaultBranchRef, nameWithOwner } = view as Record<string, unknown>;
    let name =
        typeof defaultBranchRef === "object" && defaultBranchRef !== null
            ? (defaultBranchRef as Record<string, unknown>).name
            : undefined;
    return { nameWithOwner: nonEmpty(nameWithOwner), defaultBranch: nonEmpty(name) };
}

function collectBranch(root: string, ghDefaultBranch: string | null) {
    let branch = commandOutput("git", ["rev-parse", "--abbrev-ref", "HEAD"], root);
    let defaultReference = commandOutput("git", ["symbolic-ref", "refs/remotes/origin/HEAD"], root);
    return {
        branch,
        ...defaultBranchFacts(branch, defaultReference, ghDefaultBranch),
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

function parsePullRequest(value: unknown): PullRequest | null {
    if (typeof value !== "object" || value === null) return null;
    let { number, isDraft, state, baseRefName, url, title } = value as Record<string, unknown>;
    if (typeof number !== "number" || !Number.isInteger(number)) return null;
    if (typeof isDraft !== "boolean") return null;
    if (state !== "OPEN" && state !== "MERGED" && state !== "CLOSED") return null;
    if (typeof baseRefName !== "string" || baseRefName === "") return null;
    if (typeof url !== "string" || url === "") return null;
    if (typeof title !== "string" || title === "") return null;
    return { number, isDraft, state, baseRefName, url, title };
}

function unknownPullRequest(reason: string): PullRequestFacts {
    return { pullRequest: null, state: "unknown", reason };
}

// `gh pr list --head` matches a branch name alone, which also hits a fork's branch of the same
// name and a record whose branch was later reused; the head repository and revision are what tie
// a record to this checkout.
function parseListedPullRequest(value: unknown): ListedPullRequest | null {
    let pullRequest = parsePullRequest(value);
    if (!pullRequest) return null;
    let { headRefOid, headRepository } = value as Record<string, unknown>;
    let headRevision = nonEmpty(headRefOid);
    let nameWithOwner =
        typeof headRepository === "object" && headRepository !== null
            ? nonEmpty((headRepository as Record<string, unknown>).nameWithOwner)
            : null;
    if (headRevision === null || nameWithOwner === null) return null;
    return { pullRequest, headRepository: nameWithOwner, headRefOid: headRevision };
}

function identifyPullRequest(
    root: string,
    listed: ListedPullRequest,
    repository: string | null,
): PullRequestFacts {
    let { pullRequest, headRepository, headRefOid } = listed;
    if (repository === null) return unknownPullRequest("gh could not identify this repository");
    if (headRepository !== repository)
        return unknownPullRequest(
            `#${pullRequest.number} belongs to ${headRepository}, not ${repository}`,
        );
    // An open pull request may legitimately trail this checkout by unpushed commits. A merged or
    // closed one is history: a head revision other than this one means the name was reused.
    if (pullRequest.state === "OPEN") return { pullRequest, state: "known", reason: null };
    let head = commandOutput("git", ["rev-parse", "HEAD"], root);
    if (head === null) return unknownPullRequest("the checked-out revision is unavailable");
    if (head !== headRefOid)
        return unknownPullRequest(
            `#${pullRequest.number} records revision ${headRefOid.slice(0, 7)}, not the checked-out ${head.slice(0, 7)}`,
        );
    return { pullRequest, state: "known", reason: null };
}

function collectPullRequest(
    root: string,
    branch: string | null,
    repository: string | null,
): PullRequestFacts {
    if (commandOutput("gh", ["--version"], root) === null)
        return unknownPullRequest("gh is unavailable");
    if (!commandSucceeds("gh", ["auth", "status"], root))
        return unknownPullRequest("gh is not authenticated");
    if (branch === null) return unknownPullRequest("the current branch is unavailable");
    let fields = "number,isDraft,url,title,state,baseRefName,headRefOid,headRepository";
    let source = commandOutput(
        "gh",
        ["pr", "list", "--state", "all", "--head", branch, "--limit", "2", "--json", fields],
        root,
    );
    if (source === null)
        return unknownPullRequest(`gh could not list the pull requests for ${branch}`);
    let listed: unknown;
    try {
        listed = JSON.parse(source);
    } catch {
        return unknownPullRequest("gh returned invalid pull request data");
    }
    if (!Array.isArray(listed)) return unknownPullRequest("gh returned invalid pull request data");
    if (listed.length === 0) return { pullRequest: null, state: "known", reason: null };
    if (listed.length > 1)
        return unknownPullRequest(`Multiple pull requests found for branch ${branch}`);
    let candidate = parseListedPullRequest(listed[0]);
    if (!candidate) return unknownPullRequest("gh returned invalid pull request data");
    return identifyPullRequest(root, candidate, repository);
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
    let repository = collectRepository(root);
    let branch = collectBranch(root, repository.defaultBranch);
    let upstream = collectUpstream(root);
    let pullRequestFacts = collectPullRequest(root, branch.branch, repository.nameWithOwner);
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
    if (status === "accepted" || status === "implemented")
        return result(facts, completionStep(status, facts.pullRequest, facts.defaultBranch));
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

function pullRequestLabel({ state, isDraft }: PullRequest): string {
    if (state === "MERGED") return "merged";
    if (state === "CLOSED") return "closed";
    return isDraft ? "draft" : "open";
}

export function formatStatus(facts: Facts, derived: DerivedStatus = deriveStatus(facts)): string {
    let proposal = facts.proposal
        ? `${facts.proposal.id}  ${facts.proposal.title}  ·  ${facts.proposal.status}`
        : "proposal — none";
    let pullRequest = facts.pullRequest
        ? `PR #${facts.pullRequest.number} (${pullRequestLabel(facts.pullRequest)})  ${facts.pullRequest.url}`
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
