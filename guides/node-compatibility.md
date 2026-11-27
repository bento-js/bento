---
id: guide.node-compatibility
title: Node Compatibility
describes: [proposal.0001, proposal.0003]
---

# Node Compatibility

## What it does

Bento executes installed JavaScript applications in-process using V8 and Deno-derived Node and Web compatibility. The milestone targets the locked Remix 3 guest-book dependency graph, not every Node 26 API or arbitrary npm package.

The exercised Node surface includes `node:path`, `node:fs`, `node:fs/promises`, `node:sqlite`, and streaming `node:http` servers. SQLite supports migrations, parameterized statements, persistent databases, and explicit connection cleanup. Servers support `close()` and `closeAllConnections()`.

Available globals include `process.argv`, instance-local `process.cwd()` and `process.env`, `Buffer`, `import.meta.dirname` and `import.meta.filename`, `createRequire`, microtasks, structured cloning, abort controllers, `AsyncLocalStorage`, Fetch requests and responses, form data, and readable streams.

Bento follows Node 26's permission model. A `bento run` command without permission flags is unrestricted, as `node` is, and running code you do not trust requires `--permission`; a Rust host chooses restricted or unrestricted for every instance it creates. For environment variables, Bento follows `--allow-env` from Node v27.0.0-alpha.0.2.0, because Node 26 does not restrict the environment. [Permissions and native code](#permissions-and-native-code) and the sections after it describe the differences a restricted instance makes.

## Module resolution

JavaScript entries and dependencies can use `.js`, `.mjs`, and `.cjs`. CommonJS modules imported by ESM receive generated facades with statically discovered named exports; CommonJS `require()` uses the upstream Node compatibility resolver. The runtime's ESM resolver uses Oxc with Node's `node`, `import`, and `default` conditions, package `exports` and `imports`, and real-path pnpm symlink resolution. Relative ESM imports must name the complete file, including its extension. TypeScript `paths` and `baseUrl` do not change runtime resolution.

Install the project's locked dependencies before execution. A missing package reports the specifier and recommends `bento install`; application startup never invokes an installer or downloads dependencies. An unregistered `host:` import reports that the host did not register the module, without an installation suggestion. Remote URL imports are unavailable in every instance, whatever its permissions. Installation and explicitly trusted project tasks are separate host operations.

Bento transforms `.ts`, `.mts`, `.cts`, `.tsx`, and `.jsx` in memory through `bento_compiler`; plain JavaScript passes through unchanged. Explicit TypeScript extensions in imports work. The project-root `tsconfig.json` and its inherited execution settings are loaded once per instance. Every non-dependency source uses them, including workspace sources outside the project root. Dependencies use default compiler settings rather than inheriting the application's settings: inside the project, files beneath its own `node_modules`; outside it, files with a `node_modules` directory anywhere in their path. A project whose own root has a `node_modules` ancestor still uses its project settings. `noEmit` never disables runtime transformation, and no emitted JavaScript is written to disk.

Configuration diagnostics are found once per instance at startup and returned by `RunningRuntime::diagnostics()`; the runtime prints nothing, so each host reports them as it chooses, and the `bento` CLI prints each one to its stderr. Unsupported JSX execution settings such as `jsx: preserve` and `react-native` allow non-JSX modules to run; only modules containing JSX fail at transform time as `Transform` errors naming the setting and the JSX location. An unreadable or malformed `tsconfig.json` returns a `Transform` error from `start` before execution. Source maps preserve original TypeScript line and column positions in exceptions. Transformation does not perform semantic type checking.

## Permissions and native code

An unrestricted instance holds every permission, as Node does without `--permission`. A restricted instance holds only the permissions it was allowed, as Node does with `--permission`. A Rust host can also create an unrestricted instance that is denied particular permissions. Permissions never prompt, and an instance's permissions can shrink while it runs, through `process.permission.drop()`, but never grow. [Running with Bento](running-with-bento.md#control-what-code-may-do) lists the CLI flags, and [Embedding Bento](embedding-bento.md#permissions-and-environment) the Rust API.

