---
name: completing-a-feature
description: Use after a human has reviewed a pull request and the accepted change needs integration, release, and cleanup. Do not use before human review.
---

# Completing a Feature

**Phase 5.** Complete work in this order: **process feedback → record policies and decisions → update the vision → record `implemented` in the feature PR → merge → required release → dependent-repo updates → cleanup**.

## 1. Process feedback

Read the human's PR comments and direct commits before acting. Classify each item:

| Feedback changes | Action |
| --- | --- |
| The implementation | Fix it, then rerun the affected implementation loop and review evidence. |
| Intended design | Update the proposal first with the human, then rerun tests → guides → code → quality gates → review against the revision. |

Never encode a design change in code, tests, or guides while leaving the proposal stale.

While revisions are outstanding, the proposal is `returned-for-revisions`, which the human sets when requesting changes. Return to Phase 2 for a design revision or Phase 3 for implementation. If reworking the design reopens a question, record a clarification marker — `returned-for-revisions` is exempt from the marker rule so honest rework remains possible. At the end of Phase 3, hand off to `reviewing-an-implementation`, which alone owns the coupled status, readiness-report, and pull-request transition. Do not set proposal status when reposting.

## 2. Record policies and decisions

After feedback establishes the feature's final shape, record only knowledge that outlives its proposal.

**The usual answer is that there is none.** Most features establish no policy and no decision, and recording nothing is the correct outcome rather than a step you skipped. Empty `policies/` and `decisions/` directories are healthy. A record written because the phase has a slot for it is one nobody follows and everybody has to read.

| Record   | Distinguishing test                                                              |
| -------- | -------------------------------------------------------------------------------- |
| Policy   | It constrains future work: “always or never do X.”                               |
| Decision | It explains a choice future work must understand: “we chose X over Y because Z.” |

If it does not outlive this proposal, it belongs in the proposal and nowhere else.

Before recording either:

1. Discuss it with the human. Neither is an agent-only conclusion.
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

## 4. Record the accepted implementation

Add the changeset if the work does not already carry one: `mise run changeset`, bump chosen deliberately, entry written for the person reading the changelog. A change with no user-visible effect still carries one, created with `--empty`. CI rejects a pull request without a changeset, so this is a gate, not a courtesy.

After human acceptance and the vision update, set the proposal to `implemented` in the same feature pull request that contains the implementation. This status records accepted implementation, not a merged pull request or completed release. Record acceptance and any outstanding delivery in the proposal or PR, then commit and verify the updated integration candidate before merging. Never open a follow-up PR solely to move this status.

Confirm the record is present in the pull request revision before marking a draft PR ready or merging; a local file or unpushed commit is not proof. For an already merged pull request, inspect its merged revision before concluding a local `accepted` status means the record is missing. If absent, ask the human how to reconcile it and keep any required delivery visible. Do not try to amend a merged pull request or open a status-only PR.

## 5. Merge

Merge only after human acceptance and fresh checks on the pull request revision that contains the implementation record. Preserve what was proposed, implemented, and learned in review, including why the final state differs from earlier revisions.

If the accepted pull request is still draft, mark it ready and verify the updated revision before merging. Draft status never authorizes skipping readiness or review.

Choose a merge strategy that retains the proposal and PR discussion. Do not squash the work into one opaque commit that discards that context.

## 6. Release

When release is required and separately authorized, use the project's existing CI/CD process. Merging accumulates changesets on the default branch, and the release workflow opens a release pull request that runs `mise run version`: `changeset version` applies the pending changesets, bumping the version in the root `package.json` and writing `CHANGELOG.md`. Merging that release pull request is what publishes. Run `mise tasks` when the setup is unknown. Skip publication for approved merge-only delivery.

**Never publish without explicit human confirmation.** Approval to merge is not approval to release.

The implementation record does not complete a required release. Keep that obligation visible in the proposal or PR until the authorized release actually happens; do not call the feature delivered before then.

## 7. Update dependent repositories

After an authorized release, update any dependent draft PRs consuming a preview version to the production release, then mark them ready for review when appropriate. No dependent-release update is needed for merge-only delivery.

## 8. Clean up

After the required delivery, remove temporary files and preview configuration and prune the feature branch only with the required authorization.

> **Explicit human confirmation required:** Branch deletion, force-push, and tearing down published prerelease artifacts are the most destructive operations in this process. Ask before any of them, even when cleanup appears routine.

Cleanup runs when confidence is highest. Treat it as a separate destructive operation, not an automatic epilogue.

## Red flags

- You fixed a design-change comment without revising the proposal.
- You ignored a direct human commit.
- You recorded a policy or decision without the human.
- You wrote a prose policy before checking whether automation can enforce it.
- You changed an old policy or decision's meaning in place.
- You merged without preserving why the final state differs.
- You released because the PR merged.
- You merged a pull request that carries no changeset.
- A dependent PR still uses a preview version after release.
- A preview or prerelease artifact outlives its PR without a tracked, authorized teardown plan.
- You deleted a branch, force-pushed, or tore down a published prerelease artifact without explicit human confirmation.
- You waited until after merge to move `implemented` and opened a bookkeeping-only PR.
- You treated `implemented` as proof that a required release has already happened.

Any of these means stop and return to the relevant ordered step.

## Related skills

- `reviewing-an-implementation` — produces the preview and readiness record consumed during human review.
