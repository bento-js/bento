---
id: guide.embedding-bento
title: Embedding Bento
describes: [proposal.0001, proposal.0002]
---

# Embedding Bento

## Construct an instance

Bento executes installed JavaScript and TypeScript applications inside the Rust host process. Each instance owns a V8 isolate, execution thread, grants, environment, host-module registrations, and runtime resources. The shared V8 platform is initialized once; application execution never spawns Node, Deno, Bun, or a Bento CLI subprocess.

Construct `RuntimeConfig` before calling `start`. `project_root` identifies the application's package and compiler configuration; `cwd` supplies guest `process.cwd()`. `argv` supplies arguments after the entry path. Prepare dependencies before launching; execution never downloads missing packages.

```rust
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::time::Duration;

use bento_host::{Grants, Scope};
use bento_runtime::{HostError, HostModule, RunOutcome, RuntimeConfig, start};
use serde_json::json;

let root = std::fs::canonicalize("./application")?;
let module = HostModule::new("host:demo")
    .function("greet", |arguments| async move {
        let name = arguments[0].as_str().ok_or_else(|| HostError {
            name: "TypeError".into(),
            message: "greet requires a string".into(),
        })?;
        Ok(json!({ "message": format!("Hello, {name}") }))
    });
let config = RuntimeConfig {
    project_root: root.clone(),
    cwd: root.clone(),
    grants: Grants {
        read: Scope::Only(BTreeSet::from([root.clone()])),
        env: Scope::Only(BTreeSet::from(["MODE".into()])),
        ..Grants::default()
    },
    env: BTreeMap::from([("MODE".into(), "embedded".into())]),
    argv: vec![],
    host_modules: vec![module],
    shutdown_deadline: Duration::from_secs(5),
};
let runtime = start(config, &PathBuf::from("main.ts"))?;
for diagnostic in runtime.diagnostics() {
    eprintln!("{diagnostic}");
}
let handle = runtime.handle();
let outcome = runtime.wait();
match outcome {
    RunOutcome::Completed => {}
    RunOutcome::Exited(code) => println!("Guest exited: {code}"),
    RunOutcome::Failed(error) => eprintln!("{:?}: {}", error.kind, error.message),
    RunOutcome::Shutdown(report) => println!("Shutdown: {:?}", report.outcome),
}
```

`RuntimeConfig::new(root)` initializes `cwd` to the project root, denies grants, leaves environment, arguments, and registrations empty, and sets the shutdown deadline to five seconds. Configure authority explicitly before execution.

The runtime loads the project compiler settings once before execution and transforms TypeScript, JSX, and TSX in memory. Configuration diagnostics are found once at startup and returned by `RunningRuntime::diagnostics()`; the runtime itself prints nothing, so the host decides how to show them, and the `bento` CLI prints each one to its stderr. Unsupported JSX execution settings such as `jsx: preserve` and `react-native` allow non-JSX modules to run; only modules containing JSX fail at transform time as `ErrorKind::Transform`, naming the setting and the JSX location. An unreadable or malformed `tsconfig.json` returns from `start` as `ErrorKind::Transform` before execution. Explicit `.ts` imports, top-level `await`, TypeScript enums, and the project's selected JSX import source do not require a build step or JavaScript output files.

## Grants and environment

Read, write, network, environment, native-addon, and language-model authority are independent. Ungranted operations fail without interactive prompts. Guest subprocesses and remote module imports remain unavailable. Granting access to a host module does not grant filesystem or database authority: the Rust handler remains responsible for its application's service policy, such as rejecting writes to a read-only store.

Build path items with `bento_host::canonical_path`, which resolves an absolute path as the operating system would: symlinks are followed, a `..` after a link leaves the link's target, a dangling link names the location it points to, and a part that does not exist yet is kept as written beneath its nearest existing directory. Parse network items from text with `"[::1]:8080".parse::<NetTarget>()`, which accepts a hostname, an IPv4 address, or a bracketed IPv6 address with an optional port and rejects anything else. Network items name endpoints: IP addresses are kept in canonical form and hostnames in lowercase, so `[0:0:0:0:0:0:0:1]:8080` and `[::1]:8080` are one target, and hostnames are never looked up.

