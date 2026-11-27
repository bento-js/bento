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

The allowed scopes are the entries in [`.agents/commit-scopes`](../commit-scopes) — one per line, process scopes first, product scopes added as the project grows — plus `<NNNN>-<slug>` for work implementing that real proposal, such as `0007-export-queue`. That file is the only list; do not memorize a second one.

Prefer the narrowest scope that accurately describes the change; use `treewide` only for a true sweep. A change that needs a scope the file does not have adds the scope to the file in the same commit.

Examples:

- `proposal: clarify release preview ownership`
- `guides: explain the readiness report`
- `hooks: enforce scoped commit subjects`
- `0007-export-queue: reject duplicate export jobs`

Mechanical merge, revert, fixup, and squash commits are exempt. hk runs `node tools/check-scoped-commit.ts` as its `commit-msg` step, so every commit is checked regardless of which harness, editor, or human made it, and CI checks every commit in a pull request.

## Staging

- Never use `git add .` or `git add -A`. Stage explicit paths only.
- Run `git status` before staging. Decide exactly what belongs in this commit.
- Run `git diff --staged` before committing. Unstage anything unexpected.
- Preserve the user's uncommitted changes. Do not discard, overwrite, stage, or reformat work you did not make.

## Commit hooks and history

- hk owns every git hook; `mise run setup` installs them with `hk install --mise`. Never run another tool's hook installer, and never set `core.hooksPath` by hand.
- If a hook fails, the commit did not happen. Fix the issue, re-stage, then create a new commit.
- Never bypass hooks with `--no-verify` unless the human explicitly asks.
- Default to a new commit. Amend only when the previous commit is local, the correction belongs to the same logical change, and amending makes history clearer.
- Never rewrite pushed history. Correct it with a new commit. A force-push is a history rewrite and requires explicit human confirmation.

## Never commit

- Secrets: environment files, credentials, tokens, and keys.
- Build output and generated artifacts the project does not track.
- Personal editor configuration unless the human explicitly asks.
- Large binaries unless the project explicitly tracks them.

If an unexpected secret appears in `git status`, stop and warn the human. Secrets reach tasks through Fnox grants in `mise.toml`, never through a tracked file.

## Frequency

Prefer many small, focused commits to one large commit. Commit at each natural green boundary before beginning the next independent change.

## Changesets

Every pull request carries a changeset. Run `mise run changeset`, choose the bump, and write the entry for the person reading the changelog — not for the reviewer who already saw the diff. A change with no user-visible effect still carries one, created with `--empty`. The `.changeset/` file is committed with the work it describes; CI rejects a pull request without one, and the release pull request is exempt.

## Push policy

Pushing the feature branch to `origin` is expected and routine: it creates and updates the draft pull request that serves as the durable workspace. Push early and push often.

Pushing directly to the default branch is prohibited. Work reaches the default branch only by merging its pull request.

Force-pushing, deleting a branch, publishing a release, and merging require explicit human confirmation. Merging happens in phase 5 after human acceptance, not because the agent judges the work acceptable.
