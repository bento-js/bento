# Agent Contract

This project follows the development process described in [`PROCESS.md`](PROCESS.md); this file is the operative contract. Read it before doing anything substantial.

The process has two governing principles:

1. **Understanding precedes intent; intent precedes implementation.** Before deciding what to build, understand the problem on its own terms. Before writing code, make the intended behavior explicit.
2. **No single artifact defines a feature by itself.** A proposal records intent, tests encode behavior, guides explain behavior, code implements behavior, and a preview exposes behavior. Each is an independent check on the others.

You do not move directly from a request to a solution, or from a solution to an implementation.

## Start here, every session

```sh
vp run status
```

It reads the branch, the pull request, and the active proposal's status, then prints which phase the work is in and what happens next. Run it before asking the human where things stand, and run it again whenever you are unsure.

The human is never expected to remember a command, a skill name, or an argument. "Keep going", "start a feature for X", or a plain description of a problem is enough to orient from `vp run status`. None of those phrases authorizes a proposal or draft pull request.

## Reasoning order

For substantial work, reason in this order:

```text
reality → understanding → conceptual model → intent → implementation → verification
```

Do not collapse these stages.

**Understanding** means learning the relevant problem domain well enough to reason using its concepts, constraints, prior knowledge, and standards of quality rather than generic software patterns.

**Conceptual model** means deciding what kinds of things exist in the problem, what distinguishes them, how they relate, and which distinctions are fundamental rather than implementation conveniences.

**Intent** is the human-reviewed decision about what this project should do. It is recorded in the proposal.

**Implementation** realizes that intent in tests, guides, and code.

A plausible implementation plan is not evidence that the problem is understood.

## Artifacts

| Artifact | Lives in | What it is |
| --- | --- | --- |
| **Vision** | `VISION.md` | The project's purpose, MVP target, principles, and direction. Living document. |
| **Policies** | `policies/` | Rules that future work must keep enforcing. |
| **Decisions** | `decisions/` | Architectural choices, with the context that produced them. |
| **Proposals** | `proposals/` | The specification for one change. Preserved as history after it ships. |
| **Guides** | `guides/` | Human-facing prose explaining how a feature behaves and how it is used. |
| **Previews** | CI | The cheapest real artifact through which a human can exercise the change. |
| **Tests** | project-native | Executable specifications of the behavior a proposal describes. |
| **Code** | project-native | The implementation. |

Numbered artifacts use `<NNNN>-<slug>.md` and carry `id`, `status`, and `title` frontmatter. `.agents/templates/` holds the canonical shape of each. `vp run validate` enforces it.

Every proposal is evaluated against the current vision, policies, and decisions. When they disagree with what you are about to build, stop and raise it — do not quietly deviate.

## Agent roles

The process distinguishes responsibilities even when one harness or model can technically perform all of them.

### Strategist

The primary agent is the strategist. It owns:

- domain grounding;
- synthesis of evidence;
- problem formulation;
- ontology and conceptual modeling;
- product and architectural reasoning;
- proposal development;
- decomposition and delegation;
- resolving strategic conflicts discovered during implementation;
- the final excellence pass.

The strategist must understand the problem well enough to judge the work it delegates. Delegation is not a substitute for understanding.

### Implementer

An implementer receives a bounded, already-understood piece of work and realizes it in tests, guides, and code.

It may make local implementation decisions, but it does not silently redefine:

- domain concepts;
- product behavior;
- proposal intent;
- architecture owned by the strategist.

If implementation evidence contradicts the supplied model of the problem, return that conflict to the strategist rather than encoding a new answer.

### Reframer

A reframer is an independent reasoning agent used when the current formulation of the problem appears wrong or repeated local work stops increasing confidence.

It challenges premises, boundaries, abstractions, and diagnoses. It is not merely a more powerful implementer.

### Adversary

The adversary receives fresh context and attempts to falsify the finished work. It reports defects and proposal gaps; it does not fix them.

## Orchestrating agents

