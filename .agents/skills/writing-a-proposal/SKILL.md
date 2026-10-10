---
name: writing-a-proposal
description: Use when a new or substantial change needs definition before tests, guides, or code — especially when intent is unclear, designs compete, existing records may constrain the change, or the change needs approval from the human or under a bounded build mandate.
---

# Writing a Proposal

**Phase 2 — Proposal development.** A proposal is the one durable record of one coherent change. It makes intent concrete enough to derive tests, guides, and code without returning to the original conversation.

## First, decide whether this needs a proposal

Proposals are for changes that decide something: new or changed behavior, a real choice between approaches, anything establishing a policy or decision, anything touching the vision.

They are not for changes whose intent already exists. A bug has its correct behavior specified already — by the proposal that promised it, or by a contract too obvious to write down. Correcting an implementation is not defining one. Fixing a typo, a dead link, or a workflow whose behavior was never in question needs no proposal and often no pull request; describing it costs more than doing it.

Say whether you think the change warrants a proposal and why. Then establish who authorizes it. Under a valid bounded build mandate covering this change — see [`AGENTS.md`](../../../AGENTS.md#bounded-build-mandates) for what makes a mandate valid and what it can delegate — the strategist may create the workspace and the proposal without asking feature by feature, and records that authority in the proposal. Without such a mandate, ask the human directly: "Do you want a proposal for this change?" Ask even if they said "start a feature" or "write a proposal"; only their answer to this question for this change counts as consent. Do not infer yes from scope, status output, or silence, and never read a mandate as covering work beyond its stated boundaries.

If the answer is no, do not write a proposal or open a draft pull request for it. Correct unambiguous behavior directly. If intent needs a real design decision, explain the conflict and clarify or narrow the work with the human before implementation. If a small fix later reveals such a decision, stop and ask the proposal question then.

## Preparation follows authorization

Only after an explicit yes for this change, or under a mandate that covers it, confirm the change has a branch pushed to `origin` and an open draft pull request before writing the proposal. Individual consent covers revisions of that change; a different change needs its own consent or applicable mandate authority.

## Output

Create exactly one `proposals/<NNNN>-<slug>.md`. Copy [`.agents/templates/PROPOSAL.md`](../../templates/PROPOSAL.md); do not replace it with a folder or auxiliary planning files.

- Use the next unused four-digit number and a kebab-case slug.
- Complete the template frontmatter: `id: proposal.<NNNN>`, Title Case `title`, `status: draft`, the draft PR URL or number, and `supersedes`.
- Record the authority in an **Authorization** section: under a mandate, link the mandate proposal and the approved revision relied on, say why this change fits inside it and which delegated authority is being used, and keep the strategist's decisions there as they are made. For ordinary work, record the human's direct consent for this change instead.
- A proposal that carries a `## Build mandate` section is itself an ordinary change: the human approves it, and it never authorizes itself or widens an existing mandate.
- Complete every required template section: Summary, Authorization, Motivation, Domain grounding, Existing baseline, Proposed solution, Detailed design, Compatibility, Implications on adoption, Scope, Out of scope, Preview, Policies and decisions checked, Future directions, Alternatives considered, and Open questions. Acknowledgments is optional, and Build mandate appears only in a proposal that grants one.
- **Detailed design is what tests derive from.** Write it precisely enough that someone who is not the author could implement it without asking questions.
- Usually delete the template's Acceptance criteria section. Restating Detailed design as a checklist duplicates the specification in a form that drifts from it. Keep it only for a change with discrete outcomes the prose cannot express naturally — a structural change touching many independent things, or a migration with separable steps.

## Size before detail

One proposal covers one coherent change. Split before authoring when either is true:

- The motivation solves two unrelated problems.
- The change needs more than roughly six acceptance criteria.

Choose the smallest independently useful slice. A proposal with a broad motivation is not made coherent by a long scope section.

## Process

1. **Read the record.** Read `VISION.md`, `policies/`, and `decisions/` before proposing. In **Policies and decisions checked**, list each relevant record and its constraint; write `None` when none apply.

    If the change conflicts with a policy or decision, surface the conflict in that section. The strategist may revise the proposal or supersede the record only within delegated authority. Conflicts beyond that authority go to the human. Never quietly violate a governing record.

2. **Ground the problem.** When the change depends on domain knowledge that cannot safely be inferred from the request, use `grounding-a-problem` before comparing solutions. Prefer primary or authoritative evidence and separate it from repository observations, inference, and hypotheses. In **Domain grounding**, record the concepts, constraints, quality bar, and remaining uncertainty needed to assess the proposal. If no independent grounding is needed, state why there.

3. **Review the ontology when material.** If the change alters important concepts, identity, relationships, states, or vocabulary, use `ontology-review` against the grounded domain before settling the design. Resolve material conflicts in the proposal; do not encode them silently.

4. **Study the baseline.** After grounding, inspect the closest existing implementation, prototype, workflow, or prior attempt. In **Existing baseline**, name what to preserve and what is incidental or compromised. If there is no relevant baseline, state that explicitly rather than inventing one.

5. **Discover one answer at a time.** Ask one question, wait for its answer, then ask the next. Use multiple-choice questions when the answer space is finite. Cover:
    - who needs the change and the outcome they seek;
    - the trigger, success condition, and boundaries;
    - dependencies, constraints, and affected records; and
    - what is deliberately out of scope.

    Do not guess. Under a mandate, settle the questions that fall inside its boundaries and record the decision in **Authorization**. Otherwise ask, or record `[NEEDS CLARIFICATION: <question>]` in **Open questions** and continue.

6. **Compare approaches.** Present two or three viable approaches with tradeoffs. Recommend one and explain why. Under a mandate, the strategist may select and record an approach that stays within the delegated outcome, scope, and constraints. Otherwise ask the human to select an approach before treating the design as settled.

7. **Choose the preview now.** When the change has an exercisable surface, select the cheapest realistic artifact through which the human will exercise it. Record the repository shape, artifact, private access path, and completion cleanup in **Preview**. For a pure refactor without such a surface, record that no preview applies and why.

    | Repository shape | Preview artifact                            |
    | ---------------- | ------------------------------------------- |
    | Library          | prerelease package                          |
    | Web app          | preview deployment                          |
    | API              | preview endpoint deployment                 |
    | Docs             | deployed documentation site                 |
    | CLI              | installable executable                      |
    | Desktop          | installable build                           |
    | Mobile           | installable build (ad-hoc / internal track) |

    Keep the preview private unless the project deliberately opts into publication. Do not configure unused preview mechanisms.

8. **Author the proposal.** Fill the template from the grounding, baseline, answers, and selected approach. State behavior and boundaries plainly. Record rejected approaches and why they lost. Keep unresolved questions as markers rather than inventing answers.

9. **File unrelated defects.** If discovery reveals a defect outside this proposal's scope, file it as a GitHub issue. Discovering a defect is never a reason to expand the proposal or forget it. Use [`.agents/skills/tracking-defects/`](../tracking-defects/) for the judgment and filing process.

10. **Self-review.** Before showing the human, check:
    - every template section and frontmatter field is complete;
    - the grounding, baseline, motivation, behavior, criteria, scope, preview, and record check agree;
    - no unstated assumption remains outside an explicit clarification marker;
    - no rejected approach or out-of-scope item reappears as promised behavior; and
    - the proposal remains one coherent change.

    Fix clear defects. Count remaining clarification markers and present them to the human.

11. **Publish the draft.** Commit the proposal and push it as soon as it is coherent enough to react to. Do not polish it privately first — the draft pull request exists so the human can read and edit the proposal in place, and a proposal held back is a proposal being decided by the agent alone.

12. **Iterate on the pull request.** The human comments, requests changes, and may commit edits to the proposal directly. Read those commits before responding; they are instructions, not conflicts to resolve. Keep iterating — answering questions, revising the design, resolving markers — until the human is satisfied, or, under a mandate covering the change, until the proposal is coherent and marker-free inside its boundaries. This can take many rounds, and the proposal stays `draft` throughout.

13. **Close the draft under the right authority.** The human decides when the proposal is settled and moves it to `status: awaiting-implementation`. They may ask you to make that edit; making it when asked is fine, deciding it without authority is not. Under a mandate covering the change, the strategist may make that move once the proposal has no clarification markers, and records it in **Authorization**.

    Never decide it on unauthorized judgment. `accepted` is a different and much later state — it records acceptance of the finished implementation after independent review, not permission to begin. An agent that reaches either conclusion unaided and unauthorized is an agent approving its own work.

## The gate, once a proposal exists

This gate applies to work you judged proposal-worthy. It does not turn every change into one — that decision was made at the top of this skill, and a correction never reaches here.

Once a proposal exists, never write tests, guides, or code until all of these are true:

1. The proposal file exists.
2. It contains zero `[NEEDS CLARIFICATION:` markers.
3. `status: awaiting-implementation` is set — by the human, or by the strategist under a mandate covering the change.

Then enter Phase 3 — Implementation with `implementing-a-proposal`.

## Red flags — stop and correct

| Symptom | Required response |
| --- | --- |
| No explicit yes to the proposal question, and no mandate covering this change | Ask before creating a branch, draft PR, or proposal. |
| No pushed branch or draft pull request | Return to Phase 1 — Preparation. |
| Two unrelated motivations or more than roughly six criteria | Split the proposal. |
| A record conflicts with the change | Surface it; resolve it within delegated authority or return it to the human. |
| An unknown detail has no clarification marker | Ask or add the marker. |
| Preview names no exercisable artifact for a behavior change | Choose and record one before approval. |
| A newly discovered defect expands scope | File a GitHub issue; keep the proposal bounded. |
| Existing code suggests the design | Treat it as evidence, not authority; settle intent in the proposal. |
| The change would stretch the mandate's outcome, scope, or constraints | Stop that slice, record the question and evidence, keep other authorized work moving, and wait for the human. |
| A mandate has expired, completed, or was revoked | Fall back to per-change human consent immediately. |

**Do not use the existing implementation to teach yourself the domain when independent domain knowledge is available. Ground yourself first; then inspect the implementation as a specimen.**

## Anti-patterns

- **Proposal as a conversation summary.** Record decisions and observable behavior, not chat history.
- **Folder-shaped planning.** The proposal is one document. Do not distribute its sections across files.
- **Approval by silence.** `draft` is not accepted. The status changes by a human decision or by an authority a mandate actually delegates — never by silence or self-assessment.
- **Preview at review time.** Decide it during Phase 2, then make it during Phase 3.
- **Implementation-led intent.** Existing code does not resolve open questions.
- **Self-granted authority.** Only the human widens, renews, or revokes a mandate. Reading a change into one is not authorization.