Grants remain fixed for the instance's lifetime. `Deno.permissions.query()` and `querySync()` preserve upstream states: granted access reports `granted`, and ungranted access can report `prompt`, but Bento never prompts or allows escalation. `request()`, `requestSync()`, `revoke()`, and `revokeSync()` fail with `NotSupported`, explaining that grants are fixed by the host. Construct a replacement instance to change authority.

For the guest book's Remix asset discovery, compose the canonical project root and `package_lookup_paths(&root)?` into `Scope::Only` for reads. The helper returns canonical `<ancestor>/node_modules` paths from the project root through `/`, including missing directories, as `std::io::Result<Vec<PathBuf>>`. Keep any host-owned database outside these roots when guest filesystem access must remain denied. Adding these lookup roots is an explicit host policy decision; runtime construction adds no permissions.

Inspect every lookup path before granting it. An ancestor's `node_modules` symlink resolves to its target, and granting that path permits reads throughout the target tree, not just the packages the resolver requests. Ancestors writable by other users, such as a shared temporary directory, can therefore introduce unexpectedly broad roots. Use a project location with trusted ancestors or reject lookup roots that exceed your host's policy.

Only granted names from `RuntimeConfig.env` are visible through `process.env` and `Deno.env`. Enumeration omits ungranted names. Guest writes and deletions affect the instance map, not `std::env` or another instance; writing an ungranted name fails. Loading project environment files is a host preparation step, not implicit permission to expose their values.

Web Workers and `node:worker_threads` inherit the instance's environment and host-module registrations, not the host process environment. Their environment operations, signal listeners, and working-directory operations obey the same containment as the main isolate. Guest `chdir` cannot change the host's working directory.

Unsupported upstream features, including `Worker`'s `deno.permissions` option and listeners with `reusePort`, throw `NotSupported` before starting a worker or binding a listener. Guests can catch these errors; uncaught errors fail only the instance as `ErrorKind::Uncaught` with the guest source location. `process.abort()`, `process.kill()`, and `Deno.kill()` cannot terminate or signal the host process; they fail explicitly. Use the host's runtime handle for signal delivery.

### Enable the local language model

Enable `bento_runtime`'s opt-in `prompt-foundation-models` Cargo feature and set `RuntimeConfig.grants.language_model = true` to expose native model access to guests. The native backend requires Apple silicon, macOS 27.0 or newer, SDK 27 or newer for builds, and a ready Apple Intelligence model. Build the embedding executable with `MACOSX_DEPLOYMENT_TARGET=27.0`: only the final link declares a deployment target, and at Rust's default macOS floor the linker selects Swift's back-deployment concurrency library, which the executable cannot resolve at run time. Library embeddings enable neither the feature nor the grant by default.

`LanguageModel` remains exposed without the feature. Without the grant, its availability is `unavailable` and creation rejects with `NotAllowedError` before the backend is inspected. Workers inherit the grant but own separate session resources. Runtime disposal cancels inference and waits for native callbacks to settle within the existing bounded shutdown contract; `TimedOut` still means disposal is incomplete.

For Rust-only model use, depend on `bento_prompt` with its `foundation-models` feature; no V8 runtime is required. [Using the Prompt API](prompt-api.md) describes the web surface, schema subset, cancellation rules, and ambient declarations.

## Expose asynchronous Rust services

Host modules are private instance registrations. Each function receives a JSON array containing the guest's positional arguments and returns a future producing a JSON value or `HostError`. Closures can capture `Arc`-owned Rust services that survive runtime disposal. Guest timers and other asynchronous work continue while a handler future is pending; handlers must not perform blocking native work on the runtime thread.

Disposal drops pending handler futures, cancelling them; captured host-owned services remain alive when the host retains them. Do not call the blocking `shutdown()` method from a handler executing on that same runtime thread: it prevents the runtime from making shutdown progress and can return `TimedOut`. Request shutdown from a separate host thread instead.

```javascript
import { greet } from "host:demo";
const answer = await greet("Bento");
console.log(answer.message);
```

Return `HostError { name, message }` to reject the guest promise with a JavaScript `Error` carrying that name and message. Unknown `host:` imports fail at resolution with a message saying the host did not register the module; they never fall back to npm or the network or suggest installing packages. An instance without the registration cannot import the module, and two instances registering the same specifier retain separate handlers.

## Observe execution and control its lifetime