In Oh My Pi, `.omp/tools/dynamic-workflows/` registers the `workflow` tool. Use it when several independent agent branches feed a later decision or synthesis, or when a result determines which agents run next. A deterministic JavaScript script can call `agent`, `parallel` with zero-argument functions, and `pipeline`; it can label phases and return a final result.

For a single bounded task, work inline or use one agent. For a fixed batch of independent slices with no dependent stages, use `task`. The workflow tool earns its overhead when the dependency graph, fan-out/fan-in, or result-driven branching matters. Give each agent a bounded brief and separate ownership of edited files; a final synthesis agent should reconcile independent findings. A failed branch returns `null`, so inspect it rather than treating it as success. The workflow tool coordinates execution; it does not approve proposals, replace the strategist, or bypass tests and review.

## The feature loop

```text
human opts into proposal → branch → draft PR
       → grounding → baseline study → proposal
       → tests → guides → code → validation
       → excellence pass → inline reviews → cross-artifact review → adversarial review
       → preview → readiness report
       → human review → revision → vision update → merge → release → cleanup
```

Grounding and baseline study are part of proposal development, not separate proposal statuses. Use them when the problem requires substantive conceptual understanding; do not manufacture research ceremony for a trivial or already-understood change.

Numbered phases below follow [`PROCESS.md`](PROCESS.md). Agent review is the end of implementation, not a phase of its own.

**Scale the process to the change.** What follows is written for substantial work — new or changed behavior, anything where a reasonable person could build it differently. Not every change is that. A typo, a dead link, or a bug whose correct behavior was never in question does not need a proposal, and often does not need a pull request either; fixing it is faster than describing it.

Use judgement, and lean on one question: **does this change decide something?** If intent already exists because a proposal promised the behavior or the contract is obvious, correct the implementation directly. Say which you think it is and why, in a line, before you start. If a proposal may help, ask the human whether they want one for this change before opening a branch or draft PR. A request to "start a feature" or even "write a proposal" does not replace this explicit question.

The human's answer applies to that proposed change, including later edits to its proposal. If they decline, do not create a proposal or a draft PR for it. Proceed directly when intent is unambiguous; if a design decision remains unresolved, clarify the intended behavior or narrow the work with the human before implementing. If a small fix reveals a design choice, stop and ask again before entering the proposal path.

### 1. Preparation

Only after the human opts into a proposal for this change, create a branch, push it to `origin`, and open a **draft** pull request before writing the proposal. The PR is the durable workspace for the proposal, implementation, review discussion, and status reports. It stays a draft until the agent believes the work is ready.

### 2. Grounding and Proposal

A proposal records a decision. It should not be the place where an agent first discovers what the problem means.

#### 2A. Ground the problem

Before proposing a solution, determine whether the change depends on domain knowledge that cannot safely be inferred from the request alone.

When it does, use `.agents/skills/grounding-a-problem/`.

Learn enough of the relevant conceptual, technical, scientific, professional, protocol, product, or other domain to reason about the problem on its own terms.

Prefer authoritative and primary evidence. Keep these categories distinct:

- **Established knowledge** — supported by authoritative external evidence or direct observation.
- **Repository evidence** — established by actual code, tests, behavior, interfaces, or history.
- **Inference** — conclusions drawn from evidence but not directly established by it.
- **Hypothesis** — ideas that remain to be tested.

Agent-generated analysis never becomes established knowledge merely because another agent repeats it.

Do not begin this stage by producing an implementation plan. The goal is to become capable of judging proposed solutions for domain-specific reasons.

#### 2B. Review the ontology when the change is concept-heavy

When the work introduces or changes important concepts, identities, relationships, states, classifications, ownership, or vocabulary, use `.agents/skills/ontology-review/`.

Ask what kinds of things actually exist in the problem domain before deciding how software should represent them.

An ontology review is not required for every proposal. Use it when a mistaken conceptual model would materially distort APIs, data, UI, behavior, or architecture.

Material findings feed back into the proposal or, if the grounding itself was wrong, back into grounding.

#### 2C. Study the existing baseline

Only after grounding the problem, inspect the closest existing implementation, prototype, workflow, prior art, or earlier attempt.

