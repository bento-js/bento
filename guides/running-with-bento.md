---
id: guide.running-with-bento
title: Running with Bento
describes: [proposal.0001]
---

# Running with Bento

## What it does

The `bento` command runs your project. `bento run` starts a JavaScript, TypeScript, JSX, or TSX source file, or a task declared in the top-level `tasks` field of `package.json`, with exactly the capabilities you grant it. `bento install` installs the project's dependencies through its declared pnpm toolchain.

```text
bento run [GRANTS] [--watch] <script-path | task-name> [args...]
bento install [--frozen-lockfile]
```

Bento options go before the target. Everything after the target belongs to the script or task.

## How to use it

### Choose what to run

Bento decides whether the target is a file or a task:

- A target containing `/` is a file: `bento run ./server.ts`, `bento run scripts/seed.ts`.
- A target with a file extension is a file when that file exists: `bento run server.ts`.
- Otherwise a task with exactly that name wins, even over a file of the same name.
- Anything else is an error that names the target. Bento never runs it as a shell command.

A task's project is the nearest directory, from the current one upward, that holds a `package.json`. A script's project is the nearest one from the script's own directory upward. [Tasks and Environment](tasks-and-environment.md) explains how tasks are declared, ordered, and run.

### Pass arguments

Words after a script path are the script's own arguments, passed through unchanged: `bento run server.ts --port 3000`.

Arguments for a task go after `--`, and Bento appends them, shell-quoted, to the task's own command: `bento run test -- --reporter "dot matrix"`. Dependencies never receive them. Bento rejects task arguments without the `--`, so a misplaced option such as `bento run dev --watch` fails instead of being passed on silently. A task that has only `dependsOn`, or an array of commands, has no single command to receive arguments, so it rejects them too. In each case Bento says why and runs nothing.

### Grant capabilities

Guest code can use only what the launch grants it. Without any grant flag, a script may not read or write files, use the network, see environment variables, or load native addons.

| Flag | Grants | Items |
| --- | --- | --- |
| `--allow-read[=PATH,...]` | Reading files and directories | Paths; each covers everything beneath it |
| `--allow-write[=PATH,...]` | Creating, changing, and removing files | Paths; each covers everything beneath it |
| `--allow-net[=HOST[:PORT],...]` | Connecting and listening | A host covers all its ports; write IPv6 hosts in brackets, as in `[::1]:8080` |
| `--allow-env[=NAME,...]` | Seeing and setting environment variables | Exact variable names |
| `--allow-addons[=PATH,...]` | Loading native Node-API addons | Paths; each covers everything beneath it |

A flag without `=` grants the whole kind: `--allow-net` allows every host. With `=`, it grants only the comma-separated items. Give each flag at most once; list several items in one flag instead. An empty item, such as `--allow-read=` or `--allow-env=PORT,,HOST`, and a malformed network target are errors.

Relative paths resolve from the directory you run `bento` in, not from the project root. Bento then resolves symlinks, so a grant names the real location: granting `./data` when `data` is a link to `/srv/data` grants `/srv/data`. A path that does not exist yet, such as a database file the application will create, is normalized as written beneath its nearest existing directory.

No flag can let guest code start subprocesses or import remote modules. An addon grant is different from the others: an addon runs native code inside the Bento process, so once loaded it is not contained by the file, network, or environment grants. Grant addons only from packages you trust.

Grants govern guest code. They do not restrict task commands or the tools those commands start, which run with your user's full authority.

### Nested `bento` commands inherit grants as a ceiling

When a task command runs `bento` again, the nested launch receives the grants of the launch that started the task through `BENTO_GRANT_CEILING`. The name `bento` in a task command always means that same running `bento` executable, even when the project installs its own `bento` in `node_modules/.bin`, because Bento puts a link to itself first on the task's `PATH`.

- A kind the nested command does not mention is inherited unchanged.
- A nested flag may narrow its kind: `--allow-read=app` under an inherited `--allow-read=.` is fine.
- A nested flag that asks for more than the inherited grant fails before anything runs. The error names the flag and the inherited limit, and the task that ran the nested command fails with it.

A launch started directly from your shell has no ceiling, so any kind its flags leave out is denied.

### Load the environment

Before running a script or task, Bento loads the project's `.env`, `.env.<mode>`, and `.env.local` files beneath the variables you set yourself. The mode comes from your `NODE_ENV`, else the `NODE_ENV` in `.env`; `bento run --watch` defaults it to `development` when neither sets it. [Tasks and Environment](tasks-and-environment.md) describes the file syntax, the precedence, and how nested launches choose their own mode file.

Loading a variable does not grant guest code access to it; `--allow-env` does. Task commands receive the whole prepared environment.

### Run a script

> This section describes behavior that the next stage of the Bento CLI completes. Until then, `bento run <script-path>` selects the script, checks its grants, and loads its environment, then exits with status 1 and the message `script execution requires the runtime`. It never starts Node, Deno, Bun, or another runtime in its place.

