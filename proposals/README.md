# Proposals

This directory holds the durable design record for one change per file.

Name each file `<NNNN>-<slug>.md`, using the next four-digit number and a kebab-case slug. Required frontmatter is `id: proposal.<NNNN>`, `title`, `authors` as a non-empty list, `status`, `pull-request`, and `supersedes`. Optional `issues` is a list of issue references or URLs. The normal lifecycle is `draft` → `awaiting-implementation` → `active-review` → `accepted` → `implemented`.

| Status | Meaning | Decided by | Phase |
| --- | --- | --- | --- |
| `draft` | Being written and iterated on in the draft pull request. May carry `[NEEDS CLARIFICATION:`. | agent | 2 |
| `awaiting-implementation` | The approval owner settled the proposal. Implementation is underway or not yet started. | approval owner | 3 |
| `active-review` | First full pass at tests, guides and code exists; report posted and pull request marked ready. | agent | 4 |
| `returned-for-revisions` | The acceptance review asked for changes. Back to implementation, or to the proposal if the design changed. | approval owner | 4 → 3 or 2 |
| `accepted` | The approval owner accepted the work. Vision update, merge and required delivery are underway. | approval owner | 5 |
| `implemented` | Merged, with every delivery the proposal requires performed. | agent | done |
| `rejected` | The approval owner declined the change. Kept as record. | approval owner | terminal |
| `withdrawn` | The author pulled it. Kept as record. | either | terminal |
| `superseded` | A later proposal replaced it. | either | terminal |

**Decided by** names the approval owner: the human, or the strategist for decisions delegated by [an active build mandate](https://github.com/markmals/bento/blob/main/AGENTS.md#bounded-build-mandates). Proposal approval requires settled intent and recorded authority. Acceptance of implementation additionally requires the readiness report, fresh checks, independent reviews, and an exercised preview. An implementer's completion claim alone cannot authorize acceptance.

Every proposal records where its authority came from in `## Authorization`: the human's direct consent for this change, or the mandate proposal and approved revision it runs under, why the change fits inside it, and the approval decisions the strategist took. A proposal may also carry an optional `## Build mandate` section; once the human approves that proposal, it is what delegates a bounded set of decisions. Mandates need no separate directory, frontmatter key, or status — an ordinary proposal in this lifecycle carries them — and a mandate proposal cannot authorize itself.

`implemented` means merged and delivered. A proposal moves there once the merge and every delivery it or its mandate requires are performed. A merge-only change reaches `implemented` without a release; where a release is required it needs authorization of its own, and the proposal stays `accepted` until it has actually happened.

A proposal stays `draft` for as long as it takes. It is published to the draft pull request as soon as there is something to react to, and the approval owner iterates on it there — editing directly, or asking for changes — until the design is settled. Iteration is the normal case, not a sign of a bad first draft.

History is preserved: never delete a superseded entry — mark it `superseded` and link its replacement through `supersedes`.