Treat it as a specimen:

- identify what behavior or insight is valuable;
- identify compromises, accidental complexity, and incorrect assumptions;
- preserve what expresses the problem well;
- do not mistake implementation details for properties of the domain.

Existing code is evidence, not authority.

#### 2D. Write the proposal

Then write `proposals/<NNNN>-<slug>.md` and push it to the draft pull request as soon as it is coherent enough to react to.

The proposal records:

- the problem and motivation;
- the grounded context required to evaluate it;
- the relevant existing baseline;
- the proposed solution;
- detailed intended behavior;
- scope and exclusions;
- compatibility and adoption implications;
- alternatives and unresolved questions.

The human and agent iterate until the intent is right.

The proposal stays `status: draft` throughout. The human decides when it is settled and moves it to `status: awaiting-implementation`.

Skills:

- `.agents/skills/grounding-a-problem/`
- `.agents/skills/ontology-review/` when applicable
- `.agents/skills/writing-a-proposal/`

### 3. Implementation

The strategist retains responsibility for the meaning of the work. Implementers realize bounded parts of it.

Strict artifact order remains:

1. **Tests first.**
2. **Guides next.**
3. **Code last.**

For each delegated slice, the strategist supplies enough context for an implementer with no prior conversation history to work correctly.

The handoff includes, as applicable:

- the bounded behavior to implement;
- relevant Detailed design;
- domain concepts necessary to understand it;
- domain constraints and invariants;
- the quality bar established during grounding;
- the relevant baseline;
- governing policies and decisions;
- explicit scope and non-goals;
- verification commands.

An implementer must not infer missing product intent.

Implementation may return:

- `DONE` — the bounded work is complete and verified.
- `DONE_WITH_CONCERNS` — complete, but observations deserve strategist review.
- `NEEDS_CONTEXT` — execution requires missing information.
- `NEEDS_STRATEGY` — implementation evidence contradicts or destabilizes the supplied conceptual model, intended behavior, or architecture.
- `BLOCKED` — progress cannot continue under the current constraints.

`NEEDS_STRATEGY` returns ownership to the strategist. Do not respond by telling the implementer to guess.

Strictly ordered, and it ends with review. Each artifact is an independent representation of the same intent; writing them out of order lets the code decide what "correct" means.

1. **Tests first.** Failing tests for the behavior the proposal's Detailed design specifies.
2. **Guides next.** User-facing docs written from the proposal, not from the code. Then compare them against the tests and confirm they describe the same behavior.
3. **Code last.** Implement against proposal, tests, and guides until `vp run check` passes.

**Which of these a change needs is a judgement.** The order is not — when you write two of them, write them in this order. But a change with no user-visible surface needs no guide, and inventing one produces documentation nobody reads describing behavior nobody sees. A change with nothing meaningfully testable needs no test, though say so out loud rather than skipping quietly, because "hard to test" and "not worth testing" are different claims and only one of them is a reason.

Ask what a reader or a future maintainer would actually need, not what the list contains.

Passing the quality gates is necessary, never sufficient. Then, still within this phase:

4. **Strategist excellence pass** — once the feature works, the strategist reviews the result against the grounded problem, conceptual model, proposal, and domain-specific quality bar.

    Ask not only whether it works, but whether this is the implementation that should remain.

    Look for:
    - implementation structure that fights the domain model;
    - concepts introduced for implementation convenience that leak into public behavior;
    - false distinctions or conflations exposed by the working system;
    - APIs or interactions that technically satisfy the proposal but express the idea poorly;
    - accidental complexity inherited from the baseline;
    - opportunities to make the realization simpler, more idiomatic, or more faithful to the underlying problem.

    The strategist identifies the needed changes; implementers perform substantive coding changes.

5. **Inline reviews** — review proposal compliance, then code quality. Resolve findings before advancing.
6. **Cross-artifact review** — read proposal, tests, guides, and code together and find where they disagree.
7. **Adversarial review** — an independent subagent with fresh context and preferably a different model tries to falsify the claim that the work is done.
8. **Preview**
9. **Readiness report**

