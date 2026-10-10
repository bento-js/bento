---
name: completing-a-feature
description: Use after a pull request has been reviewed and the change is accepted — by the human, or by the strategist under a bounded build mandate — and needs integration, release, and cleanup. Do not use before acceptance.
---

# Completing a Feature

**Phase 5.** Complete work in this order: **process feedback → record policies and decisions → update the vision → merge → release → dependent-repo updates → cleanup**.

## 1. Process feedback

Read the human's PR comments and direct commits before acting. Classify each item:

| Feedback changes | Action |
| --- | --- |
| The implementation | Fix it, then rerun the affected implementation loop and review evidence. |
| Intended design | Update the proposal first — with the human, or under a mandate that delegates the revision inside its boundaries — then rerun tests → guides → code → quality gates → review against the revision. |

Never encode a design change in code, tests, or guides while leaving the proposal stale.

While revisions are outstanding, the proposal is `returned-for-revisions`, set by the approval owner. Return to Phase 2 for a design revision or Phase 3 for implementation. If reworking the design reopens a question, record a clarification marker; `returned-for-revisions` permits markers so honest rework remains possible. At the end of Phase 3, hand off to `reviewing-an-implementation`, which owns the coupled status, readiness-report, and pull-request transition. Do not set proposal status merely when reposting.

## 2. Record policies and decisions

After feedback establishes the feature's final shape, record only knowledge that outlives its proposal.

**The usual answer is that there is none.** Most features establish no policy and no decision, and recording nothing is the correct outcome rather than a step you skipped. Empty `policies/` and `decisions/` directories are healthy. A record written because the phase has a slot for it is one nobody follows and everybody has to read.

| Record   | Distinguishing test                                                              |
| -------- | -------------------------------------------------------------------------------- |
| Policy   | It constrains future work: “always or never do X.”                               |
| Decision | It explains a choice future work must understand: “we chose X over Y because Z.” |

If it does not outlive this proposal, it belongs in the proposal and nowhere else.

Before recording either:

1. Discuss it with the human. Neither is an agent-only conclusion, unless a mandate delegates in-scope records; even then it cannot change the mandate's own constraints or supersede a governing record outside the delegated authority.
2. Confirm the originating proposal id.
3. Use the canonical template. Both records carry `established-by: proposal.<NNNN>`.

### Enforce policies first

A policy that can be enforced by a check **must** be. Before writing prose, try to express it as a validator rule, continuous-integration step, or template section, per `.agents/rules/enforcement-hierarchy.md`.

Keep prose for judgment a deterministic check cannot decide. Prose policies are the ones that rot.

### Supersede; never rewrite history

Never edit a policy or decision into a different meaning. Never delete one.

1. Write a new record with `supersedes: [<old id>]`.
2. Set the old record's `status: superseded`.
3. Preserve the old rationale and link.

The validator enforces the pairing. A correction that does not change meaning may amend the existing record; a changed rule or rationale requires supersession.

## 3. Update the vision

Reread `VISION.md`. Update it when the accepted change alters the project's capabilities, MVP, or direction. Bump `updated` to the current date.

Consult the human before a substantial rewrite. Do not turn a narrow completion change into a new vision exercise.

## 4. Merge

Merge only after acceptance is recorded, and only into the stated merge target through the pull request. Never push directly to the default branch. The human accepts the work; where a bounded build mandate explicitly delegates acceptance and merge ([`AGENTS.md`](../../../AGENTS.md#bounded-build-mandates)), the strategist may accept and merge after clean independent reviews of the latest relevant revision and fresh passing checks and smoke of the integration candidate. Fix unresolved defects and unmet outcomes within scope, then repeat verification and review. Disputed material findings and changes beyond the mandate require the human; all of these block autonomous merge while unresolved.

Preserve the record of what was proposed, what was implemented, why decisions were made, what review found, and how the final state differs from earlier revisions.

Choose a merge strategy that retains that context in the PR and its history. Do not squash the work into one opaque commit that discards the discussion.

Set `implemented` only after the merge and the delivery the proposal or mandate requires, including a release where one is required and authorized — not before, because an unshipped proposal marked implemented misrepresents the record. Never mark an unperformed required release complete; merge-only work reaches `implemented` without publishing anything.

## 5. Release

When release is required and authorized, use the project's existing CI/CD process. Run `vp run` with no task to find the release path. Skip publication for approved merge-only delivery.

**Never publish without explicit human confirmation.** Approval to merge is not approval to release. A mandate that delegates merging delegates nothing about publication, deployment, credentials, access, or any other destructive or outward-facing act; each of those needs the human's explicit authority for that act.

## 6. Update dependent repositories

After an authorized release, update any dependent draft PRs consuming a preview version to the production release, within the granted repository authority. Mark them ready for review when appropriate. No dependent-release update is required for merge-only delivery.

## 7. Clean up

After the required delivery, perform authorized cleanup of temporary files, preview configuration, and feature branches.

Branch deletion, force-push, and published-artifact teardown require explicit human authorization for that action. A merge grant supplies none of that authority; ask when the specific action is not already authorized.

Cleanup runs when confidence is highest. Treat it as a separate destructive operation, not an automatic epilogue.

## Red flags

- You fixed a design-change comment without revising the proposal.
- You ignored a direct human commit.
- You recorded a policy or decision without the human and without a mandate that delegates in-scope records.
- You wrote a prose policy before checking whether automation can enforce it.
- You changed an old policy or decision's meaning in place.
- You merged without preserving why the final state differs.
- You released because the PR merged.
- A dependent PR still uses a preview version after release.
- A preview or prerelease artifact outlives its PR.
- You deleted a branch, force-pushed, or tore down a published prerelease artifact without explicit human confirmation.
- You merged with stale or failing checks, an unresolved defect, or a disputed review finding still open.
- You acted past a mandate's boundaries, or widened or renewed one yourself, instead of stopping and asking.
- You marked `implemented` while a required release had not happened.

Any of these means stop and return to the relevant ordered step.

## Related skills

- `reviewing-an-implementation` produces the preview and readiness record consumed during acceptance review.
