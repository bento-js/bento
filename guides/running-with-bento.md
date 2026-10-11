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

### Get Bento

Bento is built from this repository for macOS on Apple silicon. Building needs the Rust toolchain pinned in `rust-toolchain.toml`, which `rustup` selects automatically. From the repository root:

```sh
cargo install --locked --path crates/bento_cli
```

This installs the `bento` executable into Cargo's bin directory (`~/.cargo/bin` by default). Running an installed project needs nothing else. Installing dependencies needs pnpm and Node, because `bento install` delegates to the project's declared pnpm, and the package tools a task runs, such as `oxfmt`, `tsc`, or `remix db migrate`, start through their own executables, which may be Node scripts.

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

Guest code can use only what the launch grants it. Without any grant flag, a script may not read or write files, use the network, see environment variables, or load native addons. Loading a module reads it, so even running a script needs `--allow-read` covering the script, the modules it imports, and the packages they come from; `--allow-read=.` in the project directory covers them all. Without it, Bento reports a permission denial for the script itself.

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

`bento run --allow-read=. server.mjs` executes the file in Bento's own runtime, inside the `bento` process. Nothing else needs to be installed to run it: Bento never starts Node, Deno, Bun, or pnpm, and runs the same with none of them on `PATH`.

`bento run server.ts` executes TypeScript, JSX, and TSX directly. They need no build step, no prebuilt JavaScript entrypoint, and no `--import remix/node-tsx`, and Bento writes no JavaScript next to your sources: it transforms each module in memory as it loads. Bento follows the execution settings in `tsconfig.json` and the files it extends, such as `jsx` and `jsxImportSource`, and reports an execution-affecting setting it cannot honor rather than choosing another JSX runtime. It does not type-check; run your type checker as a task.

Modules resolve the way Node resolves them for the project: `package.json` `imports` and `exports`, ESM and CommonJS packages, and pnpm's `node_modules` layout. Packages resolve from the script's project even when you run `bento` from another directory. `tsconfig.json` `paths` never change resolution. Execution never installs or downloads anything; a missing package names the import that failed and points you to `bento install`.

The script sees what a Node script would see of its launch, limited by its grants:

- `process.argv.slice(2)` holds the words after the script path, unchanged, as under Node.
- `process.cwd()` is the directory you ran `bento` in, not the project root.
- `process.env` holds the variables `--allow-env` grants, from your environment or the project's environment files. Other variables are invisible, not just unreadable.

Packages with Node-API native addons, such as the compilers `remix/assets` uses, load into the `bento` process when `--allow-addons` covers their installed location. Without that grant, loading the addon is a permission denial and its native code never runs.

An addon runs as native code in the `bento` process, so it sees the environment `bento` itself was started with, not the variables granted to the script. The Oxc resolver that `remix/assets` loads honors `NODE_PATH` there: launched with `NODE_PATH` pointing at another project's packages, it can resolve packages outside your project, which a scoped read grant then denies. Start `bento` without `NODE_PATH` set.

Top-level `await` works, and the script runs until it has no work left or calls `process.exit`. `process.exit(code)` ends the script with that exit code. When the script fails instead, Bento names the kind of failure: the script could not start, a module could not be resolved or transformed, permission was denied, or an exception went uncaught. It shows the error's message and where it happened, as [Read error reports](#read-error-reports) describes, and `bento` exits with status 1.

### Read error reports

When an error points at a place in a file Bento can read, Bento shows that place: the file with its line and column, the source lines around it, and a marker under the position saying what is wrong there. A `package.json` that is not valid JSON, for example, is reported like this:

```text
Error: /Users/you/guest-book/package.json is not valid JSON
   ╭─[ /Users/you/guest-book/package.json:4:3 ]
   │
 4 │   "tasks": {}
   │   ┬
   │   ╰── expected `,` or `}`
───╯
```

A script's failure is shown the same way when Bento knows where it happened: a syntax error, an uncaught exception, an import that cannot be resolved, or a denied operation. The place is in the source file as you wrote it, so a TypeScript or TSX error shows your TypeScript line, not the JavaScript Bento transformed it into. The marker says what happened there, such as `cannot be transformed here`, `thrown here`, `imported here`, or `denied here`. The stack trace and any errors that caused this one follow the quoted source:

```text
Error: uncaught exception: /Users/you/app/throw.ts: Uncaught (in promise) Error: no 42
   ╭─[ /Users/you/app/throw.ts:6:24 ]
   │
 6 │ const failure: Error = new Error(`no ${answer}`);
   │                        ┬
   │                        ╰── thrown here
───╯
    at file:///Users/you/app/throw.ts:6:24
```

`bento run --watch` reports a failed load or reload in the same form before it waits for the next change.

An error without such a place, or one whose file cannot be read, is a single line that starts with `error:`. Bento colors its reports only when standard error is a terminal, so redirected output and logs stay plain text, even where the runtime would style parts of an error message for your terminal.

Environment files are never shown, because their values are often secrets. An error in `.env` names the file and the line, as in `.env:2: quoted value is never closed`, and nothing of its contents.

### Watch for changes

`bento run --watch server.ts` runs the script and restarts it completely when its source changes; it does not replace modules in place. Bento watches the project directory, so browser modules compiled by `remix/assets` count even though the server never imports them. It also watches every module the script loads from outside the project, including ones first loaded after startup, such as by a dynamic `import()` while serving a request; Bento starts watching those within a fraction of a second of their loading. When it cannot watch one yet, for example because its directory has been moved away, it prints a warning once and keeps trying while the script runs. It watches the configuration that changes how modules resolve and transform, too: `package.json`, `tsconfig.json`, and every file `tsconfig.json` extends, even from another directory or a package. Bento reads `tsconfig.json` again on each change, so a file that an edit makes it extend is watched from the restart that edit causes. Edits, replaced files, deletions, and a newly created file that fixes a missing import all trigger a restart.

Inside the project, Bento ignores what running or building the application writes, so requests that write to them do not cause restarts:

- dependencies and version control: `node_modules` and `.git`;
- build output and caches: `dist`, `build`, `target`, `.cache`, and `coverage` directories, and `.DS_Store` files;
- databases and SQLite side files: `*.db`, `*.sqlite`, `*.sqlite3`, `*-wal`, `*-shm`, and `*-journal`;
- environment files: `.env` and `.env.*`, because a restart would not reload them.

A module the script actually loads from one of those places, such as an entrypoint in `dist`, is still watched. Modules loaded from a `node_modules` directory are not.

Edits within about a tenth of a second of each other cause one restart. Bento says on standard error which file changed each time it restarts. The new instance starts only after the old one has shut down, as it would for a termination signal, and released its port. When loading fails, Bento reports the error, says it is waiting for changes, and keeps watching, so fixing the file or creating the missing one recovers; it never keeps serving the old code as though the reload had worked. A script that ends by itself, by finishing its work or calling `process.exit`, does not end the watch either: Bento reports how it ended and runs it again after the next change.

Watching does not reload `.env` files and does not rerun the tasks that started it. When the guest book's `dev` task starts `bento run --watch server.ts`, migrations run once, before the watcher starts. `--watch` selects the `development` mode for the environment files and `NODE_ENV` unless your environment or `.env` sets `NODE_ENV`.

