---
id: proposal.0003
title: Node's Permission Model
authors: [Mark]
status: awaiting-implementation
pull-request: https://github.com/bento-js/bento/pull/259
issues: [https://github.com/bento-js/bento/issues/266]
supersedes: []
---

# Node's Permission Model

## Summary

Bento adopts Node 26's permission model. A `bento run` launch is unrestricted unless it passes `--permission`, and Node's `--allow-*` flags say what a restricted launch may do. Rust hosts must choose restricted or unrestricted explicitly. System information and subprocesses become available as they are in Node.

## Motivation

Bento denies guest code everything until the launch grants it. To run their own project, a developer names every kind of access: `bento run --allow-read --allow-write --allow-net --allow-env --allow-addons node_modules/vite/bin/vite.js build`. Even then, current tools fail, because two kinds of access cannot be granted at all.

A spike on 2026-11-25 ran freshly scaffolded projects on the latest releases under `main`'s CI build (`9c64f6f`) with every flag Bento has. Node 26.13.1 passed every run.

| Run | Under Bento |
| --- | --- |
| Vite 8.3.8 build and dev server (React and Tailwind) | CSS fails: Vite's PostCSS configuration lookup calls `os.homedir()`, which Bento denies. |
| Astro 7.4.13 build and dev server | Fails at startup on `os.homedir()`. |
| `react-router-serve` 8.5.3 | Fails on `os.networkInterfaces()` unless `HOST` is set. |
| `react-router dev` 8.5.3, Remix 3.3.0 HMR, Vitest 5.1.2's default pool | Fail: each starts a subprocess, and Bento refuses every guest subprocess. |

The denial messages tell the user to pass `--allow-sys`, a flag Bento does not have; the wording comes from the Deno crates underneath. With the system-information calls stubbed out, the Vite build and dev server, the Astro build and dev server, and `react-router-serve` all ran.

These tools are written for Node, and Node trusts the code it is asked to run: nothing is restricted unless the command says `--permission`. Bento's opposite default makes a developer's own project the hard case. Its flags are also a second vocabulary beside Node's, so restricting a command means learning another runtime's names for the same idea.

There is no workaround for system information or subprocesses; for everything else, the workaround is to pass every flag on every command.

## Domain grounding

### Established context

- Node's security policy is that Node "trusts any code it is asked to run". Its permission model is opt-in through `--permission` and has been stable since v23.5.0 and v22.13.0. ([Node 26 permissions](https://github.com/nodejs/node/blob/v26.13.1/doc/api/permissions.md))
- Under `--permission`, Node 26.13.1 restricts file system access through `node:fs`, network access, child processes, worker threads, the inspector, WASI, native addons, FFI, and OpenSSL STORE loaders, and refuses to mount a virtual file system. The flags that lift each restriction are `--allow-fs-read`, `--allow-fs-write`, `--allow-net`, `--allow-child-process`, `--allow-worker`, `--allow-inspector`, `--allow-wasi`, `--allow-addons`, `--allow-ffi`, `--allow-openssl-store`, and `--allow-fs-vfs`. ([Node 26 command-line options](https://github.com/nodejs/node/blob/v26.13.1/doc/api/cli.md))
- The file system flags take `*`, an absolute path, or a path relative to the working directory, and are repeated for several paths. A `*` matches every path beginning with the text before it; characters after it are ignored, so `/home/test*` covers `/home/test/file1` and `/home/test2`. An existing directory covers everything beneath it, while a path that does not exist covers only itself. Commas no longer separate paths: a value containing one is a single path, and Node prints a warning saying comma-separated lists are no longer valid. The entrypoint and `--require` preloads are readable without a flag. Every other module, including installed packages, needs read permission; in a local check, requiring a package without it was denied while reading the package's `package.json`.
- Observed with Node 26.13.1:
    - `--allow-*` without `--permission` fails before running anything with `TypeError [ERR_MISSING_OPTION]: --permission is required` and status 1.
    - `--allow-fs-read` without a value fails with "requires an argument".
    - `--allow-net=example.com` and `--allow-net=false` both allow every network operation, because the value is ignored.
    - Under `--permission`, `process.env`, `os.homedir()`, and `os.networkInterfaces()` keep working: Node 26 restricts neither the environment nor system information.
- A denied operation throws an error with `code: 'ERR_ACCESS_DENIED'`, a `permission` property such as `FileSystemRead`, `FileSystemWrite`, `Net`, `ChildProcess`, or `WorkerThreads`, and, for file system operations, a `resource` property naming the path. The message names the flag: `Access to this API has been restricted. Use --allow-fs-read to manage permissions.` A denied native addon throws `ERR_DLOPEN_DISABLED` with `Cannot load native addon because loading addons is disabled.` A denied `fetch` rejects with an error whose `cause` has code `ERR_ACCESS_DENIED`.
- `process.permission` exists only when the permission model is on. `has(scope[, reference])` answers for the scopes `fs`, `fs.read`, `fs.write`, `child`, `openssl.store`, `worker`, `net`, `inspector`, `wasi`, `addon`, and `ffi`; unknown scopes answer `false`. `drop(scope[, reference])`, added in v26.3.0, irreversibly removes a whole scope, or one exactly granted resource; in a local check, dropping a resource that was never granted changed nothing. ([Node 26 `process.permission`](https://github.com/nodejs/node/blob/v26.13.1/doc/api/process.md))
- A child Node process started with `fork()` or `spawn()` receives the parent's permission flags, through its arguments or `NODE_OPTIONS`. Node warns that `--allow-child-process`, `--allow-worker`, and `--allow-addons` "could invalidate the permission model". A worker receives the parent's flags unless the code that creates it passes `execArgv`, which replaces them; Node documents that as intended.
- Node documents three limitations: symbolic links are followed even to locations outside the granted paths, operations on existing file descriptors bypass the model, and modules such as `node:sqlite` are not covered.
- Node v27.0.0-alpha.0.2.0 adds `--allow-env`. With the model on, every variable `--allow-env` does not match is removed at startup, except a fixed list Node and its dependencies read, such as `PATH`, `HOME`, and `NODE_OPTIONS` (the list is in `src/permission/env_permission.cc`), and the variables from `--env-file` files. Values are `*`, a name, or a prefix followed by `*`, repeated or comma-separated. `NODE_ENV` is not kept. Reading a removed variable returns `undefined` and warns the first time. Variables set at runtime are unrestricted. `drop('env', name)` removes a variable. ([Node 27 alpha `--allow-env`](https://github.com/nodejs/node/blob/v27.0.0-alpha.0.2.0/doc/api/cli.md))
- Deno is the opposite: no access by default, `-A` grants everything, and `--deny-*` overrides `--allow-*`. ([Deno security](https://docs.deno.com/runtime/fundamentals/security/)) Bento's current model has Deno's shape.

### Relevant constraints and principles

- The vision asks Bento to "make host authority explicit", and says "the host decides what code may do" and "a script can have Node-compatible APIs without automatically inheriting the host process's privileges." The CLI is a host. Its decision, like Node's, is to trust what it is asked to run unless the command says otherwise. An embedding host states its own decision every time it creates a runtime.
- The vision's non-goals include "not a sandbox merely because it uses isolates." A restricted launch is application policy. Subprocesses and native addons run outside it, as they do in Node.
- "Compatibility is behavior, not a list of names." Flags, `process.permission`, and errors spelled like Node's must behave like Node's, or the difference must be stated.

### Quality bar

- For every flag, `process.permission` call, and denied operation in this design that Node 26.13.1 also has, Bento produces Node's result for the same command and script: whether the command line is accepted, what `has()` answers, and the denial's `code`, `permission`, `resource`, and message. Environment behavior is measured against Node v27.0.0-alpha.0.2.0. The differences listed under [Where Bento differs from Node](#where-bento-differs-from-node) are the only exceptions.
- Without flags, the spike's Vite 8.3.8 build and dev server, Astro 7.4.13 build and dev server, and `react-router-serve` 8.5.3 run under Bento with no stubs.

### Remaining uncertainty

- Bento enforces permissions through the Deno permission crate, whose kinds are Deno's: read, write, net, env, sys, run, ffi, and import. Whether Node's trailing wildcards and its rule that a missing path covers only itself fit that crate, or need a checker Bento owns, is an implementation question that does not change the behavior above.
- `process.execPath` is the `bento` executable. Tools that start it again with Node's own options fail until Bento accepts Node's command line, which is a separate feature. How many tools beyond the spike's three do that is unknown.

## Existing baseline

`bento_host::Grants` holds a scope for reading, writing, network endpoints, environment variable names, and native-addon paths, plus a boolean for the local language model. `Grants::default()` denies everything, and `RuntimeConfig::new(root)` starts from it. The CLI's `--allow-read`, `--allow-write`, `--allow-net`, `--allow-env`, and `--allow-addons` grant a whole kind bare, or comma-separated items with `=`; `--allow-language-model[=true|false]` toggles the model. `bento_runtime::permissions` maps the grants onto Deno's permission options. It allows only the `uid` and `gid` system queries and no subprocesses, and `subprocess.mjs` throws `NotCapable: Guest subprocess execution is unavailable in Bento`.

A task passes its launch's grants to nested `bento` commands in `BENTO_GRANT_CEILING`. A nested launch inherits the kinds it does not mention and fails if it asks for more than the ceiling allows.

Worth keeping:

- Following file system operations check the fully resolved target, including dangling links and `..` after a link. Link operations check the canonical parent and act on the link itself, and grant paths are resolved to real locations.
- Enforcement happens at native operation boundaries, permissions are fixed per instance and never prompt, and each instance owns its own environment.
- A nested launch can never widen the authority it inherited.
- The language model is a capability of its own.

Incidental, and replaced by this proposal:

- The Deno-shaped vocabulary and flags.
- Denying everything by default in the CLI.
- The blanket denials of system information and subprocesses. These are Bento policy; the Deno crates implement both.
- The `=false` form, which exists only to narrow an inherited grant.

## Proposed solution

Bento runs code the way Node does. A launch is unrestricted until the command says `--permission`; Node's flags then grant what the code may do, and denials look the way they do in Node.

```text
bento run node_modules/vite/bin/vite.js build                 → builds; nothing is restricted
bento run --permission server.ts                               → server.ts loads; its first import is denied (ERR_ACCESS_DENIED, FileSystemRead)
bento run --permission --allow-fs-read=. --allow-net server.ts → serves; writes, subprocesses, workers, and addons are denied
bento run --allow-net server.ts                                → fails before running: --permission is required
```

An embedding host passes its choice to `RuntimeConfig::new`: an unrestricted runtime, optionally denying some of what Node's flags can allow, or a restricted runtime with what it allows. No constructor chooses for it.

## Detailed design

### Vocabulary

A **permission** is the authority to use one kind of resource; it replaces the word "grant" in Bento's interfaces and documentation. A launch or runtime instance is **restricted** when it holds only the permissions it was explicitly allowed, and **unrestricted** otherwise. A restricted instance is **allowed** a permission; an unrestricted embedding may **deny** one.

| Permission                     | CLI flag                 | `has()` scope                   |
| ------------------------------ | ------------------------ | ------------------------------- |
| Read files                     | `--allow-fs-read`        | `fs.read` (and `fs` with write) |
| Write files                    | `--allow-fs-write`       | `fs.write` (and `fs` with read) |
| Network                        | `--allow-net`            | `net`                           |
| Environment variables          | `--allow-env`            | `env`                           |
| Child processes                | `--allow-child-process`  | `child`                         |
| Worker threads                 | `--allow-worker`         | `worker`                        |
| Native addons                  | `--allow-addons`         | `addon`                         |
| WASI                           | `--allow-wasi`           | `wasi`                          |
| FFI                            | `--allow-ffi`            | `ffi`                           |
| Inspector                      | `--allow-inspector`      | `inspector`                     |
| OpenSSL STORE loaders          | `--allow-openssl-store`  | `openssl.store`                 |
| Mounting a virtual file system | `--allow-fs-vfs`         | none, as in Node                |
| Local language model           | `--allow-language-model` | `language-model`                |

The last row is Bento's own; Node has no language model.

### Unrestricted launches

- A `bento run` launch without `--permission`, and without a restricted ceiling from an invoking launch, is unrestricted. Guest code holds every permission in the table. It can read and write any file the user can, connect and listen anywhere, see the whole prepared environment, start subprocesses and workers, load native addons, and use the local language model when the build includes it.
- `process.permission` is `undefined`, as in Node.
- System information is available in every launch, restricted or not, because Node restricts none of it: `os.homedir()`, `os.hostname()`, `os.userInfo()`, `os.networkInterfaces()`, `os.cpus()`, the memory, load, uptime, and release queries, and their `Deno` counterparts.
- Some operations stay unavailable because they are not permissions: importing remote modules, changing the host process's working directory or loading an environment file into it (`process.chdir`, `process.loadEnvFile`), and terminating or signalling the host (`process.abort`, `process.kill`, `Deno.kill`). This proposal does not change them.

### Restricted launches

- `--permission`, given before the target, makes the launch restricted: guest code holds only the permissions its `--allow-*` flags give it.
- The file `bento run` executes is readable without `--allow-fs-read`. Only that file is implied: the modules it imports, including installed packages, need read permission.
- For a task, the flags govern the task's nested `bento` launches, as [Nested launches](#nested-launches) describes. Task commands themselves remain trusted host operations that no permission restricts.

### Command line

```text
bento run [--permission] [--allow-fs-read=PATH]... [--allow-fs-write=PATH]... [--allow-env=NAME]...
          [--allow-net] [--allow-child-process] [--allow-worker] [--allow-addons] [--allow-wasi]
          [--allow-ffi] [--allow-inspector] [--allow-openssl-store] [--allow-fs-vfs]
          [--allow-language-model] [--watch] <script-path | task-name> [args...]
```

- `--allow-fs-read` and `--allow-fs-write` require a value, given as `--flag=VALUE` or `--flag VALUE`; without one, the launch fails with `--allow-fs-read requires an argument`. Each may be repeated.
    - `*` allows every path.
    - Any other value is a path, absolute or relative to the directory `bento` runs in. A value containing `*` covers every path that begins with the text before the first `*`, and everything after that `*` is ignored. Without `*`, an existing directory covers itself and everything beneath it, and anything else (a file, or a path that does not exist yet) covers exactly that path. A directory the guest will create needs a trailing `*`, as in `--allow-fs-write=./build/*`.
    - A comma is part of the path. When a value contains one, Bento prints Node's warning to standard error, naming the flag: `The --allow-fs-read CLI flag has changed. Passing a comma-separated list of paths is no longer valid. Documentation can be found at https://nodejs.org/api/permissions.html#file-system-permissions`, and continues.
- `--allow-env` takes `*`, a variable name, or a name prefix followed by `*`, such as `APP_*`. Values may be repeated or comma-separated. An empty item, as in `--allow-env=` or `--allow-env=PORT,,HOST`, is an error.
- The other flags are boolean and take no value. Giving one a value, as in `--allow-net=127.0.0.1:3000`, is an error that names the flag.
- Any `--allow-*` flag without `--permission` fails the launch before anything runs, with `--permission is required`.
- These launch errors exit with status 1 and print a message, like Bento's other usage errors.
- `--allow-read`, `--allow-write`, `--allow-language-model=BOOL`, and values for `--allow-net` and `--allow-addons` are removed. Bento reports them as it reports any unknown or malformed option.
- `bento run --help` lists the flags under a "Permissions" heading.

### File system paths and links

- Bento resolves each path value to its real location before comparing it, as it does today. A value with `*` has its existing directory part resolved and the rest kept as written. On macOS, `--allow-fs-read=/tmp/app` therefore covers `/private/tmp/app`.
- Following file system operations check the fully resolved target, so a link inside an allowed directory that points outside it is denied. No-follow operations keep today's behavior.
- In a restricted launch, creating a symbolic link requires `--allow-fs-read=*` and `--allow-fs-write=*`, because a scoped permission cannot vouch for a target resolved later. This is today's rule, restated in the new flags.

### Environment variables

- An unrestricted instance sees the whole prepared environment: inherited variables, the project's environment files, and the watch-mode `NODE_ENV` default, with the precedence [Tasks and Environment](../guides/tasks-and-environment.md) describes.
- When a restricted instance starts, Bento removes every variable that `--allow-env` does not match, except:
    - the variables in Node's always-kept list at v27.0.0-alpha.0.2.0 (`src/permission/env_permission.cc`); and
    - the values Bento itself prepared for the launch: those read from the project's environment files, and the watch-mode `NODE_ENV` default. These are Bento's counterpart of Node's `--env-file` values. When a variable comes from both an environment file and the inherited environment and `--allow-env` does not match it, the inherited value is removed and the file's value is kept.
- An inherited `NODE_ENV` is not kept unless `--allow-env` matches it, as in Node.
- Reading a removed variable returns `undefined`. The first read of each removed variable emits a process warning with Node's text: `The permission model removed the environment variable "NAME" at startup. Use --allow-env to manage permissions.`
- Guest code may set, change, and delete any variable at runtime. These changes affect only the instance's environment, never the embedding process's.
- Subprocesses a guest starts receive the instance's environment after removal. Task commands still receive the whole prepared environment.

### Subprocesses

- `node:child_process` (`spawn`, `exec`, `execFile`, `fork`, and their synchronous forms) and `Deno.Command` work in unrestricted instances, and in restricted instances allowed `--allow-child-process`. Otherwise they throw Node's `ERR_ACCESS_DENIED` with permission `ChildProcess`, and no process starts.
- A child process runs with the user's full authority. No permission restricts it, so allowing child processes in a restricted launch lets the code escape every other restriction. The guides say so beside the flag.
- When a restricted instance starts a process, Bento adds `BENTO_PERMISSION_CEILING` to the child's environment, describing the instance's permissions at that moment. A `bento` started by the child stays within them, as in [Nested launches](#nested-launches). Bento does not change `NODE_OPTIONS`.
- `subprocess.kill()` signals the child. `process.kill()` stays unavailable, as above.
- `process.execPath` is the `bento` executable, and Bento does not yet accept Node's command-line options. `fork()`, and any code that starts `process.execPath` with Node's options, therefore fails with Bento's usage error from the child. The guides record this limitation until Bento accepts Node's command line.

### Workers

- `worker_threads.Worker` and the Web `Worker` work in unrestricted instances and in restricted instances allowed `--allow-worker`. Otherwise they throw Node's `ERR_ACCESS_DENIED` with permission `WorkerThreads`.
- A worker always has its creator's current permissions. A worker's `execArgv` cannot change them, and Deno's `deno.permissions` worker option still throws `NotSupported`.

### Native addons

- Native addons load in unrestricted instances and in restricted instances allowed `--allow-addons`. Otherwise loading throws Node's `ERR_DLOPEN_DISABLED` error, and the addon's native code never runs.
- A loaded addon runs native code inside the `bento` process, outside every permission. The guides keep saying so.

### WASI, FFI, the inspector, OpenSSL STORE loaders, and virtual file systems

Bento accepts these flags, records them, and reports them through `process.permission.has()`. Where Bento provides the API a flag governs, the permission gates it with Node's error. Where it does not, the API fails as it does today. Providing those APIs is outside this proposal.

### The local language model

An unrestricted instance may use the local language model when the build includes a backend. A restricted instance needs `--allow-language-model`; without it, `LanguageModel.availability()` reports `unavailable` and `LanguageModel.create()` rejects with `NotAllowedError`, as [proposal 0002](0002-web-prompt-api.md) specifies.

### Denials

- In a restricted instance, a denied operation throws what Node 26.13.1 throws for the same operation: the same error code, `permission`, `resource` when Node includes one, and message. Network denials reach guest code through the same paths as in Node; a denied `fetch`, for example, rejects with an error whose `cause` is the `ERR_ACCESS_DENIED` error.
- `Deno` namespace APIs throw the same errors as their Node counterparts. The language model keeps the Web API's `NotAllowedError`.
- The CLI reports an uncaught denial as a permission failure, with the source location Bento shows today, and exits with status 1.

### `process.permission`

- `process.permission` is defined in every instance that is denied anything: every restricted instance, and an unrestricted embedding with at least one denial. Workers of such an instance have it too.
- `has(scope[, reference])` takes the scopes in the table plus `fs`, and returns `false` for any other scope. For the file system scopes, the reference may be a path string relative to `process.cwd()`, a `file:` URL, or a `Uint8Array` of path bytes, and is resolved to its real location as checks are. For `env`, the reference is a variable name. Other scopes ignore the reference. `has()` without a reference answers whether the whole scope is allowed: `has('fs.read')` is `true` only after `--allow-fs-read=*`.
- `drop(scope[, reference])` irreversibly removes the whole scope, or with a reference, the one allowance given by exactly that value. A reference that matches no allowance changes nothing, as in Node 26.13.1. Dropping `env` with a name removes that variable from the environment. Dropping the whole `env` scope removes every variable except the always-kept ones.
- A drop affects the whole instance and every worker it has created, and the ceiling given to processes it starts afterward. Files, sockets, child processes, and workers that are already open stay open.
- `Deno.permissions.query()` reports the instance's permissions in Deno's vocabulary. `request()` and `revoke()` still fail with `NotSupported`.

### Nested launches

- `BENTO_GRANT_CEILING` is renamed `BENTO_PERMISSION_CEILING`. A task command, or a process a restricted guest starts, receives the ceiling of the launch or instance it came from.
- A nested launch without `--permission` holds exactly its ceiling's permissions. Under an unrestricted ceiling it is unrestricted; under a restricted one it is restricted the same way.
- A nested launch with `--permission` holds exactly what its own flags and its entrypoint allow, the same as a launch from the shell. Each flag must fit within the ceiling: a flag that asks for more fails before any guest code runs, naming the flag and the inherited limit. Permissions its flags do not mention are denied even when the ceiling holds them.
- A nested launch's entrypoint is implicitly readable only when the ceiling's read permission covers it.

### Where Bento differs from Node

These are the only intended differences from Node 26.13.1, and from Node v27.0.0-alpha.0.2.0 for the environment:

- **Paths are compared at their real locations, and links are followed before checking.** Node compares paths as written and documents following links outside the granted paths as a known limitation.
- **Boolean flags reject values.** Node accepts `--allow-net=anything` and allows every host. Bento rejects the value, so a command written for Bento's earlier `--allow-net=HOST` cannot silently allow every host.
- **Workers keep their creator's permissions.** In Node, a worker created with an explicit `execArgv` loses the parent's permission model.
- **Nested launches are bounded.** A nested `bento` cannot widen its ceiling. Node passes its flags to child Node processes, which can add their own.
- **`process.permission` exists in an unrestricted embedding that denies something.** Node has no such configuration.
- **Bento prints none of Node's `ExperimentalWarning` or `SecurityWarning` messages about permission flags.** Those report Node's own stability levels. The comma warning and the removed-variable warning are kept, because they explain an outcome.
- **Command-line errors exit with status 1**, Bento's status for usage errors. Node exits with 9 when a flag is missing its argument.

### Rust embedding API

- `RuntimeConfig::new(project_root, permissions)` takes a required `Permissions` value, and `RuntimeConfig.permissions` replaces `RuntimeConfig.grants`. `Permissions` implements no `Default`, and no constructor chooses a mode on the host's behalf.
- A `Permissions` value is either unrestricted, optionally with denials, or restricted, with allowances. Nothing about the CLI is implicit in the library: a restricted embedding gets no implied entrypoint read, and the environment rules apply to the `RuntimeConfig.env` map the host supplies.
- Restricted allowances:
    - Reading and writing files: all, or a set of canonical paths and wildcard patterns, with the CLI's semantics.
    - Network: all, or a set of endpoints, each a host or IP address with an optional port, as today's network scope works. Only the library can scope the network to endpoints.
    - Environment variables: all, or a set of names and prefix patterns.
    - Native addons: all, or a set of canonical paths beneath which addons may load. Only the library can scope addons to paths.
    - Child processes, workers, WASI, FFI, the inspector, OpenSSL STORE loaders, mounting a virtual file system, and the local language model: each allowed or not.
- Unrestricted denials are exactly as specific as Node's flags, no more and no less:
    - Reading and writing files: all, or a set of canonical paths and wildcard patterns, matched with the CLI's semantics.
    - Environment variables: all, or a set of names and prefix patterns.
    - Network, child processes, workers, native addons, WASI, FFI, the inspector, OpenSSL STORE loaders, mounting a virtual file system, and the local language model: each denied as a whole, or not.
- A denial takes away exactly what the matching allowance would give. Reading a denied path throws Node's `ERR_ACCESS_DENIED` with permission `FileSystemRead`, as in a restricted instance, while other paths stay readable. Denied environment variables are removed at startup unless they are always kept, and denying the whole environment removes every variable except the always-kept ones.
- In an unrestricted instance, each permission that is not denied is held as if allowed with `*`. `has()` without a reference answers `false` for a kind with any denial. As in Node for a `*` allowance, `drop()` with a reference changes nothing, and `drop()` without one removes the whole kind.
- `bento_host::canonical_path` and `package_lookup_paths` keep their roles for building canonical path allowances.

### Documentation and fixtures

[Running with Bento](../guides/running-with-bento.md), [Embedding Bento](../guides/embedding-bento.md), [Node Compatibility](../guides/node-compatibility.md), [Tasks and Environment](../guides/tasks-and-environment.md), [Using the Prompt API](../guides/prompt-api.md), and the guest-book embedding example are rewritten from this proposal. The guest-book fixture's tasks, the examples, the benchmark harness, and CI's preview commands move to the new flags.

## Compatibility

This change breaks existing commands and Rust callers. Nothing stored needs migrating.

- A command without flags now runs unrestricted. Anyone who relied on a bare `bento run` to contain code must add `--permission`. The guides say this first.
- Commands using the old flags fail until rewritten:

| Before | After |
| --- | --- |
| `--allow-read` / `--allow-read=a,b` | `--permission --allow-fs-read=*` / `--permission --allow-fs-read=a --allow-fs-read=b` |
| `--allow-write=data` | `--permission --allow-fs-write=data` (add `/*` when `data` does not exist yet) |
| `--allow-net` / `--allow-net=host:port` | `--permission --allow-net` (every host); scope endpoints through the library |
| `--allow-env=A,B` | `--permission --allow-env=A,B` |
| `--allow-addons` / `--allow-addons=path` | `--permission --allow-addons`; scope paths through the library |
| `--allow-language-model` / `=false` | `--permission --allow-language-model` / `--permission` without it |

- In a restricted instance, ungranted environment variables are removed rather than hidden, guest writes to any variable succeed, and values from the project's environment files are visible.
- A nested launch that passes `--permission` no longer inherits the permissions its flags leave out.
- Code that checks for Deno's `NotCapable` or `PermissionDenied` errors must check for Node's `ERR_ACCESS_DENIED` and `ERR_DLOPEN_DISABLED` instead.
- Rust callers pass a `Permissions` value to `RuntimeConfig::new`; `Grants`, `GrantRequest`, and `RuntimeConfig.grants` are replaced.
- For the CLI, this replaces proposal 0001's deny-by-default flags, its refusal of subprocesses and system information, and proposal 0002's requirement that the CLI grant the language model explicitly. For the library, proposal 0002's default of no model access becomes the host's explicit choice. Neither proposal is superseded as a whole.

## Implications on adoption

Nothing new is installed. Projects and scripts that pass the old flags must rewrite them using the table above. Running untrusted code now requires `--permission`, and the guides say so before anything else about permissions. A release containing this change can be reverted without data migration.

## Scope

- Unrestricted CLI launches, `--permission`, and the flags, syntax, and errors above.
- Enforcement of each permission, including the environment rules, worker gating, and enabling guest subprocesses.
- Making system information available in every launch.
- Node-shaped denial errors and `process.permission` with `has()` and `drop()`.
- Nested-launch ceilings under the new rules.
- The `Permissions` embedding API.
- Differential tests against Node 26.13.1, and Node v27.0.0-alpha.0.2.0 for the environment, for every behavior Node shares, plus tests for each listed difference.
- The rewritten guides and the migrated fixtures, examples, benchmark harness, and CI commands.

### Out of scope

- Node's audit mode (`--permission-audit`). It reports instead of denying and is a feature of its own ([#261](https://github.com/bento-js/bento/issues/261)).
- Reading permissions from Node's configuration file (`--config-file`'s `permission` namespace). Bento does not read Node configuration files at all yet ([#262](https://github.com/bento-js/bento/issues/262)).
- Accepting Node's command line, so that `process.execPath`, `fork()`, and `#!/usr/bin/env node` executables work under Bento. This is a separate feature with its own decisions ([#260](https://github.com/bento-js/bento/issues/260)).
- Implementing `node:wasi`, `node:ffi`, inspector sessions, OpenSSL STORE loaders, or `node:vfs`. Their flags are recorded here; the APIs are Node compatibility work.
- `process.chdir`, `process.loadEnvFile`, and `process.kill`. These are Node compatibility questions about the host process, not permissions ([#91](https://github.com/bento-js/bento/issues/91)).
- `--deny-*` flags on the command line. Node has none.
- Restricting child processes to particular commands. Node 26 has no such form.

## Preview

- Artifact: the macOS arm64 `bento` executable archive that CI publishes for this pull request.
- Reason: the change is the CLI's and runtime's behavior, and the executable is the cheapest way to exercise it. The reviewer runs freshly scaffolded Vite, Astro, and React Router projects without flags; runs the same `--permission` scripts under `bento` and Node 26.13.1 side by side; and builds the guest-book embedding example from the branch to exercise the `Permissions` API.

## Policies and decisions checked

- policy.0001 (prefer VoidZero crates) does not apply: permissions involve no parsing, transformation, or resolution role. Any path-matching code Bento adds is its own.
- `decisions/` holds no decisions.
- `VISION.md`: the CLI states its default openly, and embedders state theirs through a required argument, which honors "make host authority explicit" and "the host decides what code may do". The MVP's "host-controlled filesystem, networking, and process capabilities" remain. The vision's revision history records this change when it ships.

## Future directions

- Audit mode ([#261](https://github.com/bento-js/bento/issues/261)), reading permissions from Node configuration files ([#262](https://github.com/bento-js/bento/issues/262)), and accepting Node's command line ([#260](https://github.com/bento-js/bento/issues/260)) each extend this model toward Node.
- When Node 27 is released, its permission model may differ from both Node 26 and the alpha's `--allow-env`. Following it would be a new proposal.

## Alternatives considered

- **Keep denying everything by default, as Deno does.** This makes a developer's own project the hard case, and the tools Bento needs to run assume Node's default.
- **Restrict a launch whenever it passes any `--allow-*` flag, without `--permission`.** Mark chose Node 26's switch for parity; Node 26 rejects `--allow-*` without it.
- **Restrict only the kinds a launch names.** One flag would then sandbox nothing else, and the flags would mean something different from Node's.
- **Keep Bento's endpoint-scoped `--allow-net` and path-scoped `--allow-addons` in the CLI.** Mark chose Node's boolean flags so the CLI never gives a Node flag a different meaning. The library keeps both scopes.
- **Follow Node 26 exactly and leave the environment unrestricted.** Mark chose to keep environment restriction, with Node 27's syntax so Bento will not drift from it.
- **Compare paths as written, as Node does.** Node documents following links outside the granted paths as a limitation, and Bento's resolved checks already exist and are stricter.
- **Accept and ignore boolean flag values, as Node does.** A command written for Bento's earlier `--allow-net=HOST` would then silently allow every host.
- **Give the library a default, restricted or unrestricted.** Mark chose a required argument, so every embedder states the intention.
- **Leave subprocesses to a later proposal.** Mark chose to include them: without them, "unrestricted" would still exclude one of Node's permissions, and `--allow-child-process` would be a flag that does nothing.
- **Let unrestricted embeddings deny only whole kinds, or also deny individual endpoints and addon paths.** Mark chose denials exactly as specific as Node's flags: paths and patterns for files, names and prefixes for the environment, and whole kinds for everything else.
- **Keep project environment-file values hidden from restricted instances, as today.** Node keeps the values of the environment files an operator names, and Bento's project environment files are their counterpart.

## Open questions

None.

## Acknowledgments

Node's permission model documentation and its observed behavior in Node 26.13.1 shaped every flag, error, and API in this proposal.
