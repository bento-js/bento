---
name: implementing-a-proposal
description: Use when an approved proposal is ready for Phase 3 implementation, including a substantive bug fix whose root cause is known. Do not use for proposal authoring or trivial inline edits.
---

# Implementing a Proposal

Phase 3 turns an approved proposal into tests, guides, and code that agree with the proposal and with each other.

**Hard order when stages apply:** failing tests → guides → code → quality gates.

## Preconditions

- The proposal has `status: awaiting-implementation`, or the human returned it with `status: returned-for-revisions`; neither has an unresolved clarification marker.
- Phase 1 — preparation — created the branch, pushed it to `origin`, and opened a draft pull request.
- Read the proposal's **Detailed design**, the code it touches, and the vision, policies, and decisions it names. Tests derive from Detailed design; acceptance criteria are an occasional supplement.
- When the design is ambiguous or conflicts with a record in a way that changes behavior, raise it. Do not choose silently or expand scope.

## Who does the work

Implement the proposal yourself by default. A single agent holding the whole change makes fewer handoff errors than a strategist briefing implementers.

Split the work only into slices that are independent, touch separate files, and save real time by running in parallel. Give each subagent the behavior, the relevant Detailed design, constraints, scope, and verification commands. Integrate and commit its work yourself. When a subagent reports that the evidence contradicts the proposal's model, stop that slice and use `reframing-a-problem`; settle intent with the human before resuming.

## 1. Failing tests

```text
NO TEST-WORTHY PRODUCTION BEHAVIOR WITHOUT A FAILING TEST FIRST
```

- Write one focused test per observable behavior in Detailed design, named after the proposal id and the behavior.
- Run it. Confirm it fails because the behavior is missing, not because of setup, an import, or a typo. If it passes, correct the test.
- If you wrote test-worthy production code first, delete it and start from the test.
- A claim that something holds "always", "never", "for any", or "for all" gets a property-based test that fails first, alongside example tests for named boundaries. Do not force properties onto a single mapping or constant.
- Test real code. Isolate only what you cannot control, such as the network, time, or randomness. When a test is hard to write, simplify the interface rather than adding mocks.
- When nothing can be meaningfully tested, say so and why before writing code.

## 2. Guides

Write a guide only when the change alters what a user does, sees, or must know. Write it from the proposal before production code exists, for a reader who will never read the proposal: inputs, prerequisites, actions, outcomes, errors, defaults, and durable effects, not internals.

Extend the guide in `guides/` that already owns the workflow; add one from [`.agents/templates/GUIDE.md`](../../templates/GUIDE.md) only when none does. Keep its frontmatter valid: `id` matches the path, `title` stays accurate, and `describes` keeps earlier proposal ids while adding the current one. Every example must run as printed.

Then compare the guide with the failing tests. Anything one describes and the other does not is a defect in one of them; resolve it by correcting the guide or the test, or by revising the proposal with the human.

## 3. Code

Write the minimum production code that makes the tests pass and matches the guide. Add no behavior the proposal does not describe. When an unexplained failure appears, find its cause with [`.agents/skills/systematic-debugging/`](../systematic-debugging/SKILL.md) before changing code.

## 4. Gates and commits

Run `mise run check`, then any project-specific gate `mise tasks` lists that it does not already cover, and read the output; [`.agents/rules/verification.md`](../../rules/verification.md) says what counts as evidence. Never commit while red. Commit at green boundaries following [`.agents/rules/commit-discipline.md`](../../rules/commit-discipline.md), and push.

## Then

Continue with [`.agents/skills/reviewing-an-implementation/`](../reviewing-an-implementation/SKILL.md). It owns the self-check, the independent review, the preview, and the handoff to the human. Set no proposal status before then.