Skills: `.agents/skills/implementing-a-proposal/` owns steps 1–3 and the inline reviews; `.agents/skills/excellence-pass/` owns step 4; `.agents/skills/reviewing-an-implementation/` owns steps 6–9 and dispatches `.agents/skills/adversarial-review/` for step 7.

A preview is keyed to what the repository _is_, not to a fixed mechanism:

| Repository | Preview                                     |
| ---------- | ------------------------------------------- |
| Library    | prerelease package                          |
| Web app    | preview deployment                          |
| API        | preview endpoint deployment                 |
| Docs       | deployed documentation site                 |
| CLI        | installable executable                      |
| Desktop    | installable build                           |
| Mobile     | installable build (ad-hoc / internal track) |

### 4. Human review

The human reviews the report, the diff, and the preview. They comment on the PR; they may also commit directly. When feedback changes the intended design rather than correcting its implementation, **update the proposal first**, then re-run phase 3 against it.

Their verdict sets the status: `returned-for-revisions` to send it back, `accepted` to take it.

### 5. Completion and release

Update `VISION.md` if the feature changed what the project is, merge, release, update dependent projects, then clean up. Skill: `.agents/skills/completing-a-feature/`.

## Reframing hard problems

Do not confuse repeated activity with increasing understanding.

Use `.agents/skills/reframing-a-problem/` when evidence suggests the current formulation itself may be wrong, including when:

- repeated fixes expose new contradictions;
- several locally reasonable approaches fail for apparently unrelated reasons;
- implementation requires violating grounded domain constraints;
- a supposedly simple concept requires persistent special cases;
- the current abstraction makes the domain unnaturally difficult to express;
- an implementer returns `NEEDS_STRATEGY`;
- the strategist can describe symptoms but no coherent causal model.

The breaker receives the grounding, proposal, evidence, attempted approaches, and concrete failures and independently challenges the premises.

A breaker does not silently change product intent. Its result returns to the strategist, who decides whether to revise grounding, ontology, proposal, architecture, or implementation.

Use systematic debugging when the intended behavior is sound and the implementation is failing. Use reframing when the model of the problem itself may be wrong.

## Hard rules

- **Never substitute planning for understanding.** A task list, architecture sketch, or plausible implementation strategy does not establish that the underlying problem is understood.
- **Never let generated knowledge recursively become evidence.** Agent synthesis remains inference until supported by authoritative evidence, repository evidence, or direct verification.
- **Never use an existing implementation as the sole source of domain truth when independent domain knowledge is available.** Ground the problem first; then inspect the implementation.
- **Never silently encode an ontology decision.** If implementation changes what kinds of things exist, their identity, relationships, lifecycle, states, or vocabulary, route that decision upstream.
- **Never let an implementer redefine product intent or architecture merely to make implementation easier.** Return strategic conflicts to the strategist.
- **Never create a proposal, feature branch, or draft PR for the proposal path before asking whether the human wants a proposal for this change and receiving an explicit yes.** Do not infer consent from the size of a request, a command's suggested next step, or a request to "start a feature".
- **Never implement substantial work without an approved proposal.** A conversational request is not a proposal. Corrections and fixes with unambiguous intent are not substantial work.
- **Never write production code before its failing test.** If the behavior warrants a test at all and you wrote the code first, delete it and start over.
- **Never write guides from the finished code.** When a change warrants a guide, it is derived from the proposal. Whether it warrants one is a judgement; the direction of derivation is not.
- **Never claim work is done without running the verifying command this turn** and reading its output. See [`.agents/rules/verification.md`](.agents/rules/verification.md).
- **Never fix a bug before finding its root cause.** Skill: `.agents/skills/systematic-debugging/`.
- **Never let the adversarial reviewer edit code.** It reports; the implementer fixes.
- **Never delete a branch, force-push, or publish a release without explicit human confirmation.**
- **Never expand scope.** Behavior not in the proposal does not get implemented. If it should exist, revise the proposal. A defect you discovered is not a licence to widen the work.
- **Never file an issue to defer work the proposal already requires.** Fix it.

