---
id: guide.running-with-bento
title: Running with Bento
describes: [proposal.0001, proposal.0002, proposal.0003]
---

# Running with Bento

## What it does

The `bento` command runs your project. `bento run` starts a JavaScript, TypeScript, JSX, or TSX source file, or a task declared in the top-level `tasks` field of `package.json`. `bento install` installs the project's dependencies through its declared pnpm toolchain.

A `bento run` command without permission flags is unrestricted, as `node` is: the code it runs can do anything your user can. Running code you do not trust requires `--permission`, which restricts the launch to what its `--allow-*` flags allow. [Control what code may do](#control-what-code-may-do) lists the flags.

```text
bento run [--permission] [--allow-*]... [--watch] <script-path | task-name> [args...]
bento install [--frozen-lockfile]
```

Bento options go before the target. Everything after the target belongs to the script or task.

## How to use it

### Get Bento

Bento is built from this repository for macOS on Apple silicon. The default CLI build includes the local Prompt API backend and requires macOS 27.0 or newer and Xcode with SDK 27 or newer. Building also needs the Rust toolchain pinned in `rust-toolchain.toml`, which `rustup` selects automatically. From the repository root:

```sh
cargo install --locked --path crates/bento_cli
```

This installs the `bento` executable into Cargo's bin directory (`~/.cargo/bin` by default). Running an installed project needs nothing else. Installing dependencies needs pnpm and Node, because `bento install` delegates to the project's declared pnpm, and the package tools a task runs, such as `oxfmt`, `tsc`, or `remix db migrate`, start through their own executables, which may be Node scripts.

Use `cargo install --locked --path crates/bento_cli --no-default-features` for a backend-free build without the new Foundation Models SDK requirement. The JavaScript `LanguageModel` API remains present but unavailable. See [Using the Prompt API](prompt-api.md) for model readiness, permissions, and Rust reuse.

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

### Control what code may do

Without `--permission`, a launch is unrestricted. The code can read and write any file your user can, connect and listen anywhere, see the whole prepared environment, start subprocesses and workers, load native addons, and use the local language model when the build includes it. That is Node's default, and it is what your own project's tools expect: `bento run node_modules/vite/bin/vite.js build` needs no flags. Never run code you do not trust this way.

`--permission`, given before the target, makes the launch restricted. The code then holds only the permissions its `--allow-*` flags give it, and everything else is denied:

```text
bento run [--permission] [--allow-fs-read=PATH]... [--allow-fs-write=PATH]... [--allow-env=NAME]...
          [--allow-net] [--allow-child-process] [--allow-worker] [--allow-addons] [--allow-wasi]
          [--allow-ffi] [--allow-inspector] [--allow-openssl-store] [--allow-fs-vfs]
          [--allow-language-model] [--watch] <script-path | task-name> [args...]
```

| Flag | Allows | `process.permission.has()` scope |
| --- | --- | --- |
| `--allow-fs-read=PATH` | Reading files and directories | `fs.read` |
| `--allow-fs-write=PATH` | Creating, changing, and removing files | `fs.write` |
| `--allow-env=NAME` | Keeping environment variables | `env` |
| `--allow-net` | Connecting to and listening on any host and port | `net` |
| `--allow-child-process` | Starting subprocesses | `child` |
| `--allow-worker` | Starting worker threads | `worker` |
| `--allow-addons` | Loading native Node-API addons from any path | `addon` |
| `--allow-wasi` | WASI | `wasi` |
| `--allow-ffi` | FFI | `ffi` |
| `--allow-inspector` | The inspector | `inspector` |
| `--allow-openssl-store` | OpenSSL STORE loaders | `openssl.store` |
| `--allow-fs-vfs` | Mounting a virtual file system | none, as in Node |
| `--allow-language-model` | The local language model | `language-model` |

These are Node 26's flags, with Node's meanings. `--allow-language-model` is Bento's own, because Node has no language model.

The file `bento run` executes is readable without `--allow-fs-read`, and only that file: the modules it imports, including installed packages, need read permission. `bento run --permission server.ts` therefore loads `server.ts` and is denied its first import. Run from the project directory, `--allow-fs-read=.` covers the project's modules and its own `node_modules`.

`--allow-wasi`, `--allow-ffi`, `--allow-inspector`, `--allow-openssl-store`, and `--allow-fs-vfs` are accepted, recorded, and reported by `process.permission.has()`. Where Bento provides the API one of them governs, the flag gates it with Node's error. Where Bento does not provide that API yet, the API fails with or without the flag.

#### Flag syntax and errors

- `--allow-fs-read` and `--allow-fs-write` require a value, given as `--allow-fs-read=VALUE` or `--allow-fs-read VALUE`, and may be repeated. Without one, the launch fails with `error: --allow-fs-read requires an argument`.
- `--allow-env` takes `*`, a variable name, or a name prefix followed by `*`, such as `APP_*`. Values may be repeated or separated by commas, and an empty item between commas, as in `--allow-env=PORT,,HOST`, is skipped. Without a value, the launch fails with `error: --allow-env requires an argument`. An item of any other form, such as `A*B` or `A=B`, fails with `error: --allow-env must be '*', a variable name, or a variable name prefix followed by '*'`.
- Every other flag takes no value. Giving one a value, such as `127.0.0.1:3000` for `--allow-net`, is a usage error that names the flag. Node accepts such a value and ignores it, allowing every host; Bento refuses it, so a command written for an earlier Bento's host-scoped network flag cannot silently allow every host.
- Any `--allow-*` flag without `--permission` fails before anything runs, with `error: --permission is required`.
- These errors exit with status 1, like Bento's other usage errors. Node exits with status 9 when a flag is missing its value.
- `bento run --help` lists the flags under a "Permissions" heading.

Quote a value containing `*` in your shell, as in `--allow-fs-read='*'`, so that the shell hands it to Bento unchanged. Unquoted, zsh refuses the command because no file matches the pattern.

#### Which paths a value covers

- `*` alone covers every path.
- Any other value is a path, absolute or relative to the directory you run `bento` in, not to the project root.
- A value containing `*` covers every path whose text begins with the text before the first `*`. Anything after that `*` is ignored. `--allow-fs-read='./test*'` covers `test`, `test2`, and `testing/notes.txt`, but not `tes`.
- Without `*`, an existing directory covers itself and everything beneath it. Anything else, a file or a path that does not exist yet, covers exactly that path. A directory the code will create needs a trailing `*`: `--allow-fs-write='./build/*'` covers `build` itself and everything the code creates beneath it.
- A comma is part of the path. To allow several paths, repeat the flag. When a value contains a comma, Bento prints Node's warning to standard error, naming the flag, and continues:

```text
warning: The --allow-fs-read CLI flag has changed. Passing a comma-separated list of paths is no longer valid. Documentation can be found at https://nodejs.org/api/permissions.html#file-system-permissions
```

Bento compares paths at their real locations. It resolves each value as the operating system would before comparing it: when `data` is a link to `/srv/data`, `--allow-fs-read=./data` covers `/srv/data`, and because `..` after a link leaves the link's target, `--allow-fs-read=./data/../shared` covers `/srv/shared`. A link to something that does not exist yet names the location it points to, and a path that does not exist yet keeps its missing part as written beneath its nearest existing directory. A value with `*` has its existing directory part resolved and the rest kept as written. On macOS, where `/tmp` is a link to `/private/tmp`, `--allow-fs-read=/tmp/app` covers `/private/tmp/app`.

Operations that follow links are checked against the fully resolved target, so a link inside an allowed directory that points outside it is denied. Node compares paths as written and follows such a link. Operations on a link itself, such as `lstat`, `readlink`, and `unlink`, are checked at the link's own location, as [Node Compatibility](node-compatibility.md#permissions-and-native-code) describes. Creating a symbolic link requires both `--allow-fs-read='*'` and `--allow-fs-write='*'`, because a permission for particular paths cannot vouch for a target that is resolved later.

#### Environment variables

An unrestricted launch sees the whole prepared environment. A restricted launch starts with every variable that `--allow-env` does not match removed, except the variables Node always keeps, such as `PATH` and `HOME`, and the values Bento prepared for the launch: those from the project's environment files and the watch-mode `NODE_ENV` default. Reading a removed variable returns `undefined` and warns the first time. The code may still set, change, and delete any variable while it runs. [Tasks and Environment](tasks-and-environment.md#environment-in-a-restricted-launch) gives the full rules.

#### Subprocesses, workers, native addons, and the network

`node:child_process` (`spawn`, `exec`, `execFile`, `fork`, and their synchronous forms) and `Deno.Command` work in an unrestricted launch, and in a restricted one with `--allow-child-process`. A child process runs with your user's full authority, and no permission restricts it, so allowing child processes in a restricted launch lets the code escape every other restriction. Pass `--allow-child-process` only for code you would also run unrestricted. A subprocess receives the launch's environment after the startup removal. When a restricted launch starts a process, Bento also adds `BENTO_PERMISSION_CEILING` to that environment, describing the launch's permissions at that moment, so a `bento` the process starts stays within them, as [nested launches](#nested-bento-commands-stay-within-a-ceiling) do. Bento does not change `NODE_OPTIONS`, and programs other than `bento` ignore the ceiling.

`process.execPath` is the `bento` executable, and Bento does not accept Node's command-line options yet ([#260](https://github.com/bento-js/bento/issues/260)). `fork()`, and any code that starts `process.execPath` with Node's options, therefore fails with Bento's usage error from the child, even in an unrestricted launch.

`worker_threads.Worker` and the Web `Worker` work in an unrestricted launch, and in a restricted one with `--allow-worker`. A worker always has its creator's current permissions; passing `execArgv` cannot change them.

Native addons load in an unrestricted launch, and in a restricted one with `--allow-addons`, from any path. An addon runs native code inside the `bento` process, outside every permission: once loaded, it is not contained by the file, network, or environment restrictions. Pass `--allow-addons` only when you trust every package that ships an addon.

`--allow-net` allows every host and port. Neither Bento nor Node can limit a launch to particular hosts.

#### Available in every launch, and in none

System information is available in every launch, restricted or not, because Node restricts none of it: `os.homedir()`, `os.hostname()`, `os.userInfo()`, `os.networkInterfaces()`, `os.cpus()`, the memory, load, uptime, and release queries, and their `Deno` counterparts.

Some operations are unavailable in every launch, because they are not permissions: importing remote modules, changing the `bento` process's working directory or loading an environment file into it (`process.chdir`, `process.loadEnvFile`), and terminating or signalling the `bento` process (`process.abort`, `process.kill`, `Deno.kill`). Code that starts a subprocess can still signal it with `subprocess.kill()`.

#### Denied operations

In a restricted launch, a denied operation throws what Node 26 throws for the same operation:

| Denied operation | What the code receives |
| --- | --- |
| Reading a file | `ERR_ACCESS_DENIED` with `permission: 'FileSystemRead'` and `resource` set to the path as the call named it |
| Writing a file | `ERR_ACCESS_DENIED` with `permission: 'FileSystemWrite'` and the path as `resource` |
| Connecting or listening | `ERR_ACCESS_DENIED` with `permission: 'Net'`; a denied `fetch` rejects with an error whose `cause` is that error |
| Starting a subprocess | `ERR_ACCESS_DENIED` with `permission: 'ChildProcess'` and the command as `resource`; no process starts |
| Starting a worker | `ERR_ACCESS_DENIED` with `permission: 'WorkerThreads'` |
| Loading a native addon | `ERR_DLOPEN_DISABLED`: `Cannot load native addon because loading addons is disabled.` The addon's native code never runs. |
| Using the language model | `LanguageModel.availability()` reports `unavailable`, and `LanguageModel.create()` rejects with `NotAllowedError` |
| Reading a removed environment variable | No error: the value is `undefined` |

An `ERR_ACCESS_DENIED` message names the flag that would allow the operation, as in `Access to this API has been restricted. Use --allow-fs-read to manage permissions.` `Deno` namespace APIs throw the same errors as their Node counterparts.

An uncaught denial ends the run as a permission failure: Bento shows where the operation was denied, as [Read error reports](#read-error-reports) describes, and exits with status 1.

Bento prints none of Node's `ExperimentalWarning` or `SecurityWarning` messages about permission flags; those describe Node's own stability levels. It keeps the comma warning above and the removed-variable warning, because each explains an outcome.

#### Inspect and drop permissions

In a restricted launch, `process.permission` reports what the code holds and lets it give permissions up. In an unrestricted launch it is `undefined`, as in Node. `process.permission.has("fs.read", "./data")` answers for one path, and `process.permission.has("fs.read")` is `true` only after `--allow-fs-read='*'`. `process.permission.drop("fs.write")` removes a permission for the rest of the run, and nothing gives it back. [Node Compatibility](node-compatibility.md#inspect-and-drop-permissions) describes both methods.

#### Move from earlier flags

Earlier Bento releases denied everything by default and used Deno's flag names. Commands that use them fail until you rewrite them:

| Before | After |
| --- | --- |
| `--allow-read` / `--allow-read=a,b` | `--permission --allow-fs-read=*` / `--permission --allow-fs-read=a --allow-fs-read=b` |
| `--allow-write=data` | `--permission --allow-fs-write=data` (add `/*` when `data` does not exist yet) |
| `--allow-net` / `--allow-net=host:port` | `--permission --allow-net` (every host) |
| `--allow-env=A,B` | `--permission --allow-env=A,B` |
| `--allow-addons` / `--allow-addons=path` | `--permission --allow-addons` (every path) |
| `--allow-language-model` / `--allow-language-model=false` | `--permission --allow-language-model` / `--permission` without it |

The removed flags are reported as unknown or malformed options. A command that passed no flags used to deny everything and now runs unrestricted, so add `--permission` wherever you relied on it to contain code. A nested launch that passes `--permission` no longer inherits the permissions its flags leave out. In a restricted launch, variables that `--allow-env` does not match are removed rather than hidden, writes to any variable succeed, and values from the project's environment files are visible. Code that checked for `NotCapable` or `PermissionDenied` errors must check for `ERR_ACCESS_DENIED` and `ERR_DLOPEN_DISABLED` instead.

### Nested `bento` commands stay within a ceiling

When a task command runs `bento` again, the nested launch receives the permissions of the launch that started the task through `BENTO_PERMISSION_CEILING`. A process that a restricted guest starts receives that guest's permissions the same way. The name `bento` in a task command always means that same running `bento` executable, even when the project installs its own `bento` in `node_modules/.bin`, because Bento puts a link to itself first on the task's `PATH`.

- A nested launch without `--permission` holds exactly its ceiling's permissions: it is unrestricted under an unrestricted ceiling, and restricted the same way under a restricted one.
- A nested launch with `--permission` holds exactly what its own flags and its entrypoint allow, as a launch from your shell does. Permissions its flags do not mention are denied, even when the ceiling holds them.
- Each flag of a nested launch must fit within the ceiling. A flag that asks for more fails before any guest code runs, with an error that names the flag and the inherited limit, and the task that ran the nested command fails with it.
- A nested launch's entrypoint is readable without a flag only when the ceiling's read permission covers it.

A launch started directly from your shell has no ceiling. A nested launch can never widen the permissions it inherited.

### Load the environment

Before running a script or task, Bento loads the project's `.env`, `.env.<mode>`, and `.env.local` files beneath the variables you set yourself. The mode comes from your `NODE_ENV`, else the `NODE_ENV` in `.env`; `bento run --watch` defaults it to `development` when neither sets it. [Tasks and Environment](tasks-and-environment.md) describes the file syntax, the precedence, and how nested launches choose their own mode file.

In a restricted launch, the values Bento loads from these files stay visible to the code even when `--allow-env` does not match them, as Node keeps the values from files named with `--env-file`. Task commands receive the whole prepared environment.

### Run a script

`bento run server.mjs` executes the file in Bento's own runtime, inside the `bento` process. Nothing else needs to be installed to run it: Bento never starts Node, Deno, Bun, or pnpm, and runs the same with none of them on `PATH`.

`bento run server.ts` executes TypeScript, JSX, and TSX directly. They need no build step, no prebuilt JavaScript entrypoint, and no `--import remix/node-tsx`, and Bento writes no JavaScript next to your sources: it transforms each module in memory as it loads. Bento follows the execution settings in `tsconfig.json` and the files it extends, such as `jsx` and `jsxImportSource`, and reports an execution-affecting setting it cannot honor rather than choosing another JSX runtime. It does not type-check; run your type checker as a task.

Modules resolve the way Node resolves them for the project: `package.json` `imports` and `exports`, ESM and CommonJS packages, and pnpm's `node_modules` layout. Packages resolve from the script's project even when you run `bento` from another directory. `tsconfig.json` `paths` never change resolution. Execution never installs or downloads anything; a missing package names the import that failed and points you to `bento install`.

An ESM import of a CommonJS source receives its `module.exports` value as the default export, including when that source re-exports JSON or a native addon through `require`. JSON fields and addon properties do not become inferred named exports.

The script sees what a Node script would see of its launch:

- `process.argv.slice(2)` holds the words after the script path, unchanged, as under Node.
- `process.cwd()` is the directory you ran `bento` in, not the project root.
- `process.env` holds the prepared environment: your variables and the project's environment files. In a restricted launch, the variables removed at startup are absent from it.

Packages with Node-API native addons, such as the compilers `remix/assets` uses, load into the `bento` process. A restricted launch needs `--allow-addons` for them; without it, loading an addon throws `ERR_DLOPEN_DISABLED` and its native code never runs.

An addon runs as native code in the `bento` process, so it sees the environment `bento` itself was started with, not the script's environment. The Oxc resolver that `remix/assets` loads honors `NODE_PATH` there: launched with `NODE_PATH` pointing at another project's packages, it can resolve packages outside your project, and a launch whose `--allow-fs-read` covers only the project is then denied reading them. Start `bento` without `NODE_PATH` set.

Top-level `await` works, and the script runs until it has no work left or calls `process.exit`. `process.exit(code)` ends the script with that exit code. When the script fails instead, Bento names the kind of failure: the script could not start, a module could not be resolved or transformed, permission was denied, or an exception went uncaught. It shows the error's message and where it happened, as [Read error reports](#read-error-reports) describes, and `bento` exits with status 1.

A script that ends while work it started is still running does not finish the run yet. Bento cancels that work and waits five seconds for it to settle. When it has not settled by then, `bento` reports that the work the script started did not finish within the shutdown deadline and that disposal is incomplete, then exits with status 1, whatever the script's own exit code was.

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
| `1` | Any other failure, reported with a message: an unknown target, a usage error such as a malformed permission flag, an `--allow-*` flag without `--permission`, or a nested flag beyond its ceiling, a malformed environment file, an invalid task graph, misplaced arguments, or a script that failed to start, could not load a module, was denied a permission, threw an uncaught exception, or ended while work it started had not settled. |

## Worked example

The guest book runs like any Node project, with no flags:

```sh
cd fixtures/remix-guest-book
bento install --frozen-lockfile
bento run dev
```

To contain it instead, restrict the launch to what the guest book uses: reading files, writing its database directory, the network, three variables, and the native compilers installed in `node_modules`:

```sh
bento run --permission --allow-fs-read='*' --allow-fs-write=./db --allow-net --allow-env=PORT,DATABASE_URL,NODE_ENV --allow-addons dev
```

Reading is not limited to the project because `remix/assets` looks for optional packages in the `node_modules` directory of every ancestor of the project, up to `/`, as Node's package lookup does. With only `--allow-fs-read=.`, that lookup is denied and the server does not start. The guest book's `db` directory exists, so `--allow-fs-write=./db` covers the database and its journal files beneath it.

`bento run ... dev` runs the `db:migrate` task once, then the `dev` command `bento run --watch server.ts`. That nested launch passes no `--permission`, so it holds exactly the permissions above. Editing a file under `app/`, whether a module the server imports or a browser module under a `public/` directory, restarts the server without rerunning the migrations; saving entries, which writes the database, does not. Ctrl-C reaches the guest book's own `SIGINT` listener through the nested `bento`, which closes its server and database and exits with `0`; both `bento` processes then exit with `0`. Running `bento run ... server.ts` with the same flags instead, Ctrl-C reaches the same listener, and `bento` exits with `0` too.

`dev` itself selects no mode, so it and its `db:migrate` task read only `.env`; only the nested watch launch also reads `.env.development`. When `.env.development` alone sets `DATABASE_URL`, migrate that database first with `NODE_ENV=development bento run db:migrate`, as the guest book's README shows.

If a task in the same project ran `bento run --permission --allow-fs-write=. server.ts` under those permissions, the nested launch would stop before running anything, because the project directory is more than the inherited `./db`:

```text
error: --allow-fs-write=/Users/you/guest-book/* asks for more than the inherited fs-write permission: /Users/you/guest-book/db/*
```

The error shows both values as resolved patterns: an existing directory appears as `directory/*`, because it covers everything beneath it.

## Limits and edge cases

- Bento options after the target are not Bento options. They belong to the script, or make a task invocation fail because they come without `--`.
- A cancelled task stops at the command the signal reached, even when that command handles the signal and exits with `0`; its later commands and the tasks that depend on it do not run.
- Bento needs every environment variable it starts with to be valid UTF-8, and reports the first one that is not.
- Permissions restrict guest code, not task commands. A task is a trusted operation on your behalf, and its commands run with your user's full authority.
- A restricted launch is application policy, not a sandbox. Subprocesses it is allowed to start and native addons it is allowed to load run outside every permission, as in Node.
- `fork()`, and code that starts `process.execPath` with Node's options, fails until Bento accepts Node's command line ([#260](https://github.com/bento-js/bento/issues/260)).
- Bento has no audit mode (`--permission-audit`, [#261](https://github.com/bento-js/bento/issues/261)), does not read permissions from Node's configuration files ([#262](https://github.com/bento-js/bento/issues/262)), and has no `--deny-*` flags, as Node has none. `--allow-child-process` allows every command; neither Bento nor Node 26 can limit it to particular ones.
- A `BENTO_PERMISSION_CEILING` that Bento cannot read stops the launch instead of allowing anything. Bento sets it for task commands and for processes a restricted guest starts; you do not set it yourself.