`bento run server.ts` executes the file directly. TypeScript, JSX, and TSX need no build step, no prebuilt JavaScript entrypoint, and no `--import remix/node-tsx`. Bento follows the execution settings in `tsconfig.json` and the files it extends, such as `jsx` and `jsxImportSource`, and reports an execution-affecting setting it cannot honor rather than choosing another JSX runtime. It does not type-check; run your type checker as a task.

Modules resolve the way Node resolves them for the project: `package.json` `imports` and `exports`, ESM and CommonJS packages, and pnpm's `node_modules` layout. `tsconfig.json` `paths` never change resolution. Execution never installs or downloads anything; a missing package names the import that failed and points you to `bento install`.

Top-level `await` works, and the script runs until it has no work left or calls `process.exit`. Startup, module resolution, permission, and uncaught errors are reported as such, with source locations in your TypeScript, and end the run unsuccessfully.

### Watch for changes

> This section describes behavior that the next stage of the Bento CLI completes. Until then, `bento run --watch <script-path>` fails the same way as `bento run <script-path>`.

`bento run --watch server.ts` runs the script and restarts it completely when its source changes; it does not replace modules in place. Bento watches the entrypoint, every local module it loads, including ones loaded after startup, the configuration that affects resolution and transformation, and the application's source directories, so browser modules compiled by `remix/assets` count too. Edits, replaced files, deletions, and a newly created file that fixes a missing import all trigger a restart.

A burst of edits causes one restart. The new instance starts only after the old one has shut down and released its port. When loading fails, Bento reports the error and keeps watching, so fixing the file recovers; it never keeps serving the old code as though the reload had worked. Databases, SQLite side files, `node_modules`, generated output, and caches are not watched, so requests that write to them do not cause restarts.

Watching does not reload `.env` files and does not rerun the tasks that started it. When the guest book's `dev` task starts `bento run --watch server.ts`, migrations run once, before the watcher starts. Stopping the command stops both the application and the watching.

`--watch` applies to scripts only. `bento run --watch <task-name>` is an error; declare a task whose command runs `bento run --watch <script>`, as the guest book's `dev` task does.

### Install dependencies

`bento install` runs `pnpm install` in the project root, including `devDependencies`. `bento install --frozen-lockfile` reproduces the locked graph exactly and fails, without touching `pnpm-lock.yaml`, when it disagrees with `package.json`. [Installing Dependencies](installing-dependencies.md) covers the details.

### Stop a run

When you press Ctrl-C or send `bento` a termination signal while a task runs, Bento forwards the signal to the task command's whole process group, including servers started by nested `bento` commands. Processes still running after a 10-second grace period are killed. `bento` exits once nothing from the task is left running, so its port is free again. The same holds while Bento is stopping processes a finished command left running in the background: the signal reaches them too, and `bento` exits with `130` or `143` rather than the command's own status.

When the runtime runs a script (next stage), the signal reaches the script's `process.on("SIGINT")` and `process.on("SIGTERM")` listeners. Without a listener, Bento shuts the script down.

### Exit codes

| Code | When |
| --- | --- |
| `0` | The script or task finished successfully. |
| The script's code | The script called `process.exit(code)` (next stage). |
| The command's code | A task command failed; a command killed by signal `N` gives `128 + N`, as shells report it. |
| pnpm's code | `pnpm install` failed. |
| `130` | An interrupt (SIGINT, Ctrl-C) stopped the run. |
| `143` | A termination signal (SIGTERM) stopped the run. |
| `1` | Any other failure, reported with a message: an unknown target, a usage error, a malformed or too-wide grant flag, a malformed environment file, an invalid task graph, or misplaced arguments. |

## Worked example

The guest book needs to read its project, write its database directory, listen on the network, see three variables, and load the native compilers installed in `node_modules`:

```sh
cd fixtures/remix-guest-book
bento install --frozen-lockfile
bento run --allow-read=. --allow-write=./db --allow-net --allow-env=PORT,DATABASE_URL,NODE_ENV --allow-addons=./node_modules dev
```

`bento run ... dev` runs the `db:migrate` task once, then the `dev` command `bento run --watch server.ts`. That nested launch names no grant flags, so it runs with exactly the five grants above. Ctrl-C stops the watcher, the server, and its listener, and `bento` exits with `130`. Until the next stage of the CLI completes script execution, the nested launch stops after checking its grants and environment with `script execution requires the runtime`, and the `dev` task fails with status 1.

If a task in the same project ran `bento run --allow-read=/ server.ts` under those grants, the nested launch would stop before running anything with an error like:

```text
error: --allow-read=/ asks for more than the inherited read grant: /Users/you/guest-book
```

## Limits and edge cases

- Bento options after the target are not Bento options. They belong to the script, or make a task invocation fail because they come without `--`.
- A cancelled run exits with `130` or `143` even when the task's processes handle the signal and exit cleanly.
- Bento needs every environment variable it starts with to be valid UTF-8, and reports the first one that is not.
- Grants restrict guest code, not task commands. A task is a trusted operation on your behalf.
- A `BENTO_GRANT_CEILING` that Bento cannot read stops the launch instead of granting anything. Bento sets it for task commands; you do not set it yourself.