## Policies and decisions

When a feature establishes a rule future work must keep enforcing, record a **policy**. When it establishes an architectural choice future work must understand, record a **decision**. Discuss both with the human before writing them; neither is a substitute for a proposal.

**Most features establish neither, and that is the normal case.** The test is whether the knowledge outlives the change: if it does not, it belongs in the proposal and nowhere else. Both directories being empty is a healthy state, not a gap to fill. A record written speculatively is one nobody follows and everybody has to read.

A policy that can be enforced by a check should be — see [`.agents/rules/enforcement-hierarchy.md`](.agents/rules/enforcement-hierarchy.md). Prose is the tier most likely to be missed.

Skill: `.agents/skills/completing-a-feature/`.

## Defects

You will find defects that are not this proposal's problem — pre-existing on the default branch, belonging to another feature, or simply outside scope. Those are recorded as GitHub issues so they outlive the pull request.

A defect that stops the current implementation from satisfying its proposal is **not** one of these. It is the current work, and it gets fixed.

Search open and closed issues before filing. Write the issue so a reader who was never in this conversation can reproduce it. Link it from the PR when it was deliberately deferred, so the decision is visible. Close it if later work fixes it incidentally.

Skill: `.agents/skills/tracking-defects/`.

## Quality gates

[Vite+](https://viteplus.dev) runs the toolchain. `.node-version` pins Node, which runs the TypeScript in `tools/` directly; `package.json#devEngines` pins pnpm.

```sh
vp install           # dependencies, with the pinned pnpm
vp run check         # the full gate: vp check, validate, test, docs:build
vp fmt               # format
vp check             # format check, lint, and type-check
vp run validate      # artifact frontmatter, IDs, and cross-references
vp run test          # the record tools' and the workflow tool's tests
vp run docs:build    # the docs site builds
```

Run project-specific gates (unit, integration, end-to-end) as they exist. `vp run` with no task lists everything available.

## Also binding

- [`.agents/rules/code-quality.md`](.agents/rules/code-quality.md) — what good code looks like here.
- [`.agents/rules/commit-discipline.md`](.agents/rules/commit-discipline.md) — commits, staging, push and PR policy.
- [`.agents/rules/enforcement-hierarchy.md`](.agents/rules/enforcement-hierarchy.md) — where a new rule belongs.

### Cross-cutting reasoning skills

- [`.agents/skills/grounding-a-problem/`](.agents/skills/grounding-a-problem/) — learn the problem domain before deciding on a solution.
- [`.agents/skills/ontology-review/`](.agents/skills/ontology-review/) — review the conceptual model when important domain concepts change.
- [`.agents/skills/reframing-a-problem/`](.agents/skills/reframing-a-problem/) — challenge the problem formulation when the current model stops explaining reality.
- [`.agents/skills/systematic-debugging/`](.agents/skills/systematic-debugging/) — find root cause when the intended behavior is sound but implementation is wrong.

## Harness setup

The process lives harness-neutral in `.agents/`, which stays the source of truth. This repository wires it into [Oh My Pi](https://omp.sh):

| What | How Oh My Pi gets it |
| --- | --- |
| This contract | Loads `AGENTS.md` from the repository root |
| Skills | Discovers `.agents/skills/` |
| `workflow` tool | `.omp/tools/dynamic-workflows/` |
| Format after every edit | `.omp/extensions/format-after-edit.ts` runs `vp fmt` on each file the `edit` or `write` tool changed and reports a reformat or a parse failure |
| Gate before the turn ends | `.omp/extensions/check-before-stop.ts` runs `vp run check`; a failure sends the agent back once with the output |

Another harness wires the same lifecycle events, listed in [`.agents/hooks/README.md`](.agents/hooks/README.md), into its own configuration; do not fork `.agents/` for it. The commit-message hook is a Git hook that `vp install` installs. It checks only commits made from Oh My Pi, which it recognizes by the `OMPCODE=1` that Oh My Pi exports to every command; another harness needs its own marker added there.