`start` returns a `RunningRuntime`; `wait` blocks until execution finishes and the instance is disposed. Startup, resolution, transformation, permission, and uncaught guest errors remain distinguishable. Guest `process.exit(code)` yields `RunOutcome::Exited(code)` rather than exiting the Rust process, including when called inside an HTTP request handler.

Use `error.location()` when presenting source diagnostics. It returns `Option<&SourceLocation>` with a local `PathBuf` and one-based `line` and `column`. Compiler failures refer to the original source diagnostic; exception and rejection frames are source-mapped back to TypeScript and select the first non-dependency source frame, including sources outside the project root; unresolved imports refer to their importing file and import position when available. Dependency classification is the same as for compiler settings: check `node_modules` components relative to the project root for sources inside it, and anywhere in the absolute path for sources outside it. `message` is plain text without ANSI escape sequences, regardless of terminal color settings, so the embedding host controls presentation. Unterminated terminal sequences in guest messages do not hide subsequent stack frames or causes, and ordinary tabs and line breaks are preserved. Locations never select an environment file. A new exception copying a caught module failure's message remains `Uncaught` at its own throw site when its class differs. A new exception that copies both the class and the exact message, such as `new TypeError(caught.message)` after a failed import, is reported as the caught failure, with its kind and location. Rethrowing a caught resolution failure preserves its `Resolution` kind and location. A dynamic import of a source that fails to transform rejects with a `TypeError`, as in Deno, so rethrowing it reports `Uncaught`. Guest `SyntaxError` exceptions, such as a failed `JSON.parse`, are also `Uncaught`; source parsing failures are `Startup`.

Permission diagnostics can identify installed package source and follow a wrapped error's cause to the denied operation. This preserves the native permission error and its asset-code location when a framework adds a compilation-error wrapper. Denied addon loading identifies the package or guest call site and names the host's `addons` grant (`--allow-addons` in the CLI).

Clone the `RuntimeHandle` to control the instance from another thread. `watch_inputs()` returns a sorted, deduplicated snapshot of the local graph's watch inputs attempted so far: loaded non-dependency sources, failed file-resolution targets from non-dependency importers, and those sources' package-scope configuration. It includes ESM, CommonJS, dynamic imports discovered after startup, sources loaded through `createRequire`, workspace siblings outside the project root, and files whose parsing or transformation failed. Failed `require.resolve` and `import.meta.resolve` calls retain their missing targets; successful calls alone add no watch inputs, even when loading the resolved module later throws. Failed calls using synthetic CommonJS parents, including `createRequire` anchored to a loaded application file, add no missing targets; repairs outside existing watches need a manual restart. File URL imports from non-dependency sources retain their local paths. A dependency's dynamic import adds no failed target merely because the application supplied the URL. If the application itself previously resolved that URL, including with `import.meta.resolve`, a later failed load retains the target. Package-map targets and CommonJS extension candidates are retained when resolution fails. Package scopes include the nearest existing `package.json`, missing nearer scopes inside the project, and a missing scope in an external source's own directory. Creating a scope above that external directory needs a manual restart. Package-scope searches for external sources stop below the first ancestor of the project root. Built-ins, host modules, and environment files are excluded. Dependency exclusion checks `node_modules` components relative to the canonical project root for files inside it, and anywhere in the absolute path for files outside it. The project's own files remain included when its root has a `node_modules` ancestor. A missing target in a workspace package reached through `node_modules` needs a manual restart; once its source loads, its canonical path outside `node_modules` is included. Watch hosts can poll this snapshot to expand their watch inputs and watch a failed target's nearest existing ancestor recursively when its directory does not yet exist, including above the project root for an explicitly named failed path. An external package scope whose parent exists needs only a nonrecursive directory watch to observe edits, replacement, deletion, and recreation without subscribing to its dependency or build-output trees. The project's `tsconfig.json` and its `extends` chain are separate watch inputs that the host must discover.

Local Web Worker and `node:worker_threads` scripts share the instance's watch inputs. A missing worker script is retained even outside the project or in a build-output directory such as `dist`, whoever starts the worker: the engine does not report which module started it, so a dependency that starts a worker on a missing path outside `node_modules` also adds that path. Scripts under `node_modules` remain excluded.

CommonJS failures are attributed by cached parent-module identity. Dependency modules and synthetic parents are excluded, but dependency code borrowing a cached application module through `require.main.require`, `module.parent.require`, an application-supplied `require`, or `Module._load(request, appModule)` has application attribution. Its failed targets can reach any ancestor, including `/`. A host recursively watching every missing target's nearest existing ancestor will consequently watch `/` if such a call names a missing file there.

