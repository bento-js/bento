# Commit Discipline

Commit often, atomically, and with messages a careful human would write. Commits are durable review units, not an afterthought.

## Natural commit points

Commit when the working tree is one coherent, internally consistent unit:

- A failing test, its implementation, and its verification are complete.
- A self-contained refactor is complete and its checks pass.
- A proposal, guide, policy, decision, or configuration change is complete and verified.

Do not commit half-edited, incomplete, or red work. Finish the thought — or stash and pivot — then commit.

## One logical change

One commit contains one logical change. If its description needs “and also,” split it. Keep unrelated dependency updates, formatting sweeps, and cleanup in separate commits or drop them.

## Scoped Commits

This repository uses [Scoped Commits](https://scopedcommits.com/), not Conventional Commits. Every ordinary subject is:

```text
<scope>: <imperative description>
```

Write an imperative, present-tense description: `add`, `fix`, `remove` — not `added` or `adds`. Keep the subject under roughly 72 characters, specific, and without a trailing period. Use an optional body for motivation or non-obvious trade-offs; wrap it at roughly 72 characters.

Do not write Conventional Commits `type(scope):` subjects. Scoped Commits lead with the area being changed.

The allowed scopes are:

| Scope | Use for |
| --- | --- |
| `proposal` · `policy` · `decision` | The corresponding record type |
| `guides` · `docs` | Guides and rendered documentation |
| `tools` · `ci` · `vite` | Tooling, workflows, and task definitions |
| `agents` · `skills` · `rules` · `templates` · `hooks` | Agent process infrastructure |
| `readme` · `vision` | Project overview and direction |
| `treewide` | A genuinely repository-wide change with no narrower home |
| `<NNNN>-<slug>` | Work implementing that real proposal, such as `0007-export-queue` |

Product scopes extend this vocabulary through `.agents/commit-scopes`, one scope per line. Prefer the narrowest scope that accurately describes the change; use `treewide` only for a true sweep.

Examples:

- `proposal: clarify release preview ownership`
- `guides: explain the readiness report`
- `hooks: enforce scoped commit subjects`
- `0007-export-queue: reject duplicate export jobs`

Mechanical merge, revert, fixup, and squash commits are exempt. The `commit-msg` hook enforces ordinary subjects and scopes on commits made from Oh My Pi, and CI checks every commit in a pull request.

## Staging

- Never use `git add .` or `git add -A`. Stage explicit paths only.
- Run `git status` before staging. Decide exactly what belongs in this commit.
- Run `git diff --staged` before committing. Unstage anything unexpected.
- Preserve the user's uncommitted changes. Do not discard, overwrite, stage, or reformat work you did not make.

## Commit hooks and history

- If a pre-commit hook fails, the commit did not happen. Fix the issue, re-stage, then create a new commit.
- Never bypass hooks with `--no-verify` unless the human explicitly asks.
- Default to a new commit. Amend only when the previous commit is local, the correction belongs to the same logical change, and amending makes history clearer.
- Never rewrite pushed history. Correct it with a new commit. A force-push is a history rewrite and requires explicit human confirmation.

## Never commit

- Secrets: environment files, credentials, tokens, and keys.
- Build output and generated artifacts the project does not track.
- Personal editor configuration unless the human explicitly asks.
- Large binaries unless the project explicitly tracks them.

If an unexpected secret appears in `git status`, stop and warn the human.

## Frequency

Prefer many small, focused commits to one large commit. Commit at each natural green boundary before beginning the next independent change.

## Push policy

Pushing an authorized feature branch to `origin` creates and updates the draft pull request that serves as the durable workspace. Direct human consent or an active, applicable human-approved build mandate must authorize that workspace. Push early and push often within that authority.

Pushing directly to the default branch is prohibited. Work reaches the default branch only by merging its pull request.

PR merges require recorded acceptance and explicit authority to merge into the target branch, from the human or [an active, applicable human-approved build mandate](../../AGENTS.md#bounded-build-mandates). Delegated acceptance alone is not a merge grant. Fresh integration checks and independent review evidence are required; an explicit mandate merge grant replaces repeated per-feature confirmation.

Force-pushing, deleting a branch, publishing a release, changing access, and other destructive actions still require explicit human authorization for that action. Permission to merge grants none of these.