System information is available in every instance, restricted or not, because Node restricts none of it: `os.homedir()`, `os.hostname()`, `os.userInfo()`, `os.networkInterfaces()`, `os.cpus()`, the memory, load, uptime, and OS release queries, and their `Deno` counterparts. The platform and architecture Bento was built for are visible through `process.platform` and `process.arch`.

Following filesystem operations check the fully resolved target, including dangling links and `..` after a symlink. Link operations such as `lstat`, `readlink`, `unlink`, and `rename` check the canonical parent and final component and act on the link itself. In a restricted instance, creating a symlink requires permission to read and write every path (`--allow-fs-read=*` and `--allow-fs-write=*`): permissions for particular paths cannot safely authorize links whose targets are resolved later.

No-follow filesystem operations retain the requested native path, including trailing directory components. `O_NOFOLLOW` rejects a symbolic link, and exclusive creation rejects an existing link even when its target is missing. Hard links preserve the source link, while `rmdir` and nonrecursive `mkdir` retain their native link-error behavior.

Following filesystem operations differ from Node for paths that pass through a missing directory. A `..` after a component that does not exist is resolved lexically, as Deno resolves it, so `missing/../target` addresses `target` instead of failing with `ENOENT`. An empty path addresses the working directory instead of failing with `ENOENT`. Permission checks still apply to the fully resolved target, so neither case reaches a file outside the allowed paths.

A path is checked when the operation starts, and the operating system resolves it again when the operation runs. If the guest itself moves an existing link into a checked path while one of its operations is still pending, that operation can reach the link's target. Permissions are application policy, not a sandbox for deliberately hostile code, so keep links whose targets lie outside the allowed paths out of writable directories when that distinction matters.

