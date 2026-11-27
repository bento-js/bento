---
id: guide.running-with-bento
title: Running with Bento
describes: [proposal.0001, proposal.0002, proposal.0004]
---

# Running with Bento

## What it does

The `bento` command runs your project. `bento run` starts a JavaScript, TypeScript, JSX, or TSX source file with exactly the capabilities you grant it. `bento install` installs the project's dependencies through its declared pnpm toolchain.

```text
bento run [GRANTS] [--watch] <script-path> [args...]
bento install [--frozen-lockfile]
```

Bento options go before the script path. Everything after the script path belongs to the script.

## How to use it

### Get Bento

Bento is built from this repository for macOS on Apple silicon. The default CLI build includes the local Prompt API backend and requires macOS 27.0 or newer and Xcode with SDK 27 or newer. Building also needs the Rust toolchain pinned in `rust-toolchain.toml`, which `rustup` selects automatically. From the repository root:

```sh
cargo install --locked --path crates/bento_cli
```

This installs the `bento` executable into Cargo's bin directory (`~/.cargo/bin` by default). Running an installed project needs nothing else. Installing dependencies needs pnpm and Node, because `bento install` delegates to the project's declared pnpm. A project's package tools, such as `oxfmt`, `tsc`, or `remix db migrate`, start through their own executables, which may be Node scripts.

Use `cargo install --locked --path crates/bento_cli --no-default-features` for a backend-free build without the new Foundation Models SDK requirement. The JavaScript `LanguageModel` API remains present but unavailable. See [Using the Prompt API](prompt-api.md) for model readiness, grants, and Rust reuse.

### Choose the script

`bento run` takes the path of a source file, relative to the directory you run `bento` in: `bento run server.ts`, `bento run ./server.ts`, `bento run scripts/seed.ts`. The path always names a file. Bento never looks it up in `package.json`, never runs a `scripts` entry, and never runs it as a shell command. When nothing is at the path, or the path names a directory, Bento reports `error: no script file at <path>` and runs nothing.

The script's project is the nearest directory, from the script's own directory upward, that holds a `package.json`. Bento reads environment files and packages from there.

Bento does not run project tasks. Coordinate work such as migrating a database before starting the server, or running `tsc` and a linter, with your package manager or whichever task runner you prefer, and have it call `bento run` for each source file. For example, `"start": "bento run server.ts"` in the `scripts` of `package.json` runs with `pnpm start`, and `make`, `just`, Mise, or a shell script can run the same command.

### Pass arguments

Words after the script path are the script's own arguments, passed through unchanged: `bento run server.ts --port 3000`. That includes a `--`: `bento run server.ts -- --port 3000` hands the script `--`, `--port`, and `3000`.

### Grant capabilities

Guest code can use only what the launch grants it. Without any grant flag, a script may not read or write files, use the network, see environment variables, load native addons, or access the local language model. Loading a module reads it, so even running a script needs `--allow-read` covering the script, the modules it imports, and the packages they come from; `--allow-read=.` in the project directory covers them all. Without it, Bento reports a permission denial for the script itself.

| Flag | Grants | Items |
| --- | --- | --- |
| `--allow-read[=PATH,...]` | Reading files and directories | Paths; each covers everything beneath it |
| `--allow-write[=PATH,...]` | Creating, changing, and removing files | Paths; each covers everything beneath it |
| `--allow-net[=HOST[:PORT],...]` | Connecting and listening | A hostname, an IPv4 address, or an IPv6 address in brackets, as in `[::1]:8080`; a host without a port covers all its ports |
| `--allow-env[=NAME,...]` | Seeing and setting environment variables | Exact variable names |
| `--allow-addons[=PATH,...]` | Loading native Node-API addons | Paths; each covers everything beneath it |
| `--allow-language-model[=true\|false]` | Accessing the local language model | Boolean capability; no model identifiers |

For the scoped flags above, a flag without `=` grants the whole kind: `--allow-net` allows every host. With `=`, it grants only the comma-separated items. Give each flag at most once; list several items in one flag instead. An empty item, such as `--allow-read=` or `--allow-env=PORT,,HOST`, is an error. So is a network target that is not a hostname, an IPv4 address, or a bracketed IPv6 address with an optional port: `--allow-net=localhost/path`, `--allow-net=[bogus]:8080`, and `--allow-net='white space'` stop the launch before any script runs. Write IPv4 addresses as four decimal numbers; shorthand such as `127.1` is rejected rather than read as a hostname.