Failed `require.resolve` calls with `options.paths` record no watch inputs. Repairing a target outside the host's existing watches through those search paths needs a manual restart.

`runtime.is_finished()` reports whether the execution thread has ended without requesting shutdown. A host waiting for a listener can check this state and use `handle.shutdown()` to retrieve the retained completion or failure report immediately when the instance has ended, instead of waiting for a connection timeout.

`handle.signal(Signal::Terminate)` delivers virtual `SIGTERM`; `Signal::Interrupt` delivers virtual `SIGINT`. Guest `process.on` listeners receive these signals. If the signal has no guest listener, its default action requests shutdown. Embedded instances never register process-global OS signal handlers; the CLI owns OS signal handling and forwards the relevant signals to its guest.

`handle.shutdown()` requests termination and returns a `ShutdownReport` within the configured deadline plus a small disposal margin. If a `SIGTERM` listener exists, the guest can stop accepting requests and drain work, for example with `server.close(() => process.exit(0))` and `server.closeAllConnections()`. Without a listener, Bento closes runtime resources and cancels pending guest work. `ShutdownOutcome::Graceful { exit_code }` means disposal completed cooperatively. `ShutdownOutcome::Failed(error)` preserves an uncaught failure during shutdown or a failure that occurred before the shutdown request; `wait()` also returns that failure as `RunOutcome::Failed`, unless disposal then timed out as well, as described below. At the deadline Bento interrupts synchronous JavaScript and disposes the instance, reporting `Interrupted`, never graceful success. `TimedOut` means disposal did not finish within the bounded interruption/disposal window, including when native code has not yielded; it is not proof that disposal will never finish.

The graceful report's `exit_code` is `Option<i32>`: `Some(n)` preserves the guest's exit code, including `Some(0)` when a signal handler closes its server and lets the event loop drain, an assigned `process.exitCode`, or an explicit `process.exit(0)`. `None` means Bento's default shutdown ended execution without a guest exit code. A host must not interpret `None` as a successful guest exit. The CLI preserves guest codes and uses 130 for interrupt or 143 for termination when no guest exit code exists. A timed-out disposal keeps no guest code at all; the CLI reports the incomplete disposal and exits with 1, including for a run that ended on its own.

`wait()` reports incomplete disposal for a run that ended on its own as well. A guest that completes, calls `process.exit`, or fails still owns whatever work the instance holds, such as live Prompt API inference. When that work does not settle within the shutdown deadline, `wait()` returns `RunOutcome::Shutdown` carrying the same `ShutdownReport` with `ShutdownOutcome::TimedOut` that a later `shutdown()` call returns, instead of `Completed`, `Exited`, or `Failed`. The guest's exit code is not preserved then, exactly as for a timed-out requested shutdown.

Once shutdown completes gracefully or by interruption, listeners and connections are closed and another instance can bind the same port. Dropping a running instance also requests bounded shutdown. Host-owned services and persistent data are not discarded when the isolate is disposed. A runtime interrupted by the deadline is never restarted in the same isolate.

`wait()` always joins the runtime thread. After a `TimedOut` report it can therefore block until native code returns and disposal finishes; dropping the runtime instead detaches that thread rather than blocking the host indefinitely. Do not reuse a timed-out or interrupted instance.

## Limits and native addons

These capabilities are application policy, not an OS sandbox or comprehensive resource quota system. JavaScript allocation can exhaust V8's heap or native memory and abort the embedding process; the main isolate and workers without explicit `resourceLimits` have no configured heap quota. Worker heap limits can interrupt that worker but are not a process-memory limit. Embedders must not treat an in-process instance as protection against memory exhaustion or run untrusted guests where host-process survival is required. Authorized arbitrary native addons also run inside the host process and are outside JavaScript permission containment. An addon can abort the host, including through Node-API's fatal-error callback or a failed legacy libuv-symbol lookup. Arbitrary blocking native code is outside the isolate's interruption guarantee. Enable only trusted addons, and keep blocking service operations off the runtime thread. The supported fixture's actual native compilers must still satisfy shutdown and recreation checks.

Executables loading Node-API addons must export their ABI symbols. See [Node Compatibility](node-compatibility.md) for the linker setup and the exercised compatibility surface.
