# Agentic Development Process

This document describes the process I use for agentic development (“vibe coding”).

The process is intended to be generalizable to any modern agentic coding harness, including [Claude Code](https://claude.com/product/claude-code), [Codex](https://openai.com/codex/), [Copilot](https://github.com/features/copilot), [Pi](https://pi.dev/), and [Oh My Pi](https://omp.sh/). Throughout this document, the harness and the model driving it are collectively referred to as _the agent_.

The process starts with understanding the problem. For substantive changes, the agent grounds the domain, makes intent explicit with the human, then represents it independently in tests, documentation, and code. The resulting behavior is reviewed from multiple perspectives and finally verified by a human before release.

## Goals

The process has several goals:

- Preserve a coherent long-term vision for the project even as individual features are developed independently.
- Make the intended behavior and scope of a feature explicit before implementation begins.
- Maintain multiple independently useful representations of that intent in proposals, tests, guides, and code.
- Make disagreements between those representations visible before a feature is released.
- Delegate product and technical decisions within human-approved boundaries, with independent evidence required before acceptance.
- Preserve enough design rationale, implementation history, and review discussion for future humans and agents to understand why the project is the way it is.
- Keep dependent projects synchronized as new functionality moves from development previews to production releases.

## Artifacts

The process produces and maintains several kinds of artifacts.

### Visions

A _vision_ describes the project as a whole, including its purpose, minimum viable product target, design principles, and a possible future for the project beyond the current implementation.

The vision establishes the long-term direction against which individual proposals can be evaluated. It is created once near the beginning of the project and maintained as the project evolves.

### Policies

A _policy_ records a rule that should continue to be enforced across future work.

Policies capture constraints or conventions that should not have to be rediscovered from previous implementations or review discussions.

### Decisions

A _decision_ records an architectural or technical choice that should be remembered, including enough context to understand why that choice was made.

Future work may supersede a decision, but should do so explicitly rather than accidentally.

### Proposals

A _proposal_ describes a specific change to the project in enough detail to serve as the specification for its implementation.

Proposals record the motivation, intended behavior, scope, and relevant design decisions of a feature. They remain part of the project's history after implementation. A future proposal may revise or supersede an earlier proposal.

### Tests

_Tests_ are executable specifications of the behavior described by a proposal.

They may include unit tests, integration tests, end-to-end tests, or other forms of automated verification implemented idiomatically for the implementation language and environment.

### Guides

_Guides_ are human-consumable Markdown documentation explaining how the proposed or implemented feature behaves and how it is used.

Guides provide a prose representation of the same behavior represented more formally by the proposal and tests.

### Code

_Code_ is the concrete implementation of the behavior described by the proposal, tests, and guides.

### Previews

A _preview_ is a prerelease form of the documentation or running software made privately available for human review.

A preview should be the cheapest realistic artifact through which a human can exercise the changed behavior. Both qualities matter: _cheapest_ rules out standing up production-grade infrastructure merely to review a change, and _realistic_ rules out screenshots, transcripts, and descriptions of what would happen.

What that artifact is depends on what the project is:

| Project             | Preview                                                |
| ------------------- | ------------------------------------------------------ |
| Library             | preview package deployment                             |
| Web application     | preview app deployment                                 |
| API                 | preview endpoint deployment                            |
| Documentation       | preview documentation deployment                       |
| Command-line tool   | installable executable                                 |
| Desktop application | installable build                                      |
| Mobile application  | installable build, via ad-hoc or internal distribution |

A project wires the mechanism matching its own shape. Previews are private by default; a project makes one public only deliberately, as when a library publishes its documentation.

## Project Vision

A new project begins with the creation of its vision document.

The agent and I develop the vision together before normal feature work begins. The document describes both the minimum viable version of the project and the broader direction in which the project may evolve.

The vision is not a collection of committed feature specifications. Individual changes are designed through proposals, with approval owned by the human or delegated through a bounded build mandate.

Unlike proposals, however, the vision is a living document. After accepted features materially alter the project's capabilities or direction, the vision is updated so that it continues to describe the project accurately.

## Feature Development

Substantial feature work proceeds through five phases:

1. preparation,
2. proposal development,
3. implementation,
4. acceptance review, and
5. completion and release.

The process is iterative. Findings during implementation or review may cause the proposal itself to change, at which point the implementation is reevaluated against the revised proposal.

The full feature loop applies to changes that decide new behavior or resolve a meaningful design choice. A correction whose intent is already established, such as an unambiguous bug fix or dead link, may be handled directly. If investigation uncovers a design choice, return to proposal development before implementing it.

The primary agent is the strategist: it owns domain understanding, conceptual modeling, proposal intent, delegation, and the excellence pass. Implementers realize bounded, already-understood behavior without redefining the proposal. A reframer challenges the problem formulation when it no longer explains the evidence. An independent adversary tries to falsify the finished work and reports findings without editing it.

Before entering the proposal path, establish authority. An active human-approved build mandate can authorize the strategist to create and approve in-scope proposals without a new question for each feature. Otherwise the agent asks whether the human wants a proposal and receives an explicit yes before creating its branch or draft PR. A no rules out the proposal and its draft PR; correct unambiguous behavior directly or clarify consequential choices before implementation.

### Bounded build mandates

A mandate gives the strategist authority to deliver one agreed outcome without repeated human approval. Its canonical requirements are in [the agent contract](AGENTS.md#bounded-build-mandates). It lives in an ordinary proposal and records the outcome, boundaries, delegated actions, resource limits, evidence requirements, end condition, and human approval of a specific revision.

The human approves the mandate itself. Governed proposals link that approved revision in an Authorization section. Within the delegated scope, the strategist may settle design choices, approve proposals for implementation, create policies and decisions, and accept independently reviewed work. An explicit merge grant permits gated PR integration into the named branch even while the human is unavailable.

Tests, guides, implementation, fresh validation, independent review, and preview still apply. Acceptance records who exercised the delegated authority and the evidence supporting it. The implementer's own success report does not satisfy independent review, and unresolved defects or disputed material findings block autonomous acceptance.

Only the human changes the mandate's boundaries or extends its authority. A boundary conflict blocks the affected slice and is recorded in Beads and the PR; unrelated authorized work continues. Expiry, completion, revocation, missing approval evidence, or unresolved boundaries restore the individual human-approval path. Approval to introduce this mechanism does not activate a build mandate.

Publication, production deployment, credential or access changes, and destructive operations need their own explicit human authority. Merge permission alone grants none of them. The mandate can define a merge-only delivery that completes without publication; a required release cannot be silently dropped.

### Agent orchestration

The project's Oh My Pi `workflow` tool, registered by `.omp/extensions/dynamic-workflows.ts`, is for a dependency graph of agent work: independent branches that feed a later synthesis, or result-driven branches and pipelines. A single small task stays inline; a fixed batch of independent slices uses ordinary task delegation. A run goes to the background and reports back as a `workflow-result` message, so the strategist keeps working while it runs, addresses an agent by its label-derived id with `write agent://<id>`, and uses `workflow_runs` to inspect a run, block on it with `action: "wait"`, or cancel it. Workflow scripts orchestrate subagents deterministically and expose phases, but the agents still need bounded ownership and the strategist still owns intent and review. A failed branch yields `null`; treat it as a missing result, not evidence of success.

## Preparation

After direct human consent or an applicable mandate authorizes the proposal, work begins in an isolated Git branch.

The agent creates the branch, pushes it to the project's `origin` remote, and creates a draft pull request for the work.

The pull request becomes the durable workspace for the feature. It collects the proposal, implementation, review discussion, agent status reports, and other artifacts associated with the change.

The pull request remains a draft while the feature is being specified and implemented.

## Proposal Development

Before implementation, the agent writes a proposal describing the feature under the applicable approval path.

The proposal should make the intended change sufficiently concrete that the agent can derive tests, documentation, and an implementation from it without relying primarily on the original conversational request.

The approval owner settles the intent: the human for individual work, or the strategist for decisions delegated by a mandate. The proposal remains a historical record after the feature ships.

If future work changes that design, a later proposal may revise or supersede it.

### Ground the problem

When a change depends on domain knowledge that cannot safely be inferred from the request, the strategist uses `grounding-a-problem` before selecting an approach. Prefer authoritative sources; distinguish established knowledge, observed repository behavior, inference, and untested hypotheses. Record the context needed to judge the proposal in **Domain grounding**. When no independent research is needed, say why in that section rather than manufacturing it.

When important domain concepts, identities, relationships, states, or vocabulary change, use `ontology-review` before those assumptions harden into the detailed design. A material conceptual conflict goes back to grounding or the proposal, not directly into code.

### Study the existing baseline

After grounding, inspect the closest implementation, prototype, workflow, or prior attempt. Record what works, what should be preserved, and what is incidental or compromised in **Existing baseline**. Existing code is evidence of current behavior, not authority about the domain.

### Decide and record intent

Compare viable approaches against the grounded constraints, vision, policies, and decisions. Write the selected behavior in the proposal's **Detailed design**, with scope and unresolved questions explicit. The approval owner records the decision and sets `awaiting-implementation`; an implementer cannot authorize its own work, and a strategist cannot manufacture mandate authority.

## Implementation

After the approval owner sets `awaiting-implementation`, the strategist delegates bounded implementation slices with their detailed design, domain constraints, baseline, quality bar, governing records, authorization, and verification commands. If an implementer reports `NEEDS_STRATEGY`, stop that slice and return the conflict to the strategist. Use `reframing-a-problem` when repeated local fixes no longer explain the problem. The strategist may revise in-mandate intent before resuming implementation; changing mandate boundaries requires the human.

### Behavioral Tests

The agent first writes failing tests for the observable behavior the proposal specifies when such tests add meaningful protection.

These tests establish an executable interpretation of the proposal before the implementation itself can influence what is considered correct.

The goal is not merely to increase test coverage, but to encode the externally meaningful behavior of the feature.

### Usage Guides

When the change has a user-facing surface, the agent then writes its guide from the proposal before production code exists. This forces the intended interface to be explainable independently from the implementation.

The agent compares the guide with the tests before implementing. A change with no user-facing surface needs no invented guide.

### Implementation

The agent implements the feature against the proposal, tests, and guides.

Implementation continues until the project's quality gates pass. Depending on the project, these may include:

- formatting,
- linting,
- type checking,
- building,
- unit tests,
- integration tests,
- end-to-end tests, and
- other project-specific validation.

Passing the quality gates is necessary but not sufficient for the feature to be considered complete.

### Excellence Pass and Inline Reviews

After behavior works and validation passes, the strategist uses `excellence-pass` to ask whether the implementation should remain: does it express the grounded model, reuse the right boundaries, and present a clear interface without inherited or speculative complexity? Substantive corrections return to implementation, then validation and this pass run again. A finding that changes intended behavior returns to the proposal and approval owner before code changes.

The strategist then obtains a proposal-compliance review and a code-quality review, in that order. Both must be clean before `reviewing-an-implementation` begins the cross-artifact review. These inline reviews do not replace the independent adversarial review.

### Cross-Artifact Review

Once the implementation passes its automated checks, the agent reviews the proposal, tests, guides, and code together.

The purpose of this review is to find disagreement between the different representations of the feature.

In particular, the agent verifies that:

- the tests cover the behavior promised by the proposal;
- the guides describe the same behavior exercised by the tests;
- the code implements the behavior described by the proposal and guides;
- no substantial documented behavior is missing from the implementation;
- no substantial implemented behavior exists without being represented in the proposal or documentation; and
- implementation details have not unintentionally changed the original scope or intent.

### Adversarial Review

The implementation is then reviewed independently by a subagent using a different model from the primary agent.

The reviewing agent receives the proposal, tests, guides, and implementation and attempts to find weaknesses rather than continue implementation.

Its review considers issues such as:

- weak, misleading, or incomplete tests;
- implementation defects;
- security vulnerabilities;
- regressions;
- scope creep;
- undocumented behavior;
- inconsistencies between artifacts;
- violations of policies or decisions;
- unnecessary complexity;
- code smell;
- fragile architecture; and
- assumptions that have not been adequately validated.

The reviewing agent reports its findings to the primary agent.

The primary agent addresses legitimate findings and preserves declined findings with evidence and reasoning in the readiness report. Disputed material findings block autonomous acceptance and return to the human.

The adversarial reviewer reports evidence rather than deciding acceptance. The approval owner judges the finished work against its intent after considering that independent attempt to falsify it.

### Preview

Produce the preview selected in the proposal after review findings are resolved. It must be a usable build, installation, or deployed surface through which the human can exercise the changed behavior. Confirm that it runs; a green preview build alone is insufficient. A pure refactor without an exercisable surface needs no fabricated preview.

### Readiness Report

After the cross-artifact and adversarial reviews and preview, the agent posts a readiness report describing implemented behavior, verification, any declined findings, and how to exercise the preview. `reviewing-an-implementation` owns this handoff. Moving the proposal to `active-review`, posting the report, and marking the pull request ready form one transition into acceptance review. An applicable mandate can delegate the next decision to the strategist without a wait for the human.

## Acceptance Review

Acceptance determines whether the implementation satisfies the intended feature. Under a mandate, the strategist checks its current authority, reviews the independent evidence, and records `accepted` or `returned-for-revisions`. Delegated merge additionally requires fresh verification of the integration candidate. Without that authority, the human follows the review process below.

I review both the agent's readiness report and the artifacts produced during development. Depending on the feature, this may include:

- the code diff;
- the proposal;
- tests;
- rendered preview documentation;
- a preview application deployment; and
- a preview package consumed by draft pull requests in dependent projects.

Review is not limited to reading generated artifacts. The preview forms of the feature are intended to make the actual behavior directly inspectable whenever possible.

I leave general or inline pull request comments describing changes I want the agent to make. I may also edit and commit prose or code directly during review.

I then instruct the agent to process the review feedback. Additional context may be supplied directly to the agent when it does not belong in a pull request comment.

If the feedback changes the intended design rather than merely correcting its implementation, the agent updates the proposal first.

The agent then repeats the affected tests, guides, code, validation, excellence pass, cross-artifact review, and adversarial review against the revised proposal as necessary.

Implementation and acceptance review therefore form a loop:

**proposal → tests → guides → code → validation → excellence pass → inline reviews → cross-artifact review → adversarial review → preview → acceptance review → revision**

The loop continues until the approval owner accepts the feature. A human may intervene at any point, including when acceptance was delegated.

## Policies and Decisions

Feature development may reveal information that should outlive the proposal itself.

When a feature establishes a rule that future work should continue to enforce, record or update a policy with the approval owner's authorization.

When it establishes an architectural choice that future work needs to understand, record or update a decision under the same authority. Delegated policy and decision work cannot change the mandate's own constraints or silently supersede governing records beyond its authority.

Policies and decisions are therefore not substitutes for proposals. A proposal explains a particular change; policies and decisions extract the durable constraints and architectural knowledge discovered while making that change.

Future proposals should be evaluated against the current vision, policies, and decisions.

## Defect Tracking

During proposal development, implementation, automated validation, adversarial review, or human review, the agent may discover defects that should be preserved independently of the current pull request.

A defect that prevents the current implementation from satisfying its proposal remains part of the current work and should normally be fixed before the pull request is accepted. The agent should not create an issue merely to defer work required by the proposal.

A defect may instead be recorded as a GitHub issue when it is outside the scope of the current proposal, already exists on the main branch, affects another feature or project, or otherwise represents legitimate future work that should survive beyond the current pull request.

Before creating a new issue, the agent checks the repository's existing open and closed issues for the same defect to avoid creating duplicates.

When a new issue is necessary, the agent creates it using the [`gh`](https://cli.github.com/) CLI. The issue should contain enough information for a future human or agent to understand and reproduce the defect without depending on the conversational context in which it was discovered.

At minimum, a defect should record:

- a concise description of the incorrect behavior;
- the behavior that was expected instead;
- reproduction steps or a minimal reproduction when available;
- relevant environmental or version information;
- links or references to the proposal, pull request, test, or code where the defect was discovered when useful; and
- any known constraints, suspected causes, or related defects that would help a future investigation.

For example:

```sh
gh issue create \
  --title "Parser rejects nested optional routes" \
  --label bug \
  --body-file - <<'EOF'
## Description

The route parser rejects nested optional route segments that should be valid.

## Expected behavior

`($lang).($region).products.tsx` should produce two nested optional route segments.

## Actual behavior

Route discovery fails with an invalid segment error.

## Reproduction

1. Create `app/routes/($lang).($region).products.tsx`.
2. Run the route discovery command.
3. Observe the invalid segment error.

## Context

Discovered while implementing #123.

The current proposal does not alter optional-route parsing, so this defect is outside its scope.
EOF
```

The agent applies the repository's normal issue labels, types, milestones, projects, parent issues, or dependency relationships when those conventions exist.

When a newly discovered defect is relevant to the current pull request but intentionally outside its scope, the agent links the resulting issue from the pull request or an appropriate review comment. This makes the decision to defer the defect explicit and leaves a durable path from the context in which it was discovered to the future work needed to resolve it.

If subsequent work fixes the defect incidentally, the agent updates or closes the issue with a reference to the pull request or commit that resolved it rather than leaving the issue stale.

## Completion

Once the feature has passed acceptance review, the agent performs the authorized integration and delivery work.

### Vision Update

The agent reviews the project vision and updates it when the completed feature changes the project's current capabilities, minimum viable product, or broader direction.

Because the vision is intended to describe the project as it currently exists and where it is heading, accepted work should not leave it stale.

The agent may consult me before making substantial changes to the vision.

### Merge

The agent merges the pull request into the authorized target branch after direct human approval or under an active mandate's explicit merge grant. One integration owner serializes merges and verifies the latest integration candidate. Direct pushes to the default branch remain prohibited.

The merge should preserve enough information for future humans or agents to recover:

- what was proposed;
- what was implemented;
- why important decisions were made;
- what problems were discovered during review;
- what changes were requested; and
- how the final implementation differs from earlier revisions.

The proposal and relevant pull request history therefore form part of the long-term knowledge base of the project rather than disposable development scaffolding.

### Release

When the proposal requires a release and the human has explicitly authorized that action, release through the project's CI/CD process using the required mechanism, such as a Git tag, package publication, or deployment. Merge approval alone does not authorize publication. Set `implemented` only after the merge and all required delivery are complete; a proposal whose approved delivery is merge-only needs no fabricated release.

### Dependent Work

If other projects contain draft pull requests depending on a preview version of the feature, the agent updates those pull requests to consume the production release.

Once their dependency on unreleased work has been removed, the agent marks those dependent pull requests as ready for review when appropriate.

This allows work across multiple repositories to be developed against previews without leaving those preview dependencies in place after release.

### Cleanup

Finally, the agent removes development state that is no longer useful.

Cleanup can include pruning feature branches, removing obsolete preview configuration, and removing temporary artifacts created during development. Branch deletion, published-artifact teardown, and other destructive actions require explicit human authorization for the action; a merge grant does not cover them.

Cleanup concludes the feature process.

## Process Summary

At the project level:

**vision → feature proposals → implemented project → updated vision**

For each feature:

**human consent or active mandate → branch → draft pull request → grounding and ontology review when needed → baseline study → approved proposal → tests → guides → code → validation → excellence pass → inline reviews → cross-artifact review → adversarial review → preview → ready pull request → acceptance review → revision or acceptance → vision update → authorized merge and delivery → dependent updates → authorized cleanup**

The central invariant of the process is that no single artifact defines the feature by itself.

The proposal records intent. Tests encode behavior. Guides explain behavior. Code implements behavior. Previews expose behavior for direct inspection. Independent agent review attempts to find contradictions and defects. The approval owner determines whether the result satisfies the intended feature within the human's authority.

Each representation acts as a check on the others.
