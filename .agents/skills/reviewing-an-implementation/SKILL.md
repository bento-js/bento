---
name: reviewing-an-implementation
description: Use when an implemented proposal has fresh validation evidence and needs its Phase 3 comparison, independent refutational review, human-exercisable preview, and ready-for-review handoff.
---

# Reviewing an Implementation

**Phase 3 — Implementation.** Compare the independent representations before asking for review of the changed behavior. This procedure owns the confirmatory review, adversarial dispatch, preview, and readiness handoff. It does not by itself decide acceptance: acceptance is the human's, or the strategist's where a bounded build mandate delegates it ([`AGENTS.md`](../../../AGENTS.md#bounded-build-mandates)).

## Inputs and order

Read these together before review. Add the current vision, policies, and decisions when they constrain the change.

| Input                 | Read             |
| --------------------- | ---------------- |
| Intent                | Current proposal |
| Executable behavior   | Changed tests    |
| Human-facing behavior | Affected guides  |
| Concrete behavior     | Implementation   |

Proceed in this order:

1. Cross-artifact review.
2. Dispatch `adversarial-review`; consume its report.
3. Produce the preview.
4. Post the readiness report and make the pull request ready together.

Begin after applicable validation has fresh evidence under `.agents/rules/verification.md` and the strategist's excellence pass and ordered inline reviews have no open findings. A substantive fix re-enters those gates before cross-artifact review.

## 1. Cross-artifact review

This is a comparison of representations, not a code-quality review. Build a behavior inventory before judging alignment.

### Build the behavior inventory

Use one row per observable behavior. The four evidence columns are proposal, tests, guides, and code. Tick a source only when it explicitly covers the behavior. Investigate every row that is not fully ticked.

| Behavior                 | Proposal | Tests | Guides | Code |
| ------------------------ | -------- | ----- | ------ | ---- |
| Reject malformed records | ✓        | ✓     | ✓      | ✓    |
| Preserve input order     | ✓        | ✓     | —      | ✓    |

Keep boundary cases, errors, defaults, and observable side effects as separate rows when they can disagree. Do not tick a source because it probably implies the behavior.

### Make all six checks

For every inventory row, establish all of these:

1. Tests cover what the proposal promises.
2. Guides describe what the tests exercise.
3. Code implements what the proposal and guides describe.
4. No documented behavior is missing from the implementation.
5. No implemented behavior is absent from the proposal and guides — this catches scope creep.
6. Implementation details have not silently changed scope or intent.

### Resolve every gap

Each non-full row resolves in exactly one way.

| Resolution | Use when | Action |
| --- | --- | --- |
| Fix the wrong artifact | Intent has not changed | Correct the tests, guides, or code, then recheck the row. |
| Revise the proposal | Intent or scope changed | Discuss with the human first, unless a mandate delegates that decision and the revision stays inside its boundaries; record it either way. Then derive affected tests, guides, and code again. |
| Record the divergence | It remains deliberate for human review | Name the behavior, affected sources, impact, and reason in the readiness report. |

You MAY edit tests, guides, and code to remove a disagreement. You MUST NOT revise a proposal without the human unless a mandate delegates that design decision and the revision stays inside its boundaries; that is a design change either way, so record the authority with it.

Record the inventory, each gap, and its resolution. `ALIGNED` requires a fully ticked inventory. A record with deliberate gaps is `DIVERGENCES RECORDED`; a human design decision is `REVISION REQUIRED`.

## 2. Dispatch and consume adversarial review

Dispatch a separate subagent with fresh context for `adversarial-review`. Use the strongest available portability rung: different model family, then different model, then a fresh-context run. Supply the current proposal, tests, guides, and implementation. The adversary reports only; it never edits the artifacts.

Do not replace this dispatch with your own review. The adversary attacks un-enumerated edges, test quality, interpretation, security surface, and proposal gaps from a refute-by-default stance.

Consume its report before continuing:

| Report item | Response |
| --- | --- |
| Confirmed defect | Fix it, refresh affected evidence, and repeat the required review stages. |
| Suspicion | Verify it or drop it; never fix it blind. |
| Proposal gap | Surface it to the human, or settle it yourself only where a mandate delegates the decision, and record it. Never silently choose an interpretation. |
| Declined finding | Preserve it with reasoning for the readiness report. |

Repeat the adversarial stage after a defect fix. Continue until its verdict is `CONVERGED`. A disputed finding does not disappear because you decline it.

## 3. Produce the preview

A preview is **the cheapest realistic artifact through which the human can exercise the changed behavior.**

| Requirement | Meaning                                                                   |
| ----------- | ------------------------------------------------------------------------- |
| Cheapest    | Do not stand up production infrastructure merely for review.              |
| Realistic   | Do not substitute screenshots, transcripts, or descriptions for behavior. |

Reviewing prose is not reviewing behavior. Use the configured project task and CI mechanism; run `vp run` with no task when the setup is unknown.

| Repository shape | Preview                                               |
| ---------------- | ----------------------------------------------------- |
| Library          | Prerelease package                                    |
| Web app          | Preview deployment                                    |
| API              | Preview endpoint                                      |
| Docs             | Deployed documentation site                           |
| CLI              | Installable executable                                |
| Desktop          | Installable build                                     |
| Mobile           | Installable build via ad-hoc or internal distribution |

For a mixed repository, preview the surface the proposal changed; do not produce every repository-shaped preview by habit. A pure refactor with no exercisable surface produces no preview. State that plainly in the readiness report rather than fabricating one.

**The human must be able to open or install it directly.** A downloadable build artifact is not a preview — reviewing it would mean unzipping a file and starting a local server, which is enough friction that the change gets reviewed as prose instead. The one exception is a command-line tool, where installing the binary is the real act. If your preview step ends in "upload artifact" for anything else, it is not finished.

A site deployed to a subpath needs its base path set to that subpath at build time, or every asset resolves against the domain root and the preview loads blank. Verify the deployed preview actually renders before linking it; a green workflow is not evidence, per `.agents/rules/verification.md`.

Build from the pull-request revision. Give repository-accessible humans a stable link or installation location; they must not build the artifact themselves. The readiness report links it and states required setup, what to try, and the expected result. A bare URL is insufficient. Keep the preview's lifetime tied to the pull request.

Previews are private by default. Do not publish one publicly unless the project deliberately opts in. A prerelease package published to a public registry is public forever; use a prerelease tag and state that public status in the readiness report. A mandate that delegates merging delegates nothing about publication: making a preview public needs the human's explicit authority for that act. Teardown belongs to Phase 5 cleanup through `completing-a-feature`.

## 4. Report readiness and mark ready

Compose the pull-request comment from `.agents/templates/READINESS_REPORT.md`. Fill every template section and add the cross-artifact inventory verdict, divergences, and resolutions.

| Report section | Required content |
| --- | --- |
| Proposal | State `FULLY` implemented only when every promised behavior exists. For `PARTIALLY`, name completed and omitted behavior, limitations, and reason. Silent partial implementation is not allowed. |
| What was built | Delivered behavior, not internals. |
| How to exercise it | Preview link or honest absence, setup, actions, and expected results. |
| Validation performed | Every command actually run in this session and its observed result, per `.agents/rules/verification.md`. Never report an unrun gate as passing. |
| Adversarial findings adopted | Each accepted or fixed finding, or `- None.` |
| Adversarial findings declined, with reasoning | Every declined finding and why it was declined, or `- None.` This section is always present. |
| Defects discovered and deferred | Each deliberate, out-of-scope defect as a linked GitHub issue from `tracking-defects`, or `- None.` |
| Remaining limitations | Known behavior gaps or `- None.` |
| What to look at first | The first behavior, preview action, or diff the human should inspect. |

Declined adversarial findings keep the independent review useful. Do not quietly omit a narrow, uncomfortable, or disputed finding.

### Post as a service account

Configure a bot token or GitHub App installation token in the environment. Confirm the authenticated identity before posting.

```sh
GH_TOKEN="$SERVICE_ACCOUNT_TOKEN" gh pr comment <pr-number> --body-file readiness-report.md
GH_TOKEN="$SERVICE_ACCOUNT_TOKEN" gh pr ready <pr-number>
```

Without a service account, post from the available authenticated identity and say in the report: `Posted from <identity>; no service account is configured.` Do not imply bot authorship.

### Make the coupled transition into Phase 4

At the end of Phase 3, these three actions form one transition and MUST NOT be separated:

1. Move the proposal to `status: active-review`.
2. Post the readiness report.
3. Flip the pull request from draft to ready with `gh pr ready <pr-number>`.

Do not leave the proposal, report, and pull-request state disagreeing. Ready means the work is reviewable, not correct or accepted. `active-review` remains the readiness transition; under delegated acceptance it does not have to wait for the human to read it.

Acceptance in Phase 4 is the human's. Where a mandate delegates it, the strategist — never an implementer accepting its own claim — may set `accepted` once the independent reviews of the latest relevant revision are clean, fresh checks and smoke of the integration candidate pass, and no real defect, unmet mandated outcome, or disputed material finding remains open. Record the authority relied on and the evidence. Phase 4 then starts Phase 5 completion.

## Red flags

- You reviewed code quality instead of comparing representations.
- A behavior-inventory row is not fully ticked and has no recorded resolution.
- You changed proposal intent without the human and without delegated authority that covers it.
- You skipped independent adversarial review or dropped a declined finding.
- The preview is fabricated, public by accident, detached from the pull request, or a bare URL.
- The report claims a validation gate was not run.
- You marked the pull request ready without the proposal status and report.
- You set `accepted` on your own implementation claim, on stale or failing evidence, or with a disputed finding unresolved.
- You read a merge grant as permission to publish a preview, package, or release.
- You stretched a mandate's boundaries instead of stopping the slice, recording the question, and waiting for the human.

Correct the record before handoff.
