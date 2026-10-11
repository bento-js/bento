---
id: guide.embedding-bento
title: Embedding Bento
describes: [proposal.0001]
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

The runtime loads the project compiler settings once before execution and transforms TypeScript, JSX, and TSX in memory. Configuration diagnostics print once per instance at startup to the host process's stderr. Unsupported JSX execution settings such as `jsx: preserve` and `react-native` allow non-JSX modules to run; only modules containing JSX fail at transform time as `ErrorKind::Transform`, naming the setting and the JSX location. An unreadable or malformed `tsconfig.json` returns from `start` as `ErrorKind::Transform` before execution. Explicit `.ts` imports, top-level `await`, TypeScript enums, and the project's selected JSX import source do not require a build step or JavaScript output files.

## Grants and environment

Read, write, network, environment, and native-addon authority are independent. Ungranted operations fail without interactive prompts. Guest subprocesses and remote module imports remain unavailable. Granting access to a host module does not grant filesystem or database authority: the Rust handler remains responsible for its application's service policy, such as rejecting writes to a read-only store.

For the guest book's Remix asset discovery, compose the canonical project root and `package_lookup_paths(&root)?` into `Scope::Only` for reads. The helper returns canonical `<ancestor>/node_modules` paths from the project root through `/`, including missing directories, as `std::io::Result<Vec<PathBuf>>`. Keep any host-owned database outside these roots when guest filesystem access must remain denied. Adding these lookup roots is an explicit host policy decision; runtime construction adds no permissions.

Inspect every lookup path before granting it. An ancestor's `node_modules` symlink resolves to its target, and granting that path permits reads throughout the target tree, not just the packages the resolver requests. Ancestors writable by other users, such as a shared temporary directory, can therefore introduce unexpectedly broad roots. Use a project location with trusted ancestors or reject lookup roots that exceed your host's policy.

Only granted names from `RuntimeConfig.env` are visible through `process.env` and `Deno.env`. Enumeration omits ungranted names. Guest writes and deletions affect the instance map, not `std::env` or another instance; writing an ungranted name fails. Loading project environment files is a host preparation step, not implicit permission to expose their values.

Web Workers and `node:worker_threads` inherit the instance's environment and host-module registrations, not the host process environment. Their environment operations, signal listeners, and working-directory operations obey the same containment as the main isolate. Guest `chdir` cannot change the host's working directory.

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

Use `error.location()` when presenting source diagnostics. It returns `Option<&SourceLocation>` with a local `PathBuf` and one-based `line` and `column`. Compiler failures refer to the original source diagnostic; exception and rejection frames are source-mapped back to TypeScript; unresolved imports refer to their importing file and import position when available. `message` is plain text without ANSI escape sequences, regardless of terminal color settings, so the embedding host controls presentation. Unterminated terminal sequences in guest messages do not hide subsequent stack frames or causes, and ordinary tabs and line breaks are preserved. Locations never select an environment file. A new `Error` copying a caught module failure's message remains `Uncaught`, including when the text is identical. Guest `SyntaxError` exceptions, such as a failed `JSON.parse`, are also `Uncaught`; source parsing failures are `Startup`.

Permission diagnostics can identify installed package source and follow a wrapped error's cause to the denied operation. This preserves the native permission error and its asset-code location when a framework adds a compilation-error wrapper. Denied addon loading identifies the package or guest call site and names the host's `addons` grant (`--allow-addons` in the CLI).

Clone the `RuntimeHandle` to control the instance from another thread. `loaded_modules()` returns the local source paths attempted so far, including dynamic imports discovered after startup, workspace siblings outside the project root, and files whose parsing or transformation failed. Built-ins, host modules, and environment files are excluded. Dependency exclusion checks `node_modules` components relative to the canonical project root for files inside it, and anywhere in the absolute path for files outside it. The project's own files remain included when its root has a `node_modules` ancestor. Watch hosts can poll this snapshot to expand their watch input and observe repairs to failed source files.

`runtime.is_finished()` reports whether the execution thread has ended without requesting shutdown. A host waiting for a listener can check this state and use `handle.shutdown()` to retrieve the retained completion or failure report immediately when the instance has ended, instead of waiting for a connection timeout.

`handle.signal(Signal::Terminate)` delivers virtual `SIGTERM`; `Signal::Interrupt` delivers virtual `SIGINT`. Guest `process.on` listeners receive these signals. If the signal has no guest listener, its default action requests shutdown. Embedded instances never register process-global OS signal handlers; the CLI owns OS signal handling and forwards the relevant signals to its guest.

`handle.shutdown()` requests termination and returns a `ShutdownReport` within the configured deadline plus a small disposal margin. If a `SIGTERM` listener exists, the guest can stop accepting requests and drain work, for example with `server.close(() => process.exit(0))` and `server.closeAllConnections()`. Without a listener, Bento closes runtime resources and cancels pending guest work. `ShutdownOutcome::Graceful { exit_code }` means disposal completed cooperatively. `ShutdownOutcome::Failed(error)` preserves an uncaught failure during shutdown or a failure that occurred before the shutdown request; `wait()` also returns that failure as `RunOutcome::Failed`. At the deadline Bento interrupts synchronous JavaScript and disposes the instance, reporting `Interrupted`, never graceful success. `TimedOut` means disposal did not finish within the bounded interruption/disposal window, including when native code has not yielded; it is not proof that disposal will never finish.

The graceful report's `exit_code` is `Option<i32>`: `Some(n)` preserves the guest's exit code, including `Some(0)` when a signal handler closes its server and lets the event loop drain, an assigned `process.exitCode`, or an explicit `process.exit(0)`. `None` means Bento's default shutdown ended execution without a guest exit code. A host must not interpret `None` as a successful guest exit. The CLI preserves guest codes and uses 130 for interrupt or 143 for termination when no guest exit code exists.

Once shutdown completes gracefully or by interruption, listeners and connections are closed and another instance can bind the same port. Dropping a running instance also requests bounded shutdown. Host-owned services and persistent data are not discarded when the isolate is disposed. A runtime interrupted by the deadline is never restarted in the same isolate.

`wait()` always joins the runtime thread. After a `TimedOut` report it can therefore block until native code returns and disposal finishes; dropping the runtime instead detaches that thread rather than blocking the host indefinitely. Do not reuse a timed-out or interrupted instance.

## Limits and native addons

These capabilities are application policy, not an OS sandbox or comprehensive resource quota system. Authorized arbitrary native addons run inside the host process and are outside JavaScript permission containment. Arbitrary blocking native code is outside the isolate's interruption guarantee. Enable only trusted addons, and keep blocking service operations off the runtime thread. The supported fixture's actual native compilers must still satisfy shutdown and recreation checks.

Executables loading Node-API addons must export their ABI symbols. See [Node Compatibility](node-compatibility.md) for the linker setup and the exercised compatibility surface.
