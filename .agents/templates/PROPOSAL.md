---
id: proposal.<NNNN>
title: <Title Case>
authors: [<name>]
# Set `draft` on creation. See the lifecycle note below for who decides each transition.
status: draft
pull-request: <url or number, empty while unopened>
issues: []
supersedes: []
---

# <Title>

<!--
Delete this instruction and replace every <placeholder>. A proposal is the durable
specification for one change; write for someone without the pull-request conversation.

Lifecycle:
- `draft` — Phase 2 proposal development; may carry clarification markers.
- `awaiting-implementation` — Phase 3 implementation may begin.
- `active-review` — Phase 4 acceptance review; readiness is posted and the pull request is ready.
- `returned-for-revisions` — Phase 4 review returned the work to Phase 2 or Phase 3.
- `accepted` — Phase 5 vision update, merge, and required delivery are underway.
- `implemented` — merged, with every delivery this proposal requires performed; done. A merge-only change needs no release; a required release that has not happened keeps the proposal in `accepted`.
- `rejected` — the approval owner declined the change; retained as a record.
- `withdrawn` — the author pulled it; retained as a record.
- `superseded` — a later proposal replaced it; retained as a record.
Normal path: `draft` → `awaiting-implementation` → `active-review` → `accepted` → `implemented`.
Who decides: the approval owner decides `awaiting-implementation`, `returned-for-revisions`, `accepted`, and `rejected` — the human, or the strategist for work an active build mandate delegates (AGENTS.md#bounded-build-mandates). At the end of Phase 3, `reviewing-an-implementation` alone moves the proposal to `active-review` while posting the readiness report and making the pull request ready. The agent moves `implemented`. Outside delegated authority, deciding is the part an agent must never do alone; under it, no agent accepts its own completion claim as the evidence. Permission to begin implementing is `awaiting-implementation`, never `accepted`.

-->

## Summary

<!--
Write no more than 50 words. The proposal list displays this out of context, so name
the change and its value plainly. It does not need to flow into the next section.
-->

<What changes and why it matters.>

## Authorization

<!--
Record where the authority for this change comes from, before the work proceeds.

Under a bounded build mandate: link the mandate proposal and the approved revision it rests on,
explain why this change falls inside its outcome, scope, and constraints, name the delegated
authority being used, and record the strategist's approval decisions as they are made.

Otherwise: record the human's direct consent for this change — when it was given and what it
covered. See AGENTS.md#bounded-build-mandates.
-->

- Authority: <mandate proposal and approved revision, or direct human consent and date>
- Fit: <why this change is inside that authority>
- Decisions recorded: <approval decisions taken under it, or “None yet.”>

## Motivation

<!--
Describe the problem: who encounters it, its impact, evidence that it exists, and the
current workaround. Do not propose a solution; establish why the problem is worth solving.
-->

<Concrete problem, affected people, impact, and current workaround.>

## Domain grounding

<!--
Record the knowledge necessary to evaluate this proposal on the problem's own
terms.

Distinguish established facts from repository observations, inferences, and
remaining uncertainties. Link primary or authoritative sources for important
external claims.

This section explains the world the proposed solution must fit. It is not an
implementation plan.

If no independent domain research is needed, state why rather than filling the
subsections with generic claims. Use “None” for remaining uncertainty when resolved.
-->

### Established context

- ...

### Relevant constraints and principles

- ...

### Quality bar

- ...

### Remaining uncertainty

- ...

## Existing baseline

<!--
After grounding in the domain, inspect the closest existing implementation,
prototype, prior art, or workflow that already accomplishes the intended result.

Describe what it demonstrates, what should be preserved, and what appears to be
incidental, compromised, or incorrect. Existing behavior is evidence, not
authority. If there is no relevant baseline, state that explicitly.
-->

<The closest usable baseline and what can be learned from it.>

## Proposed solution

<!--
Make the persuasive case for the chosen direction. Use representative examples where
they clarify the pitch. This is an overview, not a complete specification.
-->
<Concise explanation of the approach and expected result.>

```text
<Representative action or input> → <observable outcome>
```

## Detailed design

<!--
Specify behavior, interfaces, data, failures, boundary cases, and operational constraints.
Someone who is not the author must be able to implement it without asking questions; tests derive from this section.
-->

- When <trigger or input>, <actor or system> <required behavior>.
- If <error or boundary condition>, <observable result>.
- <Data, configuration, migration, or operational requirement.>

## Acceptance criteria

<!--
Usually delete this section. Detailed design is what tests derive from, and restating it as a
checklist duplicates the specification in a form that drifts from it.

Keep it only when the change has discrete outcomes that Detailed design cannot express naturally
— a structural change touching many independent things, or a migration with separable steps.
If every criterion is a restatement of a sentence above, delete the section.
-->

- [ ] <Discrete, independently verifiable outcome.>

## Compatibility

<!--
State effects on existing users, callers, stored data, or configuration, including migration,
fallback, and whether a break is opt-in. If none, state “No compatibility impact.” explicitly: considering it is the point.
-->

<Compatibility impact, migration, and opt-in status — or “No compatibility impact.”>

## Implications on adoption

<!--
Explain new requirements, version floors, setup or migration steps, and whether adoption can be reversed.
If nothing changes, state “No implications on adoption.” explicitly rather than deleting this section.
-->

<Adoption requirements and reversibility — or “No implications on adoption.”>

## Scope

<!--
Name the work required to deliver this proposal. Keep it bounded; merely related work
belongs elsewhere unless this change requires it.
-->

- <In-scope deliverable or change.>
- <In-scope validation or migration.>

### Out of scope

<!--
Name deliberate exclusions and why. Defects discovered while writing this proposal that fall outside
scope are filed as GitHub issues, not folded in; use .agents/skills/tracking-defects/.
-->

- <Excluded work and reason it is excluded.>

## Build mandate

<!--
Usually delete this section. Keep it only when this proposal asks the human to delegate a bounded
set of decisions to the strategist; a mandate lives in an ordinary proposal rather than a record of
its own. The human must approve it, and it cannot authorize itself.

Every field below must be settled before approval: an unresolved boundary grants no authority.
See AGENTS.md#bounded-build-mandates.
-->

- Outcome: <outcome and the observable criteria that complete it>
- Scope: <what the mandate covers> — Non-goals: <what it never covers>
- Constraints: <fixed constraints, including trust boundaries and compatibility promises>
- Delegated: <decisions and actions, naming proposal approval, branch pushes, pull request creation, acceptance, and merge target>
- Limits: <resource and spending limits; zero new spend unless granted>
- Verification: <required checks, reviews, and preview before anything is accepted or merged>
- Expiry: <date, completion condition, or revocation trigger>
- Approved: <human approval date and where its evidence is retrievable>

## Preview

<!--
Choose the artifact from the preview table that exercises this change and explain why it is the cheapest
realistic one. The preview must let its reviewer exercise changed behavior rather than merely inspect prose or a screenshot; use .agents/skills/reviewing-an-implementation/. For a pure refactor with no exercisable surface, state why no preview applies.
-->

- Artifact: <preview artifact>
- Reason: <why this is the cheapest realistic exercise of the change>

## Policies and decisions checked

<!--
Read every relevant entry in policies/ and decisions/, then state how this proposal honors each.
“None” is permissible only after actually reading both directories; record conflicts rather than silently departing from them.
-->

- <policy-or-decision-id> — <how this proposal honors it>

## Future directions

<!--
Describe possibilities neutrally; avoid “we will” and “we should.” Ask whether each belongs in this proposal
now rather than using this section to defer necessary work. Remove the placeholder when no direction is useful.
-->

- <Neutral possible extension and why it is not part of this proposal.>

## Alternatives considered

<!--
Reviewers may already prefer another approach. Engage with credible alternatives raised during review; be fair but need not be neutral.
If an alternative wins, change the proposal, explain why the new idea won, and update this section as review continues.
-->

- <Alternative> — <why it was not chosen>

## Open questions

<!--

Put every unresolved question in the exact marker below. Resolve every marker before the approval
owner authorizes implementation; the validator enforces marker resolution. Do not guess an answer
merely to make the proposal appear complete.
-->

- [NEEDS CLARIFICATION: <question>]

## Acknowledgments

<!--
Optional. Credit people, prior work, or review that materially shaped this proposal.
Remove this section when no acknowledgment is useful.
-->
<Contribution or influence.>