In a restricted instance, the environment variables its permissions do not cover are removed when it starts, except the ones Node always keeps and the values the host prepared, as [Tasks and Environment](tasks-and-environment.md#environment-in-a-restricted-launch) describes. Reading a removed variable returns `undefined` and warns the first time, and enumeration omits it. Guest code may set, change, and delete any variable; those changes affect only that runtime's map, never the embedding process environment.

### Subprocesses and workers

`node:child_process` (`spawn`, `exec`, `execFile`, `fork`, and their synchronous forms) and `Deno.Command` work in an unrestricted instance and in a restricted instance allowed child processes. Otherwise they throw `ERR_ACCESS_DENIED` with permission `ChildProcess` and the command as `resource`, and no process starts. A child process runs with the user's full authority, outside every permission, so allowing child processes in a restricted instance lets its code escape every other restriction. `subprocess.kill()` signals the child.

A subprocess receives the instance's environment after the startup removal. When a restricted instance starts a process, Bento adds `BENTO_PERMISSION_CEILING` to the child's environment, describing the instance's permissions at that moment, so a `bento` the child starts stays within them. Bento does not change `NODE_OPTIONS`.

`process.execPath` is the `bento` executable, and Bento does not accept Node's command-line options yet ([#260](https://github.com/bento-js/bento/issues/260)). `fork()`, and any code that starts `process.execPath` with Node's options, therefore fails with Bento's usage error from the child.

`worker_threads.Worker` and the Web `Worker` work in an unrestricted instance and in a restricted instance allowed workers. Otherwise they throw `ERR_ACCESS_DENIED` with permission `WorkerThreads`. A worker always has its creator's current permissions: a worker's `execArgv` cannot change them, and Deno's `deno.permissions` worker option throws `NotSupported`.

### Native addons

The locked `oxc-parser`, `oxc-resolver`, `oxc-transform`, `oxc-minify`, and `lightningcss` packages use their actual Node-API binaries. Addons load in an unrestricted instance and in a restricted instance allowed addons, from any path. Otherwise loading throws `ERR_DLOPEN_DISABLED` with the message `Cannot load native addon because loading addons is disabled.` before the addon initializes, and the diagnostic identifies the package or guest call site.

Native-loading executables must export the Node-API ABI symbols. With the matching `deno_napi = "=0.190.0"` build dependency, call `deno_napi::print_linker_flags("your-binary-name")` from the executable's `build.rs`. Linking the runtime alone does not prevent the linker from stripping addon symbols.

Remix asset discovery probes `<ancestor>/node_modules` for optional dependencies all the way to the filesystem root. A restricted instance whose read permission covers only the project root fails with an `ERR_ACCESS_DENIED` error pointing into the asset code, even when the probed dependency does not exist. For scoped reads, allow the canonical project root plus every path returned by `bento_runtime::package_lookup_paths(&root)?`. This helper includes the project's own `node_modules`, resolves symlinks, and preserves nonexistent path tails; it returns an I/O error if a path cannot be canonicalized. Hosts choose whether to add these paths. Bento never broadens read permissions or disguises denials as missing files.

The fixture's restricted CLI command allows reading every path for convenience. Embedded hosts can instead scope reads to the project and trusted package-lookup roots, and writes to the database directory; native addons, like the network, are allowed or denied as a whole. Inspect canonical lookup paths before allowing them: an ancestor's `node_modules` symlink makes its whole target tree readable. A project beneath an ancestor writable by other users, such as a shared temporary directory, requires particular care.

An addon that loads executes native code inside the Rust host process. It is not contained by JavaScript filesystem, network, or environment permissions. Only enable trusted native packages; permissions are application policy, not a hostile-code sandbox. The real Remix asset orchestration still controls development source maps and stable URLs, and production minification and fingerprinting.

The native Oxc resolver also reads the embedding process's `NODE_PATH`. If that variable points at another project's packages, asset resolution can leave the installed fixture graph, and a read permission scoped to the project will reject the result. Unset host-process `NODE_PATH` before launching when resolution must stay within the prepared project. A guest's environment permissions do not constrain native addon reads of the host environment, and Bento does not mutate the embedding process's environment to hide them.

## Inspect and drop permissions

`process.permission` is defined in every instance that is denied anything: every restricted instance, and an unrestricted embedding with at least one denial. Workers of such an instance have it too. Elsewhere it is `undefined`, as in Node without `--permission`.

`process.permission.has(scope[, reference])` answers for the scopes `fs`, `fs.read`, `fs.write`, `net`, `env`, `child`, `worker`, `addon`, `wasi`, `ffi`, `inspector`, `openssl.store`, and Bento's own `language-model`, and returns `false` for any other scope. `fs` answers for reading and writing together. For the file system scopes, the reference may be a path string relative to `process.cwd()`, a `file:` URL, or a `Uint8Array` of path bytes, and Bento resolves it to its real location as it does for checks. For `env`, the reference is a variable name. Other scopes ignore the reference. Without a reference, `has()` answers whether the whole scope is allowed: `has("fs.read")` is `true` only when every path is readable, as after `--allow-fs-read=*`. In the CLI, the file `bento run` executes is readable, so `has("fs.read", entry)` is `true` for it.

An unrestricted instance holds each permission it is not denied as if allowed with `*`. `has()` without a reference answers `false` for a scope with any denial.

`process.permission.drop(scope[, reference])` irreversibly removes the whole scope, or, with a reference, the one allowance given by exactly that value. A reference that matches no allowance changes nothing, as in Node 26.13.1, and so does any reference when the whole scope is allowed with `*`. Dropping `env` with a name removes that variable from the environment, and dropping the whole `env` scope removes every variable except the always-kept ones. A name containing `*` throws a `TypeError` with code `ERR_INVALID_ARG_VALUE`: `Wildcards are not supported when dropping environment variables`.

A drop affects the whole instance, every worker it has created, and the ceiling given to processes it starts afterward. Files, sockets, child processes, and workers that are already open stay open.

`Deno.permissions.query()` and `querySync()` report the instance's permissions in Deno's vocabulary. `request()`, `requestSync()`, `revoke()`, and `revokeSync()` fail with `NotSupported`; Bento never prompts.

## Denied operations

In a restricted instance, a denied operation throws what Node 26.13.1 throws for the same operation: the same error code, `permission`, `resource` when Node includes one, and message.

| Denied operation | Error |
| --- | --- |
| Reading a file | `ERR_ACCESS_DENIED`, `permission: 'FileSystemRead'`, `resource` the path as the call named it |
| Writing a file | `ERR_ACCESS_DENIED`, `permission: 'FileSystemWrite'`, `resource` the path as the call named it |
| Connecting or listening | `ERR_ACCESS_DENIED`, `permission: 'Net'` |
| Starting a child process | `ERR_ACCESS_DENIED`, `permission: 'ChildProcess'`, `resource` the command |
| Starting a worker | `ERR_ACCESS_DENIED`, `permission: 'WorkerThreads'` |
| Loading a native addon | `ERR_DLOPEN_DISABLED`, `Cannot load native addon because loading addons is disabled.` |

An `ERR_ACCESS_DENIED` message names the flag that manages the permission, as in `Access to this API has been restricted. Use --allow-fs-read to manage permissions.` Network denials reach guest code through the same paths as in Node: a denied `fetch` rejects with an error whose `cause` is the `ERR_ACCESS_DENIED` error. `Deno` namespace APIs throw the same errors as their Node counterparts. The local language model keeps the Web API's `NotAllowedError`, as [Using the Prompt API](prompt-api.md) describes.

Code that checked for Deno's `NotCapable` or `PermissionDenied` errors must check for `ERR_ACCESS_DENIED` and `ERR_DLOPEN_DISABLED` instead.

Bento accepts the WASI, FFI, inspector, OpenSSL STORE, and virtual file system permissions, records them, and reports them through `process.permission.has()`. Where Bento provides the API one of them governs, the permission gates it with Node's error: in a restricted launch without `--allow-wasi`, `new WASI()` throws `ERR_ACCESS_DENIED` with `permission: 'WASI'`, and without `--allow-inspector`, `inspector.open()` and `Session#connect()` throw it with `permission: 'Inspector'`. Bento provides no `node:ffi`, OpenSSL STORE loaders, or virtual file systems yet, so those fail whatever the permission says.

## Where Bento differs from Node

These are the only intended differences from Node 26.13.1's permission model, and from Node v27.0.0-alpha.0.2.0's for the environment:

- Paths are compared at their real locations, and links are followed before checking. Node compares paths as written and documents following links outside the allowed paths as a known limitation.
- Boolean flags reject values. Node accepts any value for `--allow-net` and allows every host. Bento rejects the value, so a command written for Bento's earlier host-scoped network flag cannot silently allow every host.
- Workers keep their creator's permissions. In Node, a worker created with an explicit `execArgv` loses the parent's permission model.
- Nested launches are bounded. A nested `bento` cannot widen its ceiling. Node passes its flags to child Node processes, which can add their own.
- `process.permission` exists in an unrestricted embedding that denies something. Node has no such configuration.
- Bento prints none of Node's `ExperimentalWarning` or `SecurityWarning` messages about permission flags; those report Node's own stability levels. The comma warning and the removed-variable warning are kept, because they explain an outcome.
- Command-line errors exit with status 1, Bento's status for usage errors. Node exits with 9 when a flag is missing its argument.
- A dropped permission is gone from workers that are already running, as from the rest of the instance. In Node, `process.permission.drop()` leaves running workers unchanged.
- Bento warns about a comma in every `--allow-fs-read` or `--allow-fs-write` value. Node warns only when the flag has exactly one value, and it counts the entry script among the values, so it never warns when running a script.

Bento also has a permission Node lacks, for the local language model, and operations that are unavailable in every instance because they concern the host process rather than a permission: `process.chdir`, `process.loadEnvFile`, `process.abort`, `process.kill`, and `Deno.kill`.

## Node compatibility decisions

`EventTarget.addEventListener(type, listener, { signal })` accepts Node-style signal objects with an `aborted` property. Already-aborted signals prevent registration, and an `abort` event removes the listener. Native `AbortSignal` objects retain their ordinary behavior. Remix 3's server renderer relies on this duck-typed signal behavior; strict branded-signal validation would break rendering.

`server.listen(port)` with no host listens on `::`, accepting IPv4 and IPv6 connections when the system supports dual-stack IPv6. IPv4-only systems fall back to `0.0.0.0`. An omitted host does not mean loopback-only; permissions and deployment policy must account for the listening address.

## Observable failures

Runtime outcomes distinguish completed execution, guest exits, failures, and host-requested shutdown. Errors identify startup or syntax failures, module resolution failures, transformation failures, permission violations, and uncaught exceptions or unhandled rejections. Diagnostics include the entry or failing source location instead of presenting a failed launch as successful execution. Guest `process.exit` disposes only the runtime, including inside HTTP handlers; it never exits the Rust embedding process.

Unsupported upstream features, including `Worker`'s `deno.permissions` option and listeners with `reusePort`, throw `NotSupported` before creating the worker or listener. Guests can catch the error; an uncaught error produces an `Uncaught` instance failure with its guest source location, not a startup failure or a process exit. `process.abort()`, `process.kill()`, and `Deno.kill()` fail explicitly because process-global termination and signal delivery belong to the host.

`RuntimeError::location()` returns an optional `SourceLocation { path, line, column }` with one-based original-source positions. Transform errors use the compiler diagnostic; uncaught errors and unhandled rejections use the first non-dependency source stack frame, including sources outside the project root; resolution errors identify the importing module and, when available, the import literal. Dependency classification is the same as for compiler settings: check `node_modules` components relative to the project root for sources inside it, and anywhere in the absolute path for sources outside it. Environment-file contents are never used as diagnostic source locations. `RuntimeError.message` is plain text without ANSI escape sequences regardless of terminal color settings; the host or CLI chooses presentation. Unterminated terminal sequences in guest messages do not hide subsequent stack frames or causes, and ordinary tabs and line breaks are preserved. A new exception copying a caught module failure's message remains `Uncaught` at its own throw site when its class differs. A new exception that copies both the class and the exact message, such as `new TypeError(caught.message)` after a failed import, is reported as the caught failure, with its kind and location. Rethrowing a caught resolution failure preserves its `Resolution` kind and location. A dynamic import of a source that fails to transform rejects with a `TypeError`, as in Deno, so rethrowing it reports `Uncaught`. Guest `SyntaxError` exceptions, such as a failed `JSON.parse`, are also `Uncaught`; source parsing failures are `Startup`.

Permission errors follow nested causes to the denied operation's local source frame, including installed package files. A failure wrapped by the asset pipeline therefore identifies the asset code that attempted the read rather than losing the denial in a generic compilation error.

Each instance owns its permissions, environment, and execution thread. Concurrent instances do not share each other's permissions. The Rust host drives execution directly; no Node, Bun, Deno, or pnpm process executes the application.

Signals are virtual inside embedded instances: registering a guest `process.on("SIGTERM")`, `process.on("SIGINT")`, or other supported signal listener never installs an OS signal handler. The embedding host explicitly delivers interrupt or termination through `RuntimeHandle`; the CLI owns process-global OS signal forwarding. Shutdown waits for cooperative guest cleanup up to a five-second default deadline, then interrupts JavaScript and disposes the instance. A deadline interruption is never reported as graceful. Listeners and active connections are released before completed shutdown returns.

See [Embedding Bento](embedding-bento.md) for Rust configuration, asynchronous private host modules, instance isolation, execution outcomes, and shutdown control. Authorized native addons and arbitrary blocking native code are outside the JavaScript containment and interruption guarantees.
