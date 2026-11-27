# Proposals

This directory holds one change record per file: what the change was, why it was made, and how it was reviewed. A completed proposal is history. Current requirements live in the specifications, policies, and decisions it changed; see Workbench's [Keeping Current Specifications](https://markmals.github.io/workbench/guides/current-specifications/).

Name each file `<NNNN>-<slug>.md`, using the next four-digit number and a kebab-case slug. Required frontmatter is `id: proposal.<NNNN>`, `title`, `authors` as a non-empty list, `status`, `pull-request`, and `supersedes`. Optional `issues` is a list of issue references or URLs. The normal lifecycle is `draft` → `awaiting-implementation` → `active-review` → `accepted` → `implemented`.

| Status                    | Meaning                                                                                                                                                                        | Decided by | Phase      |
| ------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ---------- | ---------- |
| `draft`                   | Being written and iterated on in the draft pull request. May carry `[NEEDS CLARIFICATION:`.                                                                                    | agent      | 2          |
| `awaiting-implementation` | The human settled the proposal. Implementation is underway or not yet started; the feature pull request becomes ready once the complete implementation is verified and pushed. | human      | 3          |
| `active-review`           | The readiness report is posted after Phase 3 reviews and preview; the pull request was made ready earlier.                                                                     | agent      | 4          |
| `returned-for-revisions`  | The human requested changes. Back to implementation, or to the proposal if the design changed.                                                                                 | human      | 4 → 3 or 2 |
| `accepted`                | The human accepted the work. Vision update, merge and release are underway.                                                                                                    | human      | 5          |
| `implemented`             | The accepted implementation and final records are committed in the feature pull request before merge. Merge and any required release remain separate.                          | agent      | 5          |
| `rejected`                | The human declined the change. Kept as record.                                                                                                                                 | human      | terminal   |
| `withdrawn`               | The author pulled it. Kept as record.                                                                                                                                          | either     | terminal   |
| `superseded`              | A later proposal replaced it.                                                                                                                                                  | either     | terminal   |

Pull-request readiness and proposal status track different facts. A ready feature PR exposes implemented code for inspection while its proposal may still be `awaiting-implementation`; `active-review` records the later readiness-report handoff to human review.

**Decided by** is about judgement, not keystrokes. The human may ask the agent to make the edit — that is fine. What an agent must never do is decide on its own that a proposal is settled enough to implement, or that finished work is good enough to accept. An agent that can reach those two conclusions unaided is an agent approving its own work.

After acceptance and the vision update, the agent records `implemented` in the original feature pull request before merging it. This status does not claim that the pull request has merged or that a required release happened. The pull request records integration; keep any remaining delivery visible in the proposal or pull request until separately authorized and performed. A merge-only change needs no release.

A proposal stays `draft` for as long as it takes. It is published to the draft pull request as soon as there is something to react to, and the human edits it there — directly, or by asking for changes — until they are satisfied. Iteration is the normal case, not a sign of a bad first draft.

Proposals are kept after they close. When a later proposal replaces an earlier one as a change, it names it in `supersedes` and the earlier one becomes `superseded`. That relationship records how the changes were made; it does not say which requirements apply now.