Stopping the command stops both the application and the watching. Ctrl-C or a termination signal stops the running script as [Stop a run](#stop-a-run) describes, and then `bento` exits instead of waiting for changes. When no script is running, because loading failed or the script ended by itself, `bento` exits at once with `130` or `143`. A nested `bento run --watch` that a task started stops the same way when you stop the outer `bento`.

`--watch` applies to scripts only. `bento run --watch <task-name>` is an error; declare a task whose command runs `bento run --watch <script>`, as the guest book's `dev` task does.

### Install dependencies

`bento install` runs `pnpm install` in the project root, including `devDependencies`. `bento install --frozen-lockfile` reproduces the locked graph exactly and fails, without touching `pnpm-lock.yaml`, when it disagrees with `package.json`. [Installing Dependencies](installing-dependencies.md) covers the details.

### Stop a run

When you press Ctrl-C or send `bento` a termination signal while a task runs, Bento forwards the signal to the task command's whole process group, including servers started by nested `bento` commands. Processes still running after a 10-second grace period are killed. `bento` exits once nothing from the task is left running, so its port is free again. It prints that the task was cancelled and exits with the status the command ended with, as a shell reports it: `0` when the command handles the signal and exits cleanly, as a nested `bento` does when its script's listener closes the script, and `130` or `143` when the signal ends it. When Bento had to kill anything in the group, it exits with `130` or `143`. The same holds while Bento is stopping processes a finished command left running in the background: the signal reaches them too, and `bento` exits with `130` or `143` rather than the command's own status.

When `bento` runs a script, the first Ctrl-C or termination signal goes to the script, which then has five seconds to end:

- If the script listens for that signal with `process.on("SIGINT")` or `process.on("SIGTERM")`, its listeners run and decide what happens, as under Node. When the script then ends, by closing its servers and calling `process.exit(code)` or by running out of work, `bento` exits with the script's code.
- Without a listener, Bento shuts the script down: it closes the script's servers and their connections and cancels its pending work. A `process.on("SIGTERM")` listener is told first, so a script that listens only for termination still cleans up after Ctrl-C. When Bento ends the script, `bento` exits with `130` for an interrupt or `143` for a termination signal. When the script ends itself first, for example by calling `process.exit(code)` from its termination listener, `bento` exits with the script's code, `0` included.

A script still running five seconds after the signal is stopped then, whether its listeners have not finished or its JavaScript never yields, as in an endless loop. `bento` exits with `130` or `143`, and the script's port is freed with it. A listener that throws is reported like any uncaught exception, and `bento` exits with `1`.

A second Ctrl-C or termination signal does not wait: `bento` exits immediately with `130` or `143`, and its port is freed with it.

### Exit codes

| Code | When |
| --- | --- |
| `0` | The script or task finished successfully. |
| The script's code | The script called `process.exit(code)` before the deadline, including from a signal listener or while Bento shut it down. |
| The command's code | A task command failed, or ended after Bento forwarded a signal to it; a command killed by signal `N` gives `128 + N`, as shells report it. |
| pnpm's code | `pnpm install` failed. |
| `130` | An interrupt (SIGINT, Ctrl-C) stopped the run: a task command it ended or that Bento had to kill, processes a finished task command left running, a script without a listener for it, or a script still running at the deadline or interrupted a second time. |
| `143` | A termination signal (SIGTERM) stopped the run, in the same cases. |
| `1` | Any other failure, reported with a message: an unknown target, a usage error, a malformed or too-wide grant flag, a malformed environment file, an invalid task graph, misplaced arguments, or a script that failed to start, could not load a module, was denied a permission, or threw an uncaught exception. |

## Worked example

The guest book needs to read files, write its database directory, listen on the network, see three variables, and load the native compilers installed in `node_modules`:

```sh
cd fixtures/remix-guest-book
bento install --frozen-lockfile
bento run --allow-read --allow-write=./db --allow-net --allow-env=PORT,DATABASE_URL,NODE_ENV --allow-addons=./node_modules dev
```

The read grant is not limited to the project because `remix/assets` looks for optional packages in the `node_modules` directory of every ancestor of the project, up to `/`, as Node's package lookup does. With only `--allow-read=.`, that lookup is a permission denial and the server does not start.

`bento run ... dev` runs the `db:migrate` task once, then the `dev` command `bento run --watch server.ts`. That nested launch names no grant flags, so it runs with exactly the five grants above. Editing a file under `app/`, whether a module the server imports or a browser module under a `public/` directory, restarts the server without rerunning the migrations; saving entries, which writes the database, does not. Ctrl-C reaches the guest book's own `SIGINT` listener through the nested `bento`, which closes its server and database and exits with `0`; both `bento` processes then exit with `0`. Running `bento run ... server.ts` with the same grants instead, Ctrl-C reaches the same listener, and `bento` exits with `0` too.

`dev` itself selects no mode, so it and its `db:migrate` task read only `.env`; only the nested watch launch also reads `.env.development`. When `.env.development` alone sets `DATABASE_URL`, migrate that database first with `NODE_ENV=development bento run db:migrate`, as the guest book's README shows.

If a task in the same project ran `bento run --allow-write=. server.ts` under those grants, the nested launch would stop before running anything with an error like:

```text
error: --allow-write=/Users/you/guest-book asks for more than the inherited write grant: /Users/you/guest-book/db
```

## Limits and edge cases

- Bento options after the target are not Bento options. They belong to the script, or make a task invocation fail because they come without `--`.
- A cancelled task stops at the command the signal reached, even when that command handles the signal and exits with `0`; its later commands and the tasks that depend on it do not run.
- Bento needs every environment variable it starts with to be valid UTF-8, and reports the first one that is not.
- Grants restrict guest code, not task commands. A task is a trusted operation on your behalf.
- A `BENTO_GRANT_CEILING` that Bento cannot read stops the launch instead of granting anything. Bento sets it for task commands; you do not set it yourself.
