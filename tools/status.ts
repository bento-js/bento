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

export interface PullRequest {
    number: number;
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
    isDefaultBranch: boolean | null;
    hasUpstream: boolean;
    unpushedCommits: number;
    // `isMerged` is null when the default branch is unknown, so merge state cannot be checked.
    pullRequest: (PullRequest & { isMerged: boolean | null }) | null;
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
    | { kind: "implemented-unmerged" }
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
const CONVENTIONAL_DEFAULT_BRANCHES = new Set(["main", "master"]);
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
    accepted: step(
        "5",
        "Completion",
        "The feature has been accepted and awaits completion.",
        "Update the vision, perform the authorized merge, and complete the delivery this proposal requires. Release only where it is required and separately authorized, then clean up.",
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
    implementedUnmerged: step(
        "3 → 4",
        "Inconsistent record",
        "The proposal is marked implemented, but its pull request is not merged.",
        "Resolve the record inconsistency before continuing.",
        "you",
    ),
    done: step(
        "done",
        "Done",
        "The merged proposal is recorded as implemented.",
        "Nothing outstanding. Start the next feature when ready.",
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
            "Restore access to gh, then check the pull request state.",
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

function defaultBranchFacts(
    branch: string | null,
    defaultReference: string | null,
    ghDefaultBranch: string | null,
): { defaultReference: string | null; isDefaultBranch: boolean | null } {
    let defaultBranch =
        defaultReference?.split("/").at(-1) ??
        ghDefaultBranch ??
        (branch !== null && CONVENTIONAL_DEFAULT_BRANCHES.has(branch) ? branch : null);
    return {
        defaultReference: defaultReference ?? (defaultBranch ? `origin/${defaultBranch}` : null),
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

function collectPullRequest(root: string): PullRequestFacts {
    if (commandOutput("gh", ["--version"], root) === null) {
        return { pullRequest: null, state: "unknown", reason: "gh is unavailable" };
    }
    if (!commandSucceeds("gh", ["auth", "status"], root)) {
        return { pullRequest: null, state: "unknown", reason: "gh is not authenticated" };
    }
    let source = commandOutput("gh", ["pr", "view", "--json", "number,isDraft,url,title"], root);
    if (source === null) return { pullRequest: null, state: "known", reason: null };
    try {
        let pullRequest = JSON.parse(source) as PullRequest;
        return Number.isInteger(pullRequest.number)
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

function isMergedIntoDefault(root: string, defaultReference: string | null): boolean | null {
    if (!defaultReference) return null;
    return commandSucceeds(
        "git",
        ["merge-base", "--is-ancestor", "HEAD", defaultReference.replace(/^refs\/remotes\//, "")],
        root,
    );
}

export function gatherFacts(root = process.cwd()): Facts {
    let branch = collectBranch(root);
    let upstream = collectUpstream(root);
    let pullRequestFacts = collectPullRequest(root);
    let proposals = collectProposals(root);
    let selection = selectProposal(proposals, pullRequestFacts.pullRequest);
    let proposalSuccessor =
        selection.proposal?.status === "superseded"
            ? successorFor(proposals, selection.proposal)
            : null;
    return {
        branch: branch.branch,
        isDefaultBranch: branch.isDefaultBranch,
        hasUpstream: upstream.hasUpstream,
        unpushedCommits: upstream.unpushedCommits,
        pullRequest: pullRequestFacts.pullRequest && {
            ...pullRequestFacts.pullRequest,
            isMerged: isMergedIntoDefault(root, branch.defaultReference),
        },
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
    if (proposal?.status === "implemented" && pullRequest?.isMerged === false)
        notices.push({ kind: "implemented-unmerged" });
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
    if (status === "implemented")
        return result(facts, facts.pullRequest.isMerged ? STEPS.done : STEPS.implementedUnmerged);
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
    if (notice.kind === "implemented-unmerged")
        return "Warning: proposal is implemented, but the pull request is not merged.";
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
        ? `PR #${facts.pullRequest.number} (${facts.pullRequest.isDraft ? "draft" : "open"})  ${facts.pullRequest.url}`
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
