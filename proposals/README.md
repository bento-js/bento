# Proposals

This directory holds the durable design record for one change per file.

Name each file `<NNNN>-<slug>.md`, using the next four-digit number and a kebab-case slug. Required frontmatter is `id: proposal.<NNNN>`, `title`, `authors` as a non-empty list, `status`, `pull-request`, and `supersedes`. Optional `issues` is a list of issue references or URLs. The normal lifecycle is `draft` → `awaiting-implementation` → `active-review` → `accepted` → `implemented`.

| Status | Meaning | Decided by | Phase |
| --- | --- | --- | --- |
| `draft` | Being written and iterated on in the draft pull request. May carry `[NEEDS CLARIFICATION:`. | agent | 2 |
| `awaiting-implementation` | The human settled the proposal. Implementation is underway or not yet started. | human | 3 |
| `active-review` | First full pass at tests, guides and code exists; report posted and pull request marked ready. | agent | 4 |
| `returned-for-revisions` | The human requested changes. Back to implementation, or to the proposal if the design changed. | human | 4 → 3 or 2 |
| `accepted` | The human accepted the work. Vision update, merge and release are underway. | human | 5 |
| `implemented` | The accepted implementation and final records are committed in the feature pull request before merge. Merge and any required release remain separate. | agent | 5 |
| `rejected` | The human declined the change. Kept as record. | human | terminal |
| `withdrawn` | The author pulled it. Kept as record. | either | terminal |
| `superseded` | A later proposal replaced it. | either | terminal |

**Decided by** is about judgement, not keystrokes. The human may ask the agent to make the edit — that is fine. What an agent must never do is decide on its own that a proposal is settled enough to implement, or that finished work is good enough to accept. An agent that can reach those two conclusions unaided is an agent approving its own work.

After acceptance and the vision update, the agent records `implemented` in the original feature pull request before merging it. This status does not claim that the pull request has merged or that a required release happened. The pull request records integration; keep any remaining delivery visible in the proposal or pull request until separately authorized and performed. A merge-only change needs no release.

A proposal stays `draft` for as long as it takes. It is published to the draft pull request as soon as there is something to react to, and the human edits it there — directly, or by asking for changes — until they are satisfied. Iteration is the normal case, not a sign of a bad first draft.

History is preserved: never delete a superseded entry — mark it `superseded` and link its replacement through `supersedes`.
