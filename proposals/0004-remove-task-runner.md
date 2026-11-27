---
id: proposal.0004
title: Remove the Task Runner
authors: [Mark]
status: awaiting-implementation
pull-request: https://github.com/bento-js/bento/pull/268
issues: [https://github.com/bento-js/bento/issues/170]
supersedes: []
---

# Remove the Task Runner

## Summary

Remove Bento's built-in task runner: the `bento_task` crate, `package.json` `tasks` parsing, `bento run <task-name>`, and `BENTO_ENV_FILE_KEYS`. `bento run` executes source files only, leaving task orchestration to whichever task runner a project chooses.

## Motivation

Bento currently owns a task runner in `crates/bento_task` (~1,280 lines of source and ~1,225 lines of tests, plus CLI dispatch, manifest validation, target selection, and a dedicated guide) so `bento run <name>` can execute `package.json` `tasks` graphs. In practice, dedicated task runners—such as Mise, which Bento's own repository and author already use—coordinate toolchains, environments, and task graphs without coupling task orchestration to the JavaScript runtime.

Owning a second task runner inside Bento adds substantial maintenance and compatibility surface without adding runtime value:

- **Shell-parsing liability.** Because an outer task launch loads `.env` files before running `/bin/sh -c`, `bento_task` statically lexes POSIX shell words, `export`/`readonly`, `env -S`, `command`/`builtin`/`exec`/`nohup`, line continuations, and here-documents just to guess which environment variables a shell command reassigned before a nested `bento` launch (`BENTO_ENV_FILE_KEYS`). Two open defects ([#242](https://github.com/bento-js/bento/issues/242), [#243](https://github.com/bento-js/bento/issues/243)) already sit in that heuristic, and no static lexer can match `/bin/sh` across `eval`, sourced scripts, or arbitrary wrapper utilities.
- **Bespoke manifest vocabulary.** Top-level `tasks` in `package.json` is not a standard JavaScript ecosystem field—neither `scripts` (used by npm, pnpm, Bun, and `node --run`) nor an external task runner's own configuration. Furthermore, `Manifest::read` rejects any `package.json` whose `tasks` field is not a JSON object, even when `bento install` or `bento run server.ts` never runs a task.
- **Target ambiguity.** `bento run <target>` checks `package.json` `tasks` from the working directory before checking whether `<target>` is a bare script filename, and switches project-root discovery rules depending on whether `<target>` resolved to a task or a script.
- **Roadmap distraction.** Ten open roadmap issues ([#170](https://github.com/bento-js/bento/issues/170)–[#179](https://github.com/bento-js/bento/issues/179)) commit Bento to building task caching, file tasks, task-specific environments, automatic dependency installation, complex dependency graphs, and monorepo workspace tasks—features dedicated task runners already provide.

There is no clean workaround while the feature remains in Bento: every change to CLI flags, permissions, environment loading, or documentation must continue to design, test, and explain task-runner interactions alongside the runtime.

## Domain grounding

### Established context

- Dedicated task runners (`make`, `just`, [Mise](https://mise.jdx.dev/tasks/), `pnpm run`, `nx`, `turbo`) solve task orchestration independently of any single language runtime. Mise, which Bento's own repository uses, supports `depends`, sequential command arrays, argument forwarding, per-task `env`, file-input/output caching (`sources`/`outputs`), monorepo task discovery, and `PATH` injection (such as `_.path = ["{{config_root}}/node_modules/.bin"]`).
- Node.js separates application execution (`node [options] script.js [arguments]`) from task orchestration; even its minimal [`node --run`](https://nodejs.org/api/cli.html#--run) flag is a separate option over standard `package.json` `scripts` rather than a dependency-graph engine mixed into positional file execution.
- `package.json` has no cross-ecosystem standard for a top-level `tasks` object; leaving task configuration to the user's chosen task runner avoids maintaining a Bento-only manifest dialect, and Bento itself has zero coupling to which task runner invokes it.
- Under `proposal.0003` (approved for implementation on [PR #259](https://github.com/bento-js/bento/pull/259)), `bento run` is unrestricted by default, `--permission` opts into Node's permission model, and restricted guest subprocesses inherit `BENTO_PERMISSION_CEILING` (renamed from `BENTO_GRANT_CEILING`). Task commands are no longer needed as an escape hatch to spawn tools or servers.

### Relevant constraints and principles

- `VISION.md` centers Bento on "an embeddable capability and development tooling a set of cooperating services around it," and states in its principles and non-goals: "Own the integration, not every dependency" and "Not a new package ecosystem... Bento-specific configuration should be the exception, not a new universe of manifests." A general-purpose shell task runner neither shares the runtime's module graph nor benefits from running in-process.
- `VISION.md` currently lists a "task execution interface" in its Minimum Viable Product and mentions tasks in its introduction, and `ARCHITECTURE.md` lists `bento_task` and `package.json` `tasks`. Removing the task runner narrows Bento's product boundary, so completion updates `VISION.md` and `ARCHITECTURE.md` to match.
- This proposal assumes `proposal.0003` ([PR #259](https://github.com/bento-js/bento/pull/259)) merges before implementation begins, and preserves the permission ceiling mechanism (`BENTO_GRANT_CEILING` on `main`, `BENTO_PERMISSION_CEILING` after `proposal.0003`) for nested `bento` processes.

### Quality bar

- `bento run` has one unambiguous contract: executing a JavaScript, TypeScript, JSX, or TSX file in Bento's runtime.
- Zero task-runner code, shell-assignment heuristics, `BENTO_ENV_FILE_KEYS` plumbing, or `package.json` `tasks` schema coupling remains in the workspace.
- Bento has no runtime or CLI dependency on Mise or any other task runner, while the Remix guest-book fixture's own developer workflow continues to run end to end through its `mise.toml` tasks calling `bento` and local package tools directly.

### Remaining uncertainty

None.

## Existing baseline

Today on `main`, task execution spans four layers:

1. **`bento_workspace`:** `Manifest` parses top-level `tasks` from `package.json` and rejects non-object `tasks` (`ManifestProblem::InvalidTasks`). `select_target` resolves `bento run <target>` to either `Target::Task` or `Target::Script`, giving exact task names precedence over same-named files without extensions or path separators. `provenance.rs` implements `BENTO_ENV_FILE_KEYS` (64-bit FNV-1a value fingerprints) so an outer task launch's `.env` values can be overridden by a nested watch launch's `.env.<mode>`.
2. **`bento_task`:** Validates task shapes (`string`, `string[]`, `{ command?, dependsOn? }`) and cycle-free dependency plans (`Plan`), analyses shell commands for variable assignments (`assignments.rs`, `env_arguments.rs`, `shell_words.rs`, `here_document.rs`), prepends a temporary symlink directory for the current `bento` binary plus `<root>/node_modules/.bin` to `PATH`, injects `BENTO_GRANT_CEILING` and `BENTO_ENV_FILE_KEYS`, spawns `/bin/sh -c` in a dedicated process group, and forwards `SIGINT`/`SIGTERM` with a 10-second `SIGKILL` grace period.
3. **`bento_cli`:** `arguments.rs` advertises `<script-path | task-name>`, `run.rs` branches on `Target`, `task.rs` enforces `--` before task arguments and drives `TaskRunner`, and `error.rs` formats `PlanError`, `TaskError`, `WatchedTask`, and `TaskArgumentsWithoutSeparator`.
4. **Fixture and docs:** `fixtures/remix-guest-book/package.json` defines `dev`, `start`, `test`, `fmt`, `lint`, `typecheck`, `check`, `check:fix`, and `db:*` under `tasks`. `guides/tasks-and-environment.md` documents both task execution and `.env` loading.

Worth keeping:

- `Project::for_script` and `Project::for_directory` (still used by `bento install` and tests).
- `Manifest` reading `type` and `devEngines.packageManager`, ignoring unknown fields in `package.json`.
- `LaunchEnvironment::load` reading `.env`, `.env.<mode>`, and `.env.local` from the script's project root beneath inherited process variables, with watch mode defaulting `NODE_ENV` to `development`.
- Permission ceiling decoding and narrowing (`BENTO_GRANT_CEILING` / `BENTO_PERMISSION_CEILING`), which `proposal.0003` ([PR #259](https://github.com/bento-js/bento/pull/259)) uses when restricted guest instances spawn child processes.
- `error::shell_status` in `bento_cli`, which `bento install` still uses to report `pnpm` exit statuses.

Incidental and removed by this proposal:

- The entire `bento_task` crate.
- `Manifest::tasks` and `ManifestProblem::InvalidTasks`.
- `Target`, `Selection`, `TargetError`, and `select_target` in `bento_workspace`.
- `provenance.rs`, `FILE_KEYS_ENV` (`BENTO_ENV_FILE_KEYS`), `InvalidFileKeys`, `EnvironmentError::FileKeys`, and `LaunchEnvironment::file_keys_value`.
- Task dispatch, task-specific CLI errors, and task-runner wording in `bento_cli`.

## Proposed solution

Remove task orchestration from Bento and let the user choose their project task runner independent of Bento. `bento run` takes a script path and its arguments, discovers the enclosing project from the script's location, loads `.env` files for that launch, and runs the script in Bento's runtime:

```text
bento run [OPTIONS] [--watch] <script-path> [args...]
bento install [--frozen-lockfile]
```

Bento does not read or privilege any task runner's configuration. A project coordinates workflows such as database migrations, formatting, linting, type checking, or watch servers with whatever tool its author prefers—`pnpm`, `just`, `make`, shell scripts, or Mise. In this repository's Remix guest-book fixture, `mise.toml` defines those tasks and invokes `bento` alongside package tools:

```toml
[env]
_.path = ["{{config_root}}/node_modules/.bin"]

[tasks."db:migrate"]
run = "remix db migrate"

[tasks.dev]
depends = ["db:migrate"]
run = "bento run --watch server.ts"
```

## Detailed design

### CLI command line and target resolution

- `bento run` accepts a `<script-path>` (value name `SCRIPT` in `--help`) followed by optional script arguments:
    - `Cli` about text changes from "Run TypeScript applications and project tasks." to "Run TypeScript and JavaScript applications."
    - `Command::Run` help text changes to "Run a JavaScript or TypeScript source file".
    - `RunArguments` positional help text names the script path and its trailing arguments without mentioning tasks or `--` task separators.
- Every word after `<script-path>` is passed verbatim to the guest script in `process.argv.slice(2)`, including a leading `--` when given.
- `<script-path>` is resolved relative to the invocation directory (`env::current_dir()`), never looked up in `package.json` `tasks` or `scripts`, and never executed as a shell command:
    - `Launch::prepare` resolves the inherited permission ceiling and validates CLI permission flags before touching `<script-path>` or reading `package.json`, so malformed or wider-than-ceiling flags fail closed with status `1` before any filesystem target lookup or guest execution.
    - Once permission flags are valid, `Project::for_script(&invocation_directory.join(script))` locates the script and its enclosing `package.json`. If `<script-path>` does not exist or is not a regular file (after following symlinks), `bento run` exits with status `1` and reports `error: no script file at <path>` before loading `.env` files or starting V8.
- `bento run --watch <script-path>` watches and restarts the script as it does today. `CliError::WatchedTask` and `CliError::TaskArgumentsWithoutSeparator` are removed because every target is a script.

### Workspace crate (`bento_workspace`)

- Delete `crates/bento_task` from the filesystem, workspace `Cargo.toml` dependencies, `crates/bento_cli/Cargo.toml`, and `.agents/commit-scopes` (removing the `tasks` commit scope).
- In `bento_workspace`:
    - Remove `target.rs` (`Target`, `Selection`, `TargetError`, `select_target`).
    - Remove `tasks` from `Manifest` and `InvalidTasks` from `ManifestProblem`. `Manifest::read` ignores any `tasks` field in `package.json`, regardless of its JSON type, just as it ignores `scripts` and other fields Bento does not act on.
    - Remove `provenance.rs`, `FILE_KEYS_ENV`, `InvalidFileKeys`, `EnvironmentError::FileKeys`, and `LaunchEnvironment::file_keys_value`. `LaunchEnvironment::load` treats every variable in `inherited` as an inherited process variable that overrides `.env`, `.env.<mode>`, and `.env.local`. `BENTO_ENV_FILE_KEYS` in the process environment has no special meaning and is neither parsed nor stripped by `bento_workspace`.
    - Retain `LaunchEnvironment::file_keys()` so callers (including `proposal.0003`'s restricted-environment filter) can inspect which keys in `vars()` came from `.env` files rather than inherited process variables.

### Permission ceiling preservation

- Keep the inherited ceiling check in `bento_host` and `bento_cli` (`BENTO_GRANT_CEILING` on `main`, renamed `BENTO_PERMISSION_CEILING` by `proposal.0003` on [PR #259](https://github.com/bento-js/bento/pull/259)).
- Because Bento no longer runs tasks, nothing in `bento_cli` sets the ceiling for task commands; under `proposal.0003`, restricted runtime instances set `BENTO_PERMISSION_CEILING` on child processes they spawn, and nested `bento run` invocations continue to read and enforce it.
- CLI tests that exercise ceiling inheritance and narrowing pass the ceiling environment variable directly to `bento run <script>` (or spawn a child `bento` from a guest script once `proposal.0003` lands) rather than going through a `package.json` task.

### Remix guest-book fixture (`fixtures/remix-guest-book`)

- Remove the top-level `"tasks"` object from `fixtures/remix-guest-book/package.json`. Keep `"scripts"`, `"dependencies"`, `"devDependencies"`, `"imports"`, `"devEngines"`, `"engines"`, `pnpm-lock.yaml`, and `pnpm-workspace.yaml` unchanged.
- Add `fixtures/remix-guest-book/mise.toml` as that fixture's chosen task runner configuration (matching Bento's own repository tooling, without giving Mise any special status inside Bento):
    - Add `{{config_root}}/node_modules/.bin` to `PATH` via `[env]` `_.path = ["{{config_root}}/node_modules/.bin"]`.
    - Define the tasks previously under `package.json` `tasks`:
        - `db:migrate` (`remix db migrate`), `db:reset` (`remix db reset --force`), `db:rollback` (`remix db rollback`), `db:status` (`remix db status`).
        - `dev` (`depends = ["db:migrate"]`, `run = "bento run --watch server.ts"`).
        - `start` (`run = "bento run server.ts"`).
        - `test` (`run = "NODE_ENV=test remix test"`).
        - `fmt` (`run = "oxfmt"`), `lint` (`run = "oxlint --fix"`), `typecheck` (`run = "tsc"`).
        - `check` (`run = ["oxfmt --check", "oxlint", "tsc"]`).
        - `check:fix` (`run = ["oxfmt", "oxlint --fix", "tsc"]`, run sequentially rather than via parallel `depends` because `oxfmt` and `oxlint --fix` rewrite the same source files).
- Update `fixtures/remix-guest-book/ATTRIBUTION.md` to record that `package.json` dropped the Bento-specific `tasks` block and `mise.toml` was added for task coordination, while keeping the imported `remix@3.0.0` dependency graph and lockfile byte-for-byte identical.
- Update `fixtures/remix-guest-book/README.md` to show `bento run --watch server.ts`, `bento run server.ts`, and `mise run <task>` instead of `bento run <task>`.
- Update `crates/bento_cli/tests/fixture.rs` so Bento's CLI integration tests exercise `bento run server.ts`, `bento run --watch server.ts`, `.env` / `.env.development` loading on `bento run`, and `bento install --frozen-lockfile` directly, plus a check that `mise run check` and `mise run db:migrate` succeed in the installed fixture copy.

### Documentation, architecture, vision, and roadmap cleanup

- Rename `guides/tasks-and-environment.md` to `guides/environment-files.md` (`id: guide.environment-files`, `title: Environment Files`, `describes: [proposal.0001, proposal.0002, proposal.0003, proposal.0004]`), covering `.env`, `.env.<mode>`, `.env.local`, mode selection, precedence, watch-mode `NODE_ENV=development`, and secret redaction in syntax diagnostics, with all task-runner and `BENTO_ENV_FILE_KEYS` sections removed.
- Update `guides/running-with-bento.md`, `guides/installing-dependencies.md`, `guides/typescript-and-jsx.md`, `guides/node-compatibility.md`, `guides/prompt-api.md`, `README.md`, and `ARCHITECTURE.md` to remove `bento_task`, `package.json` `tasks`, `bento run <task>`, and `BENTO_ENV_FILE_KEYS`. User-facing guides describe invoking `bento` and package tools directly or from the user's preferred task runner rather than prescribing Mise; repository and fixture docs (`README.md`, `fixtures/remix-guest-book/README.md`) show the `mise run` commands those projects actually configure.
- During Phase 5 completion (per `PROCESS.md`), update `VISION.md` to remove task execution from the introduction and Minimum Viable Product and add a revision-history entry for `proposal.0004`.
- Close the superseded GitHub issues when this proposal ships: roadmap issues [#170](https://github.com/bento-js/bento/issues/170)–[#179](https://github.com/bento-js/bento/issues/179) and `bento_task` bug issues [#242](https://github.com/bento-js/bento/issues/242) and [#243](https://github.com/bento-js/bento/issues/243).

## Compatibility

This is an intentional breaking change to Bento's CLI, `bento_workspace` Rust API, and project manifest handling. No stored user data is affected.

- **CLI:** `bento run <task-name>` and `bento run <task-name> -- <args>` no longer look up or execute `package.json` `tasks`; `bento run <arg>` always resolves `<arg>` as a script file path and fails with `no script file at ...` when no file exists there. Projects move `package.json` `tasks` to their preferred task runner (such as `mise.toml` or `package.json` `scripts`) and pass script paths to `bento run`.
- **Environment variables:** `BENTO_ENV_FILE_KEYS` is no longer set or interpreted. Each `bento run` process loads `.env`, `.env.<mode>`, and `.env.local` beneath its inherited process environment; variables already exported in the parent process override `.env` files as normal.
- **Manifest handling:** `package.json` `tasks` is ignored rather than parsed or validated. Existing `package.json` files that still contain a `tasks` field remain valid and do not fail `bento run` or `bento install`.
- **Rust crates:** `bento_task` is deleted. `bento_workspace` removes `Manifest::tasks`, `ManifestProblem::InvalidTasks`, `Target`, `Selection`, `TargetError`, `select_target`, `FILE_KEYS_ENV`, `InvalidFileKeys`, `EnvironmentError::FileKeys`, and `LaunchEnvironment::file_keys_value`.
- **Earlier proposals:** Replaces the task-runner and `BENTO_ENV_FILE_KEYS` portions of [proposal 0001](0001-embeddable-typescript-runtime.md) and the task-command ceiling/environment references in [proposal 0002](0002-web-prompt-api.md) and `proposal.0003` ([PR #259](https://github.com/bento-js/bento/pull/259)). None of those proposals is superseded as a whole.

## Implications on adoption

Projects using `bento run <task>` move task definitions to whichever task runner they already use (or invoke `bento run <script>` directly). Bento imposes no task-runner requirement on adopters; Mise is used in this repository and its guest-book fixture only as the author's choice for those projects. Removing `bento_task` shrinks the Rust workspace by one crate and removes `/bin/sh` process-group orchestration and shell-lexing code from the CLI binary. Reverting the change would only require restoring the deleted crate and `tasks` manifest field; no persistent state changes.

## Scope

- Delete `crates/bento_task` and all task-runner modules, errors, and tests in `bento_cli` and `bento_workspace`.
- Simplify `bento run` target handling to script paths only (`Project::for_script`).
- Remove `Manifest::tasks`, `ManifestProblem::InvalidTasks`, and `BENTO_ENV_FILE_KEYS` provenance tracking from `bento_workspace`.
- Migrate `fixtures/remix-guest-book` from `package.json` `tasks` to `mise.toml`, updating its `README.md`, `ATTRIBUTION.md`, and integration tests.
- Replace `guides/tasks-and-environment.md` with `guides/environment-files.md` and update all guides, `README.md`, `ARCHITECTURE.md`, and `.agents/commit-scopes`.
- Close GitHub issues `#170`–`#179`, `#242`, and `#243` upon completion, and update `VISION.md` in Phase 5.

### Out of scope

- Running `package.json` `scripts` from Bento (for example, a `node --run` equivalent). Task and script orchestration belongs to the user's chosen task runner or package manager.
- Accepting Node's CLI flags or `package.json` bin executables directly as `node` ([#260](https://github.com/bento-js/bento/issues/260)).
- Changing `.env` file syntax, mode precedence, or adding Node-style `--env-file` flags ([#180](https://github.com/bento-js/bento/issues/180)).
- Changing the permission model or child-process permission ceilings established by `proposal.0003` ([PR #259](https://github.com/bento-js/bento/pull/259)).

## Preview

- Artifact: the macOS arm64 `bento` executable archive that CI publishes for this pull request, together with the updated documentation preview on `gh-pages` and `fixtures/remix-guest-book/mise.toml`.
- Reason: `bento` is a CLI and documentation repository. Installing the CI-built `bento` binary lets the reviewer verify that `bento run server.ts` and `bento run --watch server.ts` work, that `bento run dev` fails cleanly as a missing file rather than running a shell or task, that `mise run dev` and `mise run check` work inside `fixtures/remix-guest-book`, and that the rendered `Environment Files` and `Running with Bento` guides match the CLI's behavior.

## Policies and decisions checked

- `policy.0001` (prefer VoidZero crates): unaffected. Removing `bento_task` adds no crates and touches no JS/TS parsing, transformation, resolution, or bundling role.
- `decisions/`: contains no decision records.
- `VISION.md`: aligns with "Own the integration, not every dependency" and "Not a new package ecosystem," while intentionally narrowing the MVP's "task execution interface" bullet. Phase 5 updates `VISION.md` to record that task orchestration is left to external task runners independent of Bento.

## Future directions

- Accepting Node's command-line conventions ([#260](https://github.com/bento-js/bento/issues/260)) will let external task runners and `node_modules/.bin` executables invoke Bento in place of `node` without Bento owning task scheduling itself.
- Built-in commands for Bento's own integrated toolchain (`typecheck`, `fmt`, `lint`, `test`, `dev`, `build`) remain independent CLI subcommands over Bento's shared workspace services rather than shell task-runner entries.

## Alternatives considered

- **Keep `bento run` support for `package.json` `scripts` (like `node --run`).** Rejected: external task runners and package managers (`pnpm run`, `npm run`, `mise run`, `just`) already execute project commands; keeping a second shell runner in `bento run` retains target ambiguity (`bento run build` vs `./build`), process-group management, and shell/PATH edge cases for no runtime benefit.
- **Remove CLI task dispatch only, keeping `bento_task` as an internal crate.** Rejected: nothing else in the workspace uses `bento_task`, and keeping ~1,280 lines of dead task planning, shell lexing, and process-group code violates the repository's dead-code and maintenance discipline.
- **Remove automatic `.env` loading alongside tasks.** Rejected: loading `.env`, `.env.<mode>`, and `.env.local` for `bento run <script>` is small, self-contained in `bento_workspace`, has no shell-parsing edge cases once `BENTO_ENV_FILE_KEYS` is gone, and is already integrated with `proposal.0003`'s environment permission rules.
- **Remove the inherited permission ceiling (`BENTO_GRANT_CEILING` / `BENTO_PERMISSION_CEILING`) as well.** Rejected: `proposal.0003` uses `BENTO_PERMISSION_CEILING` to bound nested `bento` launches spawned via `node:child_process` from restricted guest instances. Keeping the ceiling check preserves that invariant cleanly.
- **Leave the unused `tasks` block in `fixtures/remix-guest-book/package.json`.** Rejected: replacing `tasks` with `mise.toml` in the guest-book fixture keeps the fixture's developer tasks runnable with the repository's existing task runner rather than leaving dead Bento-only configuration behind.

## Open questions

None.