`--allow-language-model` requests the boolean capability. `--allow-language-model=false` explicitly denies it, including when the launch inherits access from a ceiling. Without the grant, model availability reports `unavailable` and creation fails with `NotAllowedError` before inspecting native readiness.

A network target names an endpoint, not a spelling. Hostnames match regardless of case, and IP addresses match by value: `[0:0:0:0:0:0:0:1]:8080` is the same endpoint as `[::1]:8080`, and the IPv4-mapped `[::ffff:127.0.0.1]` is the same as `127.0.0.1`. Bento never looks hostnames up, so `localhost` and `127.0.0.1` stay different targets.

Relative paths resolve from the directory you run `bento` in, not from the project root. Bento then resolves each path as the operating system would, so a grant names the real location: granting `./data` when `data` is a link to `/srv/data` grants `/srv/data`, and because `..` after a link leaves the link's target, `./data/../shared` grants `/srv/shared`. A link to something that does not exist yet grants the location it points to. A path that does not exist yet, such as a database file the application will create, keeps its missing part as written beneath its nearest existing directory.

No flag can let guest code start subprocesses, import remote modules, or read system information. Every script may read the process's user and group IDs, which Node's `fs.access` needs to check a file's permission bits; apart from the platform and architecture in `process.platform` and `process.arch`, nothing else about the system is available, so calls such as `os.hostname()` and `os.userInfo()` fail with `NotCapable`. An addon grant is different from the others: an addon runs native code inside the Bento process, so once loaded it is not contained by the file, network, or environment grants. Grant addons only from packages you trust.

Grants govern guest code. They do not restrict the program that starts `bento`, such as a task runner, or anything else that program runs.

### An inherited ceiling limits the grants

When `bento` starts with `BENTO_GRANT_CEILING` in its environment, it runs under the grants that value carries, as `Grants::to_ceiling_value` in the `bento_host` crate writes them. Under a ceiling:

- A kind the command does not mention is inherited unchanged.
- A flag may narrow its kind: `--allow-read=app` under an inherited `--allow-read=.` is fine.
- A flag that asks for more than the inherited grant fails before anything runs. The error names the flag and the inherited limit.

A launch started directly from your shell has no ceiling, so any kind its flags leave out is denied.

Bento checks the ceiling and the grant flags before it looks for the script or reads `package.json`. A malformed or too-wide flag, or a ceiling Bento cannot read, stops the launch with status 1 even when the script is missing or `package.json` is broken.

### Load the environment

Before running a script, Bento loads the project's `.env`, `.env.<mode>`, and `.env.local` files beneath the variables `bento` was started with. The mode comes from the inherited `NODE_ENV`, else the `NODE_ENV` in `.env`; `bento run --watch` defaults it to `development` when neither sets it. [Environment Files](environment-files.md) describes the file syntax, the precedence, and how the mode is chosen.

Loading a variable does not grant guest code access to it; `--allow-env` does.

### Run a script

`bento run --allow-read=. server.mjs` executes the file in Bento's own runtime, inside the `bento` process. Nothing else needs to be installed to run it: Bento never starts Node, Deno, Bun, or pnpm, and runs the same with none of them on `PATH`.

`bento run server.ts` executes TypeScript, JSX, and TSX directly. They need no build step, no prebuilt JavaScript entrypoint, and no `--import remix/node-tsx`, and Bento writes no JavaScript next to your sources: it transforms each module in memory as it loads. Bento follows the execution settings in `tsconfig.json` and the files it extends, such as `jsx` and `jsxImportSource`, and reports an execution-affecting setting it cannot honor rather than choosing another JSX runtime. It does not type-check; run your type checker, such as `tsc`, separately.

Modules resolve the way Node resolves them for the project: `package.json` `imports` and `exports`, ESM and CommonJS packages, and pnpm's `node_modules` layout. Packages resolve from the script's project even when you run `bento` from another directory. `tsconfig.json` `paths` never change resolution. Execution never installs or downloads anything; a missing package names the import that failed and points you to `bento install`.

An ESM import of a CommonJS source receives its `module.exports` value as the default export, including when that source re-exports JSON or a native addon through `require`. JSON fields and addon properties do not become inferred named exports.

The script sees what a Node script would see of its launch, limited by its grants:

- `process.argv.slice(2)` holds the words after the script path, unchanged, as under Node.
- `process.cwd()` is the directory you ran `bento` in, not the project root.
- `process.env` holds the variables `--allow-env` grants, from your environment or the project's environment files. Other variables are invisible, not just unreadable.

Packages with Node-API native addons, such as the compilers `remix/assets` uses, load into the `bento` process when `--allow-addons` covers their installed location. Without that grant, loading the addon is a permission denial and its native code never runs.

An addon runs as native code in the `bento` process, so it sees the environment `bento` itself was started with, not the variables granted to the script. The Oxc resolver that `remix/assets` loads honors `NODE_PATH` there: launched with `NODE_PATH` pointing at another project's packages, it can resolve packages outside your project, which a scoped read grant then denies. Start `bento` without `NODE_PATH` set.

Top-level `await` works, and the script runs until it has no work left or calls `process.exit`. `process.exit(code)` ends the script with that exit code. When the script fails instead, Bento names the kind of failure: the script could not start, a module could not be resolved or transformed, permission was denied, or an exception went uncaught. It shows the error's message and where it happened, as [Read error reports](#read-error-reports) describes, and `bento` exits with status 1.

A script that ends while work it started is still running does not finish the run yet. Bento cancels that work and waits five seconds for it to settle. When it has not settled by then, `bento` reports that the work the script started did not finish within the shutdown deadline and that disposal is incomplete, then exits with status 1, whatever the script's own exit code was.

### Read error reports

When an error points at a place in a file Bento can read, Bento shows that place: the file with its line and column, the source lines around it, and a marker under the position saying what is wrong there. A `package.json` that is not valid JSON, for example, is reported like this:

```text
Error: /Users/you/guest-book/package.json is not valid JSON
   ╭─[ /Users/you/guest-book/package.json:4:3 ]
   │
 4 │   "private": true
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

`bento run --watch server.ts` runs the script and restarts it completely when its source changes; it does not replace modules in place. Bento watches the project directory, so browser modules compiled by `remix/assets` count even though the server never imports them. It also watches non-dependency source files loaded outside the project through ESM imports, dynamic `import()`, CommonJS `require`, and `createRequire`, including calls made after startup. Bento adds those inputs within a fraction of a second. When it cannot watch one yet, it prints a warning once and keeps trying while the script runs.

Bento watches configuration that changes resolution and transformation: the project's `tsconfig.json` and its full `extends` chain, and the package scopes of loaded non-dependency sources. A workspace source's nearest existing `package.json` is watched even outside the project, so changing its `type`, `imports`, or self-reference `exports` restarts the run. Creating a nearer package scope inside the project or in an external source's own directory also restarts it. Creating `package.json` above an external source's own directory needs a manual restart. Watching an external package scope alone does not subscribe to its dependency or build-output subdirectories. Package-scope searches do not add watches above the project root. Bento reads `tsconfig.json` again on each change, so a file that an edit makes it extend is watched from that restart.

Edits, replacements, deletions, and creations that repair a missing file import trigger a restart. Failed resolution from a non-dependency source retains the missing file targets for ESM, CommonJS, dynamic `import()`, `require.resolve`, and `import.meta.resolve`, including targets named by non-dependency package `imports` and self-reference `exports`. A dependency's dynamic import adds no failed target merely because the application supplied the URL. If the application itself previously resolved that URL, including with `import.meta.resolve`, a later failed load retains the target. CommonJS missing-file candidates include extensions registered in `require.extensions`. Creating the missing parent directories and file recovers the run, including for workspace sources outside the project. An explicitly named failed path may require watching its nearest existing ancestor above the project root. Successful resolution discards its unused extension and index probes, even when a custom loader later throws a not-found error. CommonJS attribution uses the cached parent module object: requests with dependency or synthetic parents are excluded. Failed `createRequire` calls add no missing-file watches, even when anchored to a loaded application file, so repairs outside the watched project need a manual restart.

Creating a missing local script started through a Web Worker or `node:worker_threads` also restarts the run, including scripts outside the project and in excluded directories such as `dist`. This holds whoever starts the worker, so a dependency that starts one on a missing path outside `node_modules` is watched for that path too. Scripts under `node_modules` remain excluded.

A dependency borrowing an application's cached module object through `require.main.require`, `module.parent.require`, an application-supplied `require`, or `Module._load(request, appModule)` has application attribution. Its failed targets are watched, including targets in ancestor directories. A dependency walking up to the filesystem root through such a borrowed module can therefore cause a recursive watch on `/` for a missing file there. Use the dependency's own `require` for dependency-owned configuration searches.

Failed `require.resolve` calls with `options.paths` record no watch inputs. Repairing a target outside the watched project through those search paths needs a manual restart.

Dependencies reached through `node_modules` are excluded. A missing target in a workspace package reached through a `node_modules` link needs a manual restart after repair. Once a workspace source loads through that link, Bento watches its canonical path outside `node_modules`.

Sources outside the project are watched by their resolved file paths. Retargeting a symlink outside the project so that it points at a different file, or renaming away the directory that contains a loaded external source and putting another in its place, needs a manual restart. Editing, replacing, or deleting the external file itself restarts the run. Inside the project, the whole directory is watched, so these changes restart there too. An external source first loaded after startup, such as by a dynamic import inside a request handler, is watched within about 200 ms of loading; an edit made in that window is picked up only with the next change.

A missing file that `tsconfig.json` extends can be repaired the same way: creating it restarts the run, even when its parent directories must also be created. This also works through directory symlinks and for a missing configuration package installed into the project's own `node_modules`. If Bento cannot watch an extended configuration, it warns once for that file and retries on later source changes.

Inside the project, Bento ignores what running or building the application writes, so requests that write to them do not cause restarts:

- dependencies and version control: `node_modules` and `.git`;
- build output and caches: `dist`, `build`, `target`, `.cache`, and `coverage` directories, and `.DS_Store` files;
- databases and SQLite side files: `*.db`, `*.sqlite`, `*.sqlite3`, `*-wal`, `*-shm`, and `*-journal`;
- environment files: `.env` and `.env.*`, because a restart would not reload them.

A module the script actually loads from one of those places, such as an entrypoint in `dist`, is still watched. Modules loaded from `node_modules` directories inside the project are excluded, as are modules in `node_modules` directories outside it. A `node_modules` ancestor above the project root does not exclude the project's own loaded files.

Edits within about a tenth of a second of each other cause one restart. Bento says on standard error which file changed each time it restarts. The new instance starts only after the old one has shut down, as it would for a termination signal, and released its port. When loading fails, Bento reports the error, says it is waiting for changes, and keeps watching, so fixing the file or creating the missing one recovers; it never keeps serving the old code as though the reload had worked. A script that ends by itself, by finishing its work or calling `process.exit`, does not end the watch either: Bento reports how it ended and runs it again after the next change.

When a run ends with work it started still unsettled at the shutdown deadline, Bento says that disposal is incomplete and keeps watching, instead of ending with status 1 as a plain `bento run` does. A restart whose shutdown leaves work unfinished is reported the same way.

Watching does not reload `.env` files, and it restarts only the script: a command that ran before `bento` started, such as a database migration, does not run again. `--watch` selects the `development` mode for the environment files and `NODE_ENV` unless your environment or `.env` sets `NODE_ENV`.

Stopping the command stops both the application and the watching. Ctrl-C or a termination signal stops the running script as [Stop a run](#stop-a-run) describes, and then `bento` exits instead of waiting for changes. When no script is running, because loading failed or the script ended by itself, `bento` exits at once with `130` or `143`.

### Install dependencies

`bento install` runs `pnpm install` in the project root, including `devDependencies`. `bento install --frozen-lockfile` reproduces the locked graph exactly and fails, without touching `pnpm-lock.yaml`, when it disagrees with `package.json`. [Installing Dependencies](installing-dependencies.md) covers the details.

### Stop a run

The first Ctrl-C or termination signal goes to the script, which then has five seconds to end:

- If the script listens for that signal with `process.on("SIGINT")` or `process.on("SIGTERM")`, its listeners run and decide what happens, as under Node. When the script then ends, by closing its servers and calling `process.exit(code)` or by running out of work, `bento` exits with the script's code.
- Without a listener, Bento shuts the script down: it closes the script's servers and their connections and cancels its pending work. A `process.on("SIGTERM")` listener is told first, so a script that listens only for termination still cleans up after Ctrl-C. When Bento ends the script, `bento` exits with `130` for an interrupt or `143` for a termination signal. When the script ends itself first, for example by calling `process.exit(code)` from its termination listener, `bento` exits with the script's code, `0` included.

A script still running five seconds after the signal is stopped then, whether its listeners have not finished or its JavaScript never yields, as in an endless loop. `bento` exits with `130` or `143`, and the script's port is freed with it. A listener that throws is reported like any uncaught exception, and `bento` exits with `1`.

A second Ctrl-C or termination signal does not wait: `bento` exits immediately with `130` or `143`, and its port is freed with it.

### Exit codes

| Code | When |
| --- | --- |
| `0` | The script finished successfully. |
| The script's code | The script called `process.exit(code)` before the deadline, including from a signal listener or while Bento shut it down. |
| pnpm's code | `pnpm install` failed. |
| `130` | An interrupt (SIGINT, Ctrl-C) stopped the run: a script without a listener for it, or a script still running at the deadline or interrupted a second time. |
| `143` | A termination signal (SIGTERM) stopped the run, in the same cases. |
| `1` | Any other failure, reported with a message: no script file at the path, a usage error, a malformed or too-wide grant flag, a malformed environment file, or a script that failed to start, could not load a module, was denied a permission, threw an uncaught exception, or ended while work it started had not settled. |

## Worked example

The guest book needs to read files, write its database directory, listen on the network, see three variables, and load the native compilers installed in `node_modules`. Install it, create its database with its own migration tool, and serve it in watch mode:

```sh
cd fixtures/remix-guest-book
bento install --frozen-lockfile
pnpm exec remix db migrate
bento run --allow-read --allow-write=./db --allow-net --allow-env=PORT,DATABASE_URL,NODE_ENV --allow-addons=./node_modules --watch server.ts
```

The read grant is not limited to the project because `remix/assets` looks for optional packages in the `node_modules` directory of every ancestor of the project, up to `/`, as Node's package lookup does. With only `--allow-read=.`, that lookup is a permission denial and the server does not start.

`remix db migrate` is a package tool that runs under Node, outside Bento, so it needs no grants; it creates the database `db/data.db` that the app uses by default. `bento run --watch server.ts` then serves the app with exactly the five grants above. Editing a file under `app/`, whether a module the server imports or a browser module under a `public/` directory, restarts the server without running the migration again; saving entries, which writes the database, does not. Ctrl-C reaches the guest book's own `SIGINT` listener, which closes its server and database and exits with `0`, and `bento` then exits with `0` too. Without `--watch`, `bento run ... server.ts` serves the same app, and Ctrl-C ends it the same way.

The guest book's own `mise.toml` runs these steps as tasks, so `mise run dev` in its directory migrates and then serves it; any task runner can run the same commands.

Started under a ceiling that carries those five grants, `bento run --allow-write=. server.ts` would stop before running anything with an error like:

```text
error: --allow-write=/Users/you/guest-book asks for more than the inherited write grant: /Users/you/guest-book/db
```

## Limits and edge cases

- Bento options after the script path are not Bento options. They belong to the script: `bento run server.ts --watch` hands `--watch` to the script rather than watching it.
- A path without an extension still names a file: `bento run build` runs the file `build` if there is one, and otherwise reports that no script file is there, whatever `package.json` declares.
- Bento needs every environment variable it starts with to be valid UTF-8, and reports the first one that is not.
- Grants restrict guest code, not the program that starts `bento`.
- A `BENTO_GRANT_CEILING` that Bento cannot read stops the launch instead of granting anything.
