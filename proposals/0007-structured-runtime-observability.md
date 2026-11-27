---
id: proposal.0007
title: Structured Runtime Observability
authors: [Mark]
status: draft
pull-request: https://github.com/bento-js/bento/pull/277
issues: []
supersedes: []
---

# Structured Runtime Observability

## Summary

Give every Bento run one machine-readable model: structured runtime errors with stable codes, per-instance stdout and stderr capture with bounded JSON run receipts, and lifecycle events (start, diagnostics, listening, end, watch restarts) for embedders and as NDJSON, so agents stop scraping prose and polling ports.

## Authorization

On 2026-11-27, in the Sesh session "Bento for Agents", Mark authorized a Worktrunk worktree, branch, draft pull request, and draft proposal for this change, and asked that structured errors with JSON diagnostics, per-instance stdio capture with JSON run receipts, and runtime lifecycle events with NDJSON watch streaming be designed together as one proposal, so that errors, receipts, and events share one data model. The proposal stays `draft` until Mark settles it.

## Motivation

Coding agents and embedding hosts run Bento scripts to verify their work. Today every fact about a run reaches them as prose written for a person at a terminal, and the Rust library cannot keep one instance's output apart from the process's.

- **Failures are prose.** `bento run` reports a failure as an `ariadne` report on stderr with box-drawing characters, a source excerpt, and a stack trace. Exit status `1` covers usage errors, unknown scripts, malformed environment files, syntax errors, unresolved imports, permission denials, uncaught exceptions, and incomplete disposal alike ([Exit codes](../guides/running-with-bento.md#exit-codes)). An agent that needs the error's name, its message without the stack, the frame in its own code, or the operation that was denied must parse that text. Bento parses it too: `crates/bento_cli/src/runtime_report.rs` recovers the summary and the trace from `RuntimeError.message` by looking for lines that start with `at ` and lines of carets, because `RuntimeError` carries only `kind`, a formatted `message`, and one `location`. The name, the bare message, the frames, the `cause` chain, and Node's `code` are lost when the runtime builds the error.
- **Output cannot be captured per instance.** `crates/bento_runtime/src/lib.rs` documents that "the guest's console output goes to the process's stdout and stderr as it would in Node", and `workers.rs` passes `stdio: Default::default()` to every worker. Two instances in one host write to the same file descriptors, so a host cannot attribute output to the script that wrote it, bound it, or return it with the result. An agent harness that needs a script's output must spawn the `bento` CLI as a subprocess, which contradicts [the vision](../VISION.md)'s "Embedding is the default". Through the CLI, an agent gets unbounded output interleaved with Bento's own messages on stderr, and nothing tells it whether what it read is complete.
- **Readiness is polled.** Nothing reports that a server is listening. Agents start `bento run server.ts &` and loop on `curl` with `sleep`. Bento's own tests do the same: `crates/bento_runtime/tests/core.rs` polls for a sentinel file the script writes in its `listen` callback, `tests/fixture.rs` retries `GET /` every 10 ms for up to 20 seconds, and the CLI tests choose a free port before starting the script and hope it stays free. [#257](https://github.com/bento-js/bento/issues/257) is an intermittent "server did not listen" failure in exactly such a wait.
- **Watch restarts are prose.** `bento run --watch` announces `watch: restarting after a change to server.ts`, `watch: server.ts finished`, and `watch: waiting for changes before restarting` on stderr (`crates/bento_cli/src/watch/restarts.rs`). An agent that edits a file and wants to know whether the server restarted, failed to reload, or is serving the new code must scrape those lines and then poll the port again.

The workaround for each is screen-scraping and sleeping. It is slow, it races, and it breaks whenever a message's wording changes.

## Domain grounding

### Established context

- Node identifies errors by `code`, not by message: "`error.code` is the most stable way to identify an error. It will only change between major versions of Node.js. In contrast, `error.message` strings may change between any versions" ([Node 26 errors](https://nodejs.org/api/errors.html#errorcode)). Errors chain through `error.cause` (ES2022), and an `AggregateError` holds its `errors` (ES2021).
- V8's stack trace API exposes structured call sites with a function name, script name, and one-based line and column ([V8 stack trace API](https://v8.dev/docs/stack-trace-api)). Generated positions map back to original sources through source maps ([ECMA-426](https://tc39.es/ecma426/)).
- `deno_core`'s `JsError`, which Bento receives for an uncaught exception, carries `name`, `message`, source-mapped `frames`, `cause`, `aggregated` errors, and a `stack_is_custom` flag that says the frames are not authoritative because library code replaced `stack`. It converts at most 32 levels and 256 errors. It does not carry `code` or other own properties, except keys an op error registered under the `errorAdditionalPropertyKeys` symbol (observed in the pinned `bento-js/deno_core` fork, `error.rs` and `00_infra.js`).
- In the pinned Deno crates, `console.*` writes through `core.print` to `op_print`, which writes to the process's `std::io::stdout()` or `stderr()` directly. `process.stdout` and `Deno.stdout` write to the instance's stdio resources, which `deno_io::Stdio` configures per worker; when the resource is a terminal, `process.stdout` writes through a TTY handle on the raw file descriptor instead. `deno_io::ChildProcessStdio` exists so that "when the runtime redirects stdout/stderr (e.g. during `deno test` for output capture), child processes spawned with `stdio: \"inherit\"` need to inherit the redirected handles" (`deno_io` 0.169.0, `deno_node` 0.197.0, `deno_runtime` 0.267.0). `deno_core` extensions can replace an op by name, which Bento already does for `op_net_listen_tcp` and `op_net_listen_tls` in `crates/bento_runtime/src/feature_ops.rs`.
- A pipe delivers end-of-file only when every write end is closed, so a reader that waits for end-of-file hangs while any descendant still holds one. Node separates a child's `'exit'` from `'close'` for this reason: "multiple processes might share the same stdio streams" ([Node 26 `child_process`](https://nodejs.org/api/child_process.html#event-close)). POSIX makes writes of at most `PIPE_BUF` bytes atomic; larger concurrent writes may interleave.
- Once `listen(2)` returns, the kernel completes connections into the backlog before the program calls `accept`, so a client can connect as soon as the socket is listening. Binding port 0 assigns an ephemeral port that `getsockname(2)` reports.
- Machine-readable event streams in established tools put structured events on stdout and wrap the program's own output in them. `go test -json` writes "a newline-separated sequence of TestEvent objects"; output becomes `output` events, "unmodified except that invalid UTF-8 output from a test is coerced into valid UTF-8 by use of replacement characters", and "the concatenation of the Output fields of all output events is the exact output of the test execution" (`go doc cmd/test2json`). NDJSON and JSON Lines specify one UTF-8 JSON value per `\n`-terminated line ([JSON Lines](https://jsonlines.org/)).
- Mark's agent-first CLI guide (`~/Documents/Documentation/Programming/agentic-dev/agent-first-clis.md`) asks for stdout to carry the requested result and nothing else, errors as part of the public contract with `code`, `message`, `retryable`, and `details`, receipts for operations, NDJSON for event streams, and never truncating silently. Mark's `mcp` CLI implements the failure document as `{"error": {"code", "message", "retryable", "details", "suggestions"}}` on stderr with stable `SCREAMING_SNAKE_CASE` codes (`crates/mcp/src/failure.rs`).

### Relevant constraints and principles

- [The vision](../VISION.md): "Embedding is the default. … A feature available only through a CLI is a limitation to correct." Capture and events belong in `bento_runtime`; the CLI is one consumer. [The architecture](../ARCHITECTURE.md) asks Bento to "expose structured errors, exceptions, … source maps, … and runtime resource statistics to the host."
- [Proposal 0001](0001-embeddable-typescript-runtime.md): the runtime prints nothing of its own; the host decides how to report. A plain `bento run` behaves like `node script.js`: the script owns stdout and stderr, and its exit code is Bento's.
- Proposal 0003 ([#259](https://github.com/bento-js/bento/pull/259), awaiting implementation) makes denials Node's errors: `ERR_ACCESS_DENIED` with `permission` and `resource` properties, `ERR_DLOPEN_DISABLED` for addons, and the Prompt API's `NotAllowedError`. A denied `fetch` rejects with an error whose `cause` is the denial. Usage errors exit with status 1.
- Proposal 0004 ([#268](https://github.com/bento-js/bento/pull/268), draft) removes the task runner, so `bento run` runs only scripts. This proposal defines nothing for tasks.
- Proposal 0005 ([#269](https://github.com/bento-js/bento/pull/269), draft) owns Bento's command-line wire contract. On 2026-11-27 Mark decided that its sections "Output formats", "Structured invocations", "Pass-through invocations", "Deciding the contract", "Failure documents", and "Error codes" are the single definition of the output formats and their framing, the rule between structured and pass-through invocations, the failure document and its body, `--error-format`, and the one code table, and that this proposal adopts them. 0005 names this proposal's results and codes and leaves their definitions here. It also adds `bento_compiler::TransformError::kind`, which tells a syntax error from a transform failure.
- Proposal 0011 ([#276](https://github.com/bento-js/bento/pull/276), draft) adds `wait_async()` and makes `RunOutcome` `Clone`, `PartialEq`, and `Eq`. Capture and events must not depend on whether a host waits synchronously or asynchronously.
- Per-instance redirection cannot reach native code that writes to file descriptors 1 and 2 directly, because file descriptors belong to the process. Only process-wide redirection (`dup2`) can, and it cannot tell instances apart.
- A sink called synchronously from the isolate's thread delays the guest. A capture that waits for end-of-file can hang on an orphaned child.

### Quality bar

- An agent can branch on a stable code and on typed fields, never on message text, and the same failure has the same code whether it reaches the agent as a receipt, an event, or a failure document.
- Truncation is never silent: every captured stream says how many bytes the stream carried in total.
- A structured run's stdout is always parseable: nothing Bento or the guest writes can corrupt it.
- An event reports a fact the runtime observed, not something guest code claimed; the listening event's port is the bound port.
- Plain `bento run` is byte-for-byte unchanged, and the default library configuration costs nothing: no pipes, threads, or event values.

### Remaining uncertainty

- Where Node's `net.Server.listen` binds in the pinned fork: `deno_node`'s `TCPWrap.listen` is native code in `deno_core`'s `uv_compat`, a different path from `op_net_listen_tcp`. Emitting `listening` for both may need a patch to the `bento-js/deno_core` fork, which already carries patches (`PATCHES.md`). This is an implementation cost, not a design question.
- Reading `code`, `permission`, and `resource` from thrown values needs either a fork patch to `JsError` conversion or a Bento-side read when the uncaught exception is observed. Which is simpler is not established.
- Whether V8 module parse errors (`CoreErrorKind::Parse`) carry a usable position in every case is not established; `Syntax` failures carry a location when one is reported.

## Existing baseline

**`RuntimeError`** (`crates/bento_runtime/src/error.rs`) has `kind: ErrorKind`, a public `message` that is "ready to show" (entry-path prefix, `deno_runtime::fmt_errors::format_js_error` output with source line, caret, frames, and `Caused by:`), and a private `location` read through `location()`. `ErrorKind` is `Startup`, `Resolution`, `Transform`, `Permission`, or `Uncaught`. `Sources` records loader failures (resolution and transform) with their kind and location because JavaScript rethrows them without their Rust type, and `RuntimeError::guest` matches them back by class and message. Permission classification follows `cause` and aggregated errors to the denial and places it in dependency code when needed. Preserve: the stage classification, the location rules (first non-dependency, source-mapped frame; the import statement for resolution; the syntax position for transforms), escape-free messages, and the ready-to-show `message`.

Three things in it are compromised. `Startup` conflates an instance that could not be set up, JavaScript that V8 could not parse, and a panicking runtime thread, so a code derived from it could not tell "fix your syntax" from "report a Bento defect". `Transform` conflates TypeScript syntax errors, which Oxc reports while parsing, with unexecutable JSX settings and an unreadable `tsconfig.json`, so the same syntax error is `Transform` in a `.ts` file and `Startup` in a `.js` file. And everything a consumer needs beyond the kind lives inside one formatted string.

**The CLI's reports** (`crates/bento_cli/src/error.rs`, `diagnostic.rs`, `runtime_report.rs`) render an `ariadne` report when a failure has a readable location and one `error:` line otherwise, colored only when stderr is a terminal, never quoting environment files. `DividedMessage` splits the runtime's message heuristically. Preserve the text reports exactly; replace nothing about them except where a reclassified kind changes their title.

**Output** goes to the process's streams: the main worker is created with `deno_io::Stdio::default()` through `create_custom_worker`, workers with `stdio: Default::default()`, and `console` writes through `op_print` regardless. `RunningRuntime::diagnostics()` returns `tsconfig.json` diagnostics, and the CLI prints them with `eprintln!` in `instance.rs`. Preserve: `Inherit` as the default, and the runtime printing nothing itself.

**Lifecycle**: `RunOutcome` (`Completed`, `Exited`, `Failed`, `Shutdown(ShutdownReport)`) is returned by `wait()`. The CLI's own `events::Event` (`Ended`, `Signal`, `Changed`) drives `script.rs` and the watch loop, whose states are `Waiting`, `Running`, `Restarting`, and `Stopping`. `ending.rs` maps an outcome and an optional stop to an exit status. Preserve: the state machine, the one-instance-at-a-time restart, and the exit statuses.

**Listeners**: `feature_ops.rs` replaces `op_net_listen_tcp` and `op_net_listen_tls` with Bento ops that call the originals, which is the pattern for observing a bind from Rust. UDP, `unixpacket`, and `vsock` listeners are denied as unsupported.

**Proposal 0005's wire contract** is what this proposal's CLI output is written under, and Mark's `mcp` CLI is its prior art. Until this proposal is implemented, 0005 rejects `bento run --output json` without `--dry-run`, accepts no `ndjson` on `bento run`, and defines `--error-format` without declaring it on `bento run`, so Bento's run failures stay human text.

## Proposed solution

Build one data model in the libraries and render it three ways. `bento_runtime` gains structured errors (name, bare message, Node `code`, source-mapped frames classified by origin, causes, and typed details), per-instance standard streams that a host can inherit, capture with a bound, or stream to a sink, and a typed event sink that reports start, diagnostics, listeners, workers, and the run's end. Under proposal 0005's wire contract, the CLI renders that model as a receipt, an event stream, or a failure document, all sharing 0005's failure body and one set of run fields.

```text
bento run --output json build.ts               → stdout: one receipt with outcome, exit code, duration, bounded stdout and stderr, diagnostics, and error
bento run --output ndjson --watch server.ts    → stdout: started, listening {port}, output…, changed, restarting, ended, started, listening… one JSON object per line
bento run --error-format json server.ts        → the script's output passes through; a failure is reported as one failure document on stderr
bento run server.ts                            → unchanged
```

An agent starting a dev server no longer polls:

```sh
bento run --output ndjson --watch server.ts > events.ndjson &
# wait for {"event":"listening","run":1,"transport":"tcp","address":"::","port":3000,…}
# edit a file; wait for {"event":"listening","run":2,…} or {"event":"ended","run":2,"outcome":"failed",…}
```

An embedding host captures one instance's output and observes it:

```rust
let config = RuntimeConfig {
    stdio: Stdio {
        stdout: OutputTarget::Capture { max_bytes: 64 * 1024 },
        stderr: OutputTarget::Capture { max_bytes: 64 * 1024 },
        ..Stdio::default()
    },
    events: Some(EventSink::new(move |event| {
        if let RuntimeEvent::Listening(Listener::Tcp { port, .. }) = event {
            ready.send(port).ok();
        }
    })),
    ..RuntimeConfig::new(root)
};
let running = bento_runtime::start(config, Path::new("server.ts"))?;
let output = running.captured_output();
let outcome = running.wait();
let stdout = output.stdout().expect("stdout is captured");
```

## Detailed design

### Vocabulary

- A **run** is one attempt to execute the script in a runtime instance, from the call to `start` to the instance's end; when `start` fails, the run ends at once without an instance. A plain `bento run` has one run; `bento run --watch` has one for every start. The CLI numbers the runs of one invocation from 1 in the order it calls `start`.
- A **failure** is why a run ended badly. In the library it is a `RuntimeError`; the CLI reports it with proposal 0005's failure body.
- An **exception** is the JavaScript value a failure carries when it was thrown and nothing caught it. Not every failure has one, and an exception has its own causes.
- A failure's **kind** is the stage of a run it belongs to. Its **error code** is the code from proposal 0005's one table that the CLI reports for it; each kind maps to exactly one code.
- A **diagnostic** reports something about the project or the tooling without ending the run.
- An **event** is something that happened, delivered as it happens. A run's **outcome** is how it ended; one event, `ended`, carries it whatever it is.
- A **captured stream** is what Bento kept of one standard stream: its first bytes up to a limit, and how many bytes the stream carried in total.
- A **listener** is a socket the guest bound and listens on. Bento reports its transport and address, not the protocol served over it.
- A **receipt** is `bento run`'s `json` result, describing one run; an **event stream** is its `ndjson` result. Proposal 0005's vocabulary defines the other command-line terms used here: output format, structured and pass-through contracts, error format, failure document, failure body, and error code.

### Structured runtime errors

#### Kinds

`ErrorKind` becomes `#[non_exhaustive]` and has these variants. Each maps to one CLI code.

| Kind | A run fails this way when | Code |
| --- | --- | --- |
| `Startup` | The instance could not be set up: the project root, working directory, or entry does not exist; a host module specifier is not a unique `host:` name; a worker could not be initialized. | `STARTUP_FAILED` |
| `Configuration` (new) | The project's `tsconfig.json`, or a file it extends, cannot be read or understood. Was `Transform`. | `INVALID_CONFIGURATION` |
| `Resolution` | An import named a module that could not be resolved. | `RESOLUTION_FAILED` |
| `Syntax` (new) | A module's source cannot be parsed: Oxc's parser or semantic checker rejected TypeScript, JSX, or TSX (was `Transform`), or V8 rejected JavaScript, in an ES module or in a CommonJS module's wrapper (was `Startup`). | `SYNTAX_ERROR` |
| `Transform` | A module parsed but cannot be made executable: it contains JSX under a `jsx` setting that cannot run, Bento does not transform its extension, or Oxc's transformer reported an error. | `TRANSFORM_FAILED` |
| `Permission` | The guest attempted an operation outside its permissions and did not handle the denial. | `PERMISSION_DENIED` |
| `Uncaught` | The guest threw, or a promise rejected, and nothing handled it; or the guest used an unsupported runtime feature. Unchanged. | `UNCAUGHT_EXCEPTION` |
| `Internal` (new) | Bento itself failed: the runtime thread panicked, or a thread or async runtime could not be created. Was `Startup`. | `INTERNAL` |

The runtime tells `Syntax` from `Transform` with `bento_compiler::TransformError::kind`, which proposal 0005 adds: `Syntax` for diagnostics from Oxc's parser and semantic checker, and `Transform` for everything else `transform` rejects. Whichever proposal is implemented first adds it. The panic message `runtime thread panicked: <payload>` is unchanged; only its kind changes.

#### `RuntimeError`

```rust
#[derive(Clone, Debug, Error, PartialEq, Eq, Serialize)]
#[error("{message}")]
pub struct RuntimeError {
    pub kind: ErrorKind,
    pub message: String,               // unchanged: ready to show
    location: Option<SourceLocation>,  // unchanged
    exception: Option<Exception>,      // new
    details: Option<ErrorDetails>,     // new
}

impl RuntimeError {
    pub fn location(&self) -> Option<&SourceLocation>;
    pub fn exception(&self) -> Option<&Exception>;
    pub fn details(&self) -> Option<&ErrorDetails>;
}
```

- `exception` is present exactly when the failure reached the runtime as a thrown JavaScript value: every `Permission` failure, every `Uncaught` failure except an unsupported runtime feature (which Bento detects without a thrown value), and the `Resolution`, `Syntax`, and `Transform` failures the loader threw into JavaScript before they went uncaught. It is absent for `Startup`, `Configuration`, and `Internal` failures and for V8 parse errors that produced no exception object.
- `location` keeps its current rules, and the new kinds follow them: a `Syntax` failure is placed at the syntax error when Oxc or V8 reports a position, and a `Configuration` failure at the configuration file's position when the loader reports one.

#### Exceptions and frames

```rust
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Exception {
    pub name: Option<String>,
    pub message: String,
    pub code: Option<String>,
    pub frames: Vec<StackFrame>,
    pub custom_stack: Option<String>,
    pub cause: Option<Box<Exception>>,
    pub errors: Vec<Exception>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct StackFrame {
    pub function: Option<String>,
    pub script: Option<FrameScript>,
    pub line: Option<u32>,
    pub column: Option<u32>,
    pub origin: FrameOrigin,
}

pub enum FrameScript { File(PathBuf), Url(String) }
pub enum FrameOrigin { Application, Dependency, Runtime }
```

- `name` is the thrown object's `name` when it is a string (`TypeError`, `AggregateError`, `NotAllowedError`), and `None` for a thrown value that is not such an object.
- `message` is the error's `message` without its name, stack, or terminal escapes. For a thrown value that is not an error, it is the value's string form as V8 reports it (`throw 42` gives `"42"`).
- `code` is the thrown object's own `code` property when it is a string, such as `ERR_ACCESS_DENIED`, `ERR_MODULE_NOT_FOUND`, or `ENOENT`. A property whose getter throws is treated as absent.
- `frames` lists the error's call sites innermost first, as V8 recorded them, with positions mapped to the original source by the same source maps `location` uses. When library code replaced the error's `stack`, so that the recorded frames no longer describe it, `frames` is empty and `custom_stack` holds the replaced text without terminal escapes; otherwise `custom_stack` is `None`.
- A frame's `function` is V8's function name, or its method name when it has no function name, and `None` for top-level module code and anonymous functions. `script` is `File` with the local path for a `file:` script, `Url` with the script name for anything else (`node:fs`, `ext:…`, `data:…`), and `None` for native frames. `line` and `column` are one-based and `None` when V8 reports none.
- `origin` is `Dependency` for a file under a `node_modules` component, by the rule `location` uses; `Application` for any other file and for a URL that is not one of the runtime's own (`data:`, `blob:`); and `Runtime` for `node:`, `ext:`, `bento:`, and `host:` scripts and native frames.
- `cause` is the error's `cause`, converted the same way, when it has one. `errors` holds an `AggregateError`'s `errors` in order and is empty otherwise. Bento keeps the tree `deno_core` converts: at most 32 levels and 256 exceptions; anything beyond is omitted.

#### Details

```rust
#[non_exhaustive]
pub enum ErrorDetails {
    Configuration { path: PathBuf },
    Resolution { specifier: String, referrer: String },
    Permission(Denial),
}

pub struct Denial {
    pub name: String,
    pub code: Option<String>,
    pub permission: Option<String>,
    pub resource: Option<String>,
}
```

- Every `Configuration` failure carries `Configuration` details naming the file that could not be read or understood: `tsconfig.json` or the file it extends.
- Every `Resolution` failure the loader recorded carries `Resolution` details: the specifier as written and the referrer's URL.
- A `Permission` failure carries `Permission` details when its exception tree contains an error whose `code` is `ERR_ACCESS_DENIED` or `ERR_DLOPEN_DISABLED`, or whose name is `NotAllowedError`. Bento searches the exception, then its `cause` chain, then its aggregated errors depth-first, and describes the first match: its `name`, its `code`, and its own string `permission` and `resource` properties as proposal 0003 sets them. A denied `fetch` therefore reports `permission: "Net"` although the uncaught error is a `TypeError`.
- Other failures carry no details. The kind always tells which variant to expect.

#### Serialization

`RuntimeError`, `ErrorKind`, `SourceLocation`, `Exception`, `StackFrame`, `FrameOrigin`, `ErrorDetails`, `Denial`, `bento_compiler::Diagnostic`, `Severity`, `RunOutcome`, `ShutdownReport`, `ShutdownOutcome`, `RuntimeEvent`, and `Listener` implement `serde::Serialize`. `RunOutcome` also derives `Clone`, `PartialEq`, and `Eq`. Derive where the shape below allows it and write the impl where it does not; the JSON shape is the contract.

- Keys are camelCase, enumerated values lowercase kebab-case, and every key in a shape is always present, `null` when it has no value. Paths are strings, converted lossily when not UTF-8, so serialization never fails.
- `SourceLocation`: `{"path", "line", "column"}`.
- `StackFrame`: `{"function", "path", "url", "line", "column", "origin"}`, where at most one of `path` and `url` is non-null, and `origin` is `application`, `dependency`, or `runtime`.
- `Exception`: `{"name", "message", "code", "frames", "customStack", "cause", "errors"}`.
- `RuntimeError`: `{"kind", "message", "location", "exception", "details"}`, where `kind` is `startup`, `configuration`, `resolution`, `syntax`, `transform`, `permission`, `uncaught`, or `internal`, and `details` is `{"path"}`, `{"specifier", "referrer"}`, `{"name", "code", "permission", "resource"}`, or `null`.
- `Diagnostic`: `{"severity", "path", "option", "message"}`, with `severity` `warning` or `error`.
- `RunOutcome`: `{"type": "completed"}`, `{"type": "exited", "exitCode"}`, `{"type": "failed", "error"}`, or `{"type": "shutdown", "report"}`.
- `ShutdownReport`: `{"outcome"}`, where the outcome is `{"type": "graceful", "exitCode"}`, `{"type": "failed", "error"}`, `{"type": "interrupted"}`, or `{"type": "timed-out"}`.
- `RuntimeEvent` and `Listener`: as listed under [Events](#events).

A failure for an uncaught `TypeError` caused by a denial serializes as:

```json
{
    "kind": "permission",
    "message": "/app/main.ts: Uncaught (in promise) TypeError: fetch failed\n    at …",
    "location": { "path": "/app/main.ts", "line": 3, "column": 1 },
    "exception": {
        "name": "TypeError",
        "message": "fetch failed",
        "code": null,
        "frames": [
            {
                "function": null,
                "path": "/app/main.ts",
                "url": null,
                "line": 3,
                "column": 1,
                "origin": "application"
            }
        ],
        "customStack": null,
        "cause": {
            "name": "Error",
            "message": "Access to this API has been restricted. Use --allow-net to manage permissions.",
            "code": "ERR_ACCESS_DENIED",
            "frames": [],
            "customStack": null,
            "cause": null,
            "errors": []
        },
        "errors": []
    },
    "details": {
        "name": "Error",
        "code": "ERR_ACCESS_DENIED",
        "permission": "Net",
        "resource": null
    }
}
```

### Per-instance standard streams

#### Configuration

```rust
pub struct RuntimeConfig {
    // …existing fields…
    pub stdio: Stdio,
    pub events: Option<EventSink>,
}

#[derive(Clone, Debug, Default)]
pub struct Stdio {
    pub stdin: InputSource,
    pub stdout: OutputTarget,
    pub stderr: OutputTarget,
}

#[derive(Clone, Debug, Default)]
pub enum InputSource { #[default] Inherit, Bytes(Arc<[u8]>) }

#[derive(Clone, Debug, Default)]
pub enum OutputTarget { #[default] Inherit, Capture { max_bytes: usize }, Sink(OutputSink) }

#[derive(Clone)]
pub struct OutputSink { /* … */ }
impl OutputSink {
    pub fn new(write: impl FnMut(OutputStream, &[u8]) + Send + 'static) -> OutputSink;
}

pub enum OutputStream { Stdout, Stderr }
```

`RuntimeConfig::new` sets `stdio` to `Stdio::default()` and `events` to `None`. `OutputSink` and `EventSink` implement `Debug` without showing the closure. A config is cloned freely, as the watch loop does for every restart: a `Capture` is a policy, and each instance gets its own buffer.

#### Behavior

- **`Inherit`** is today's behavior: the guest writes to the process's stream, `process.stdout.isTTY` reflects it, and Bento creates no pipe or thread for it.
- **`Capture` and `Sink`** give the stream to the instance alone. Everything the instance writes to it goes there and never to the process's stream: `console` methods, `process.stdout` and `process.stderr`, `Deno.stdout` and `Deno.stderr`, the same from every worker of the instance, and child processes the guest starts with that stream inherited. The guest sees a stream that is not a terminal. Order is preserved within one writer; writes larger than `PIPE_BUF` from concurrent writers may interleave, as on a pipe.
- Native code that writes to file descriptors 1 and 2 directly, through an addon or FFI, and a child process given those descriptors explicitly, still reach the process's streams. Per-instance redirection cannot reach them.
- **`Capture { max_bytes }`** keeps the first `max_bytes` bytes of the stream and counts every byte. It never blocks the guest.
- **`Sink`** receives every chunk of the stream in order, on a Bento-owned thread. Chunk boundaries do not correspond to the guest's writes. Calls to one sink, including its clones and when one sink serves both streams, never overlap. A slow sink applies backpressure: once the stream's buffer fills, the guest's writes wait, as on a pipe whose reader is slow.
- Everything the instance and its workers wrote before the instance was disposed is captured or delivered before `RuntimeEvent::Ended` is emitted and before `wait()` returns. A child process still running after disposal may keep writing; Bento keeps draining that stream and discards the bytes, so the child is not interrupted by a closed pipe.
- A sink must not panic. If one does, Bento catches the panic, stops calling that sink for the rest of the run, keeps draining the stream, and the run continues.
- **`InputSource::Bytes`** makes the guest's standard input those bytes followed by end of input, for the main isolate's `process.stdin` and `Deno.stdin`; `isTTY` is false. A child process started with inherited standard input still reads the process's standard input; the embedding guide states this.

#### Reading captured output

```rust
impl RunningRuntime {
    pub fn captured_output(&self) -> CapturedOutput;
}

#[derive(Clone)]
pub struct CapturedOutput { /* … */ }
impl CapturedOutput {
    pub fn stdout(&self) -> Option<CapturedStream>;
    pub fn stderr(&self) -> Option<CapturedStream>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapturedStream {
    pub bytes: Vec<u8>,
    pub total_bytes: u64,
}
impl CapturedStream {
    pub fn truncated(&self) -> bool { self.total_bytes > self.bytes.len() as u64 }
}
```

`captured_output()` returns a handle that outlives the `RunningRuntime`, so a host can keep it and then call `wait()` or proposal 0011's `wait_async()`. `stdout()` and `stderr()` return `None` for a stream that is not `Capture`, and otherwise a snapshot of what has been captured so far, which is final once `Ended` has been emitted.

### Runtime events

```rust
#[derive(Clone)]
pub struct EventSink { /* … */ }
impl EventSink {
    pub fn new(deliver: impl FnMut(RuntimeEvent) + Send + 'static) -> EventSink;
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub enum RuntimeEvent {
    Started { entry: PathBuf },
    Diagnostic(Diagnostic),
    Listening(Listener),
    WorkerStarted { worker: u64, url: String },
    WorkerEnded { worker: u64 },
    Ended(RunOutcome),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub enum Listener {
    Tcp { address: IpAddr, port: u16 },
    Unix { path: PathBuf },
}
```

#### Events

| Event | JSON | When |
| --- | --- | --- |
| `Started` | `{"event": "started", "entry"}` | First, once the instance exists. `entry` is the canonical entry path. |
| `Diagnostic` | `{"event": "diagnostic", "severity", "path", "option", "message"}` | Immediately after `Started`: the diagnostics `RunningRuntime::diagnostics()` returns, in the same order. |
| `Listening` | `{"event": "listening", "transport": "tcp", "address", "port"}` or `{"event": "listening", "transport": "unix", "path"}` | Each time the main isolate or a worker starts listening: Node's `net.Server.listen` (and so `node:http`, `node:https`, and `node:http2`), `Deno.listen`, `Deno.listenTls`, and `Deno.serve`, on TCP or a Unix stream socket. |
| `WorkerStarted` | `{"event": "worker-started", "worker", "url"}` | When a worker's isolate exists, before its main module evaluates. `worker` is unique within the instance; `url` is the worker's main module. |
| `WorkerEnded` | `{"event": "worker-ended", "worker"}` | When that worker's isolate has been disposed. |
| `Ended` | `{"event": "ended", "outcome"}` | Last, once the run has ended. |

#### Rules

- Every `start` that returns `Ok` produces exactly one `Started`, before any other event of that instance, and exactly one `Ended`, after every other. A `start` that returns `Err` produces no events.
- `Listening` is emitted after the operating system accepted the listen call and before the guest observes the listener: before `Deno.listen` or `Deno.listenTls` returns, and before a `listening` event, a listen callback, or `Deno.serve`'s `onListen` runs. It carries the address the operating system reports for the socket: a request for port 0 reports the assigned port, and `0.0.0.0` or `::` is reported as bound. It is emitted by the Rust layer that binds the socket, so guest code can neither produce nor suppress it. A listen call that fails emits nothing.
- Every `WorkerStarted` has its `WorkerEnded` before `Ended`.
- `Ended` carries the outcome `wait()` returns, and proposal 0011's `wait_async()` resolves with. It is emitted when the instance has been disposed, or when its shutdown report becomes `ShutdownOutcome::TimedOut`, whichever comes first; a runtime thread that returns later emits nothing more. A run whose runtime thread panics ends with `Ended(Failed)` of kind `Internal`.
- Calls to one sink never overlap. The runtime calls it on whichever runtime-owned thread observed the event, so a sink that blocks delays that thread and may pause the guest. A sink may call `RuntimeHandle::signal`, but it must not wait for the instance it observes, through `RuntimeHandle::shutdown` or `RunningRuntime::wait`; that deadlocks. A sink panic is caught, Bento stops calling that sink, and the run continues.
- With `events: None`, the runtime constructs no events.
- `RuntimeEvent` is `#[non_exhaustive]`, and consumers of the JSON ignore `event` values they do not know. Audit mode ([#261](https://github.com/bento-js/bento/issues/261)) is expected to report violations as a new variant and a new CLI event; its proposal decides their shape.

### The `bento run` command line

This section writes under proposal 0005's wire contract and defines only what 0005 leaves to this proposal. The output formats and their framing, the rule that decides whether an invocation is structured, the failure document and its body's `code`, `message`, `retryable`, `details`, `suggestions`, and `location`, the definition of `--error-format`, and the behavior when stdout's reader has gone away are 0005's, in its sections "Output formats", "Structured invocations", "Pass-through invocations", "Deciding the contract", "Failure documents", and "Error codes".

#### What this proposal changes in proposal 0005's contract

- It lifts 0005's usage error `--output json requires --dry-run`. Without `--dry-run`, `bento run --output json` is structured and writes a [receipt](#receipts), and `bento run --output ndjson` is structured and writes an [event stream](#event-streams). With `--dry-run`, `--output json` and `text` keep 0005's meaning, and `ndjson` stays rejected.
- It declares `--error-format` on `bento run`'s registry entry. 0005 defines the option and leaves shipping it to this proposal, because only with this proposal's codes can it report every run failure.
- It adds two results to `bento run`'s registry entry: the receipt (`json`, without `--dry-run`) and the event (`ndjson`, one line). Each declares the JSON Schema of its Rust type and at least one example that 0005's parity tests run in the fixture project, and `bento run` declares the statuses under [Exit statuses](#exit-statuses).
- It declares `--max-output` on `bento run`, defined below.
- It adds `exception` to the failure body, a compatible addition under 0005's rule.
- It adds four rows to 0005's code table, and defines the details and suggestions of run failures reported under codes 0005 already has.
- It declares what `bento run` does when stdout's reader has gone away, as 0005 asks each command to declare.

#### `--max-output`

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `--max-output <bytes>` | integer, minimum 0 | 65536 | The bytes of each of stdout and stderr that a receipt keeps. |

It goes before the script. Given without `--output json`, including with `--dry-run`, it fails with `INVALID_ARGUMENT`. `--output json` with `--watch` also fails with `INVALID_ARGUMENT`: a watch session has no single run for a receipt to describe, and `--output ndjson` streams it instead.

#### The failure body's `exception`

Every failure body gains `exception`: the run failure's `Exception`, shaped as [Serialization](#serialization) says, or `null` for a failure that carried no thrown value and for every failure before a run. For a run failure, `location` is the `RuntimeError`'s location and `message` is the summary line of the text report, without color.

```json
{
    "error": {
        "code": "UNCAUGHT_EXCEPTION",
        "message": "uncaught exception: /app/throw.ts: Uncaught (in promise) Error: no 42",
        "retryable": false,
        "details": {},
        "suggestions": [],
        "location": { "path": "/app/throw.ts", "line": 6, "column": 24 },
        "exception": {
            "name": "Error",
            "message": "no 42",
            "code": null,
            "frames": [
                {
                    "function": null,
                    "path": "/app/throw.ts",
                    "url": null,
                    "line": 6,
                    "column": 24,
                    "origin": "application"
                }
            ],
            "customStack": null,
            "cause": null,
            "errors": []
        }
    }
}
```

#### Error codes

This proposal adds these rows to proposal 0005's table:

| Code | Meaning | `retryable` | `details` | `suggestions` |
| --- | --- | --- | --- | --- |
| `STARTUP_FAILED` | A runtime instance could not be set up: its project root, working directory, or script could not be resolved as it started, a host module was declared under a specifier that is not a unique `host:` name, or a worker could not be initialized. | `false` | None. | None. |
| `RESOLUTION_FAILED` | a module specifier could not be resolved | `false` | `specifier`, as written, and `from`, the importing module's absolute path, or its URL when it is not a file. | `bento install`, when the specifier is a bare package name: not relative, not absolute, not `host:`, and not a URL. |
| `UNCAUGHT_EXCEPTION` | A running script threw, or a promise rejected, and nothing handled it; or the script used a runtime feature Bento does not support. | `false` | None; `exception` is the thrown value. | None. |
| `DISPOSAL_INCOMPLETE` | A script ended by itself, but work it started had not settled by the shutdown deadline, so its instance was not fully disposed of. | `false` | None. | None. |

`RESOLUTION_FAILED`'s meaning is proposal 0005's, word for word, because proposal 0006 adds the same code and whichever is implemented first adds the row. `specifier` and `from` are the names 0006's report gives those facts; a `bento resolve` failure adds the rest of 0006's report to them.

A run failure reports the code its kind maps to:

| `ErrorKind` | Code | Row defined by | A run failure's `details` | A run failure's `suggestions` |
| --- | --- | --- | --- | --- |
| `Startup` | `STARTUP_FAILED` | this proposal | None. | None. |
| `Configuration` | `INVALID_CONFIGURATION` | proposal 0005 | `path`: the `tsconfig.json`, or the file it extends, at fault. | None. |
| `Resolution` | `RESOLUTION_FAILED` | this proposal or proposal 0006 | `specifier`, and `from` from the referrer: a path for a `file:` URL, otherwise the URL. | As in its row. |
| `Syntax` | `SYNTAX_ERROR` | proposal 0005 | None; `location` is the syntax error. | None. |
| `Transform` | `TRANSFORM_FAILED` | proposal 0005 | None; `location` is the syntax. | None. |
| `Permission` | `PERMISSION_DENIED` | proposal 0005 | `denial`: the `Denial`'s `name`, `code`, `permission`, and `resource`, or no `denial` without one. | The flag that would allow it, when the inherited ceiling allows that flag. |
| `Uncaught` | `UNCAUGHT_EXCEPTION` | this proposal | None. | None. |
| `Internal` | `INTERNAL` | proposal 0005 | None. | 0005's suggestion to report the defect. |

A run that ended by itself with `RunOutcome::Shutdown` and `ShutdownOutcome::TimedOut` reports `DISPOSAL_INCOMPLETE`.

A run's denial is nested under `denial` because 0005 already gives `PERMISSION_DENIED` a `permission` for a launch that cannot read its own script, in Bento's vocabulary (`fs-read`), while `denial.permission` is Node's name for what the guest was denied (`FileSystemRead`). A permission suggestion names the flag proposal 0003's table gives for the denied permission: `--allow-fs-read=<resource>` and `--allow-fs-write=<resource>` for file system denials with a resource (the bare flag without one), `--allow-net`, `--allow-child-process`, `--allow-worker`, `--allow-wasi`, `--allow-inspector` for those permissions, `--allow-addons` for `ERR_DLOPEN_DISABLED`, and `--allow-language-model` for `NotAllowedError`. It reads `add --allow-net before the script`. A permission without a flag in that table gets no suggestion.

Failures before a run report 0005's codes. Two of them are `bento run`'s alone: when `--watch` cannot watch the project, `PRECONDITION_FAILED` with `details.path`; and when Bento cannot install its signal handlers, `INTERNAL`.

#### Run fields

The receipt and the `ended` event describe a run with the same fields:

| Field | Value |
| --- | --- |
| `outcome` | `completed`, `exited`, `failed`, or `stopped` |
| `exitCode` | The guest's exit code: `0` for `completed`, the code it exited with for `exited`, otherwise `null`. A code outside 0–255 is reported as the guest gave it. |
| `stopReason` | `null`, or why Bento asked the run to stop: `signal` or `restart`. Set even when the guest then ended by itself. |
| `signal` | `SIGINT` or `SIGTERM` when `stopReason` is `signal`, otherwise `null` |
| `shutdown` | When `outcome` is `stopped`, how the requested stop went: `graceful`, `interrupted`, `incomplete`, or `abandoned`. Otherwise `null`. |
| `durationMs` | Milliseconds from just before Bento started the run's instance to the run's end, or to the moment Bento abandoned it |
| `error` | The failure body when `outcome` is `failed`, otherwise `null` |

They derive from the outcome and from whether Bento asked the run to stop:

| The run ended with | `outcome` | Other fields |
| --- | --- | --- |
| `start` returned an error | `failed` | `error` from the `RuntimeError` |
| `Completed` | `completed` | `exitCode: 0` |
| `Exited(code)`, or a requested stop reporting `Graceful { exit_code: Some(code) }` | `exited` | `exitCode: code` |
| `Failed(error)`, or a requested stop reporting `Failed(error)` | `failed` | `error` from the `RuntimeError` |
| `Shutdown(TimedOut)` without a stop request | `failed` | `error` with `DISPOSAL_INCOMPLETE` |
| A requested stop reporting `Graceful { exit_code: None }` | `stopped` | `shutdown: graceful` |
| A requested stop reporting `Interrupted` | `stopped` | `shutdown: interrupted` |
| A requested stop reporting `TimedOut` | `stopped` | `shutdown: incomplete` |
| Bento exited before the run ended: a second signal, or Bento's own deadline after a signal | `stopped` | `shutdown: abandoned` |

#### Exit statuses

`bento run` declares the same exit statuses for its receipt and its event stream as for a pass-through run, unchanged: `0` for `completed`; the exit code's low eight bits for `exited`; `1` for `failed` and for a failure before a run; and 128 plus the signal's number when a signal stopped or abandoned the run. A structured `bento run` therefore does not always exit with 0 on success, as 0005's discovery commands do; its status is the run's.

#### Receipts

A receipt is `bento run`'s `json` result. Once Bento has prepared the launch (arguments, project, permissions, and environment) and asked the runtime to start the script, it writes exactly one receipt when the run ends or is abandoned, whether or not the instance started. A failure before that point is a failure before the first document, which 0005's rules report.

```json
{
    "entry": "/app/build.ts",
    "outcome": "exited",
    "exitCode": 2,
    "stopReason": null,
    "signal": null,
    "shutdown": null,
    "durationMs": 184,
    "stdout": { "text": "built 3 pages\n", "totalBytes": 14, "truncated": false },
    "stderr": { "text": "warning: …", "totalBytes": 91204, "truncated": true },
    "diagnostics": [],
    "error": null
}
```

- `entry` is the canonical script path, followed by the run fields.
- `stdout` and `stderr` are the captured streams, each captured with `--max-output` bytes. `text` is the kept bytes decoded as UTF-8, with invalid sequences, including one cut by the limit, replaced by U+FFFD; control characters, terminal escapes included, are escaped as 0005 requires. `totalBytes` counts every byte the stream carried. `truncated` is `true` exactly when `totalBytes` exceeds the number of bytes kept.
- `diagnostics` lists the run's diagnostics as CLI diagnostic objects (below).
- When a second signal or the deadline abandons the run, the receipt is written before Bento exits, with the output read so far.

#### Event streams

An event stream is `bento run`'s `ndjson` result: each line is one event. Each event has `event`, `elapsedMs` (milliseconds since `bento` started, from a monotonic clock), and `run` when it belongs to a run.

| `event` | `run` | Fields |
| --- | --- | --- |
| `started` | yes | `entry` |
| `diagnostic` | for `tsconfig.json` diagnostics | `code`, `severity`, `message`, `path`, `option` |
| `listening` | yes | `transport`, and `address` and `port`, or `path` |
| `output` | yes | `stream` (`stdout` or `stderr`), `text` |
| `worker-started` | yes | `worker`, `url` |
| `worker-ended` | yes | `worker` |
| `ended` | yes | the run fields |
| `changed` | no | `paths` |
| `restarting` | yes: the run about to start | none |

- `started`, `listening`, `worker-started`, and `worker-ended` are the runtime's events with `run` and `elapsedMs` added. `diagnostic` carries a CLI diagnostic object, and `ended` is the CLI's description of the run, not the library's `RunOutcome`.
- A CLI diagnostic object has `code` `TSCONFIG_OPTION` for a `tsconfig.json` diagnostic, carrying its `severity`, `message`, `path`, and `option`, or `WATCH_UNAVAILABLE` for a file `--watch` cannot watch yet, with severity `warning`, the file as `path`, and `option` `null`. A watch diagnostic belongs to no run. These diagnostic codes name diagnostics, not failures, and are not rows of 0005's error-code table.
- `output` carries the guest's output: the instance's stdout and stderr, including its workers' and the inherited output of its child processes. Each stream is decoded as UTF-8 incrementally, so a character split between chunks appears whole in the later event, and invalid sequences become U+FFFD. Concatenating a stream's `text` values gives the stream's whole output. Events are not aligned to lines, and output is not bounded.
- Within a run, `started` comes first when the instance started, and `ended` comes last; every `output` of a run precedes its `ended`. Otherwise, `output` is not ordered against the run's other events. A run whose instance could not start has `ended`, with `outcome: failed`, and no `started`.
- A run failure after the first event is reported in the run's `ended`, as 0005 anticipates for this stream; a failure before the first event is reported by 0005's rules.
- Runs never overlap. In watch mode, a burst of changes that leads to a start produces `changed`, then `restarting` with the next run's number, then, if a run was running, its `ended` with `stopReason: restart`, then the next run's events. `paths` are the changed files' absolute paths, sorted and without duplicates. A burst that arrives while a restart is already underway or while Bento is stopping produces no events.
- The text messages of pass-through watch mode, `watch: restarting…`, `…finished`, `…exited with code`, and `…waiting for changes…`, are not written; the events replace them.
- Without `--watch`, Bento exits after the run's `ended` with the run's status. With `--watch`, a signal stops the running run, whose `ended` reports it, and Bento exits; a signal while no run is running ends Bento at once, writing nothing more.

#### Standard streams of a structured run

- Bento itself reads nothing from stdin, as 0005 requires of a structured invocation. The script's standard input is inherited, as in a pass-through run, so `bento run --output json transform.ts < data.json` works.
- Before starting any run, Bento keeps a duplicate of its standard output for its own documents and points file descriptor 1 at its standard error. Anything else that writes to file descriptor 1, such as a native addon or a child given that descriptor explicitly, then lands on stderr instead of inside the result.
- For a reader that has gone away, `bento run` stops writing to stdout, shuts down a running instance as it would for `SIGTERM`, and ends once the run has ended. 0005's rule for a reader that left then applies to what it writes and its status.

#### Pass-through runs

`--error-format` behaves as 0005's "Pass-through invocations" defines. A run failure's failure document carries the codes, details, and `exception` above.

### Documentation

- [Running with Bento](../guides/running-with-bento.md) gains a section on receipts and event streams: their contents, the run fields, and the codes this proposal adds. Proposal 0005's wire-contract section covers output formats, failure documents, and `--error-format`, and its generated CLI reference lists the codes from the `ErrorCode` enum. The guide's error-report and exit-code sections note the reclassified kinds' new titles.
- [Embedding Bento](../guides/embedding-bento.md) documents `Stdio`, capture, sinks, `RuntimeEvent`, and the structured error fields, and replaces its statement that an unreadable `tsconfig.json` is `ErrorKind::Transform`.
- The text report's title and label for the new kinds are `the project configuration is invalid` (label `invalid here`), `a module has a syntax error` (label `syntax error here`), and `Bento failed internally`.

## Compatibility

- A plain `bento run`, with or without `--watch`, writes the same bytes and exits with the same statuses, except that a report for a reclassified failure has the new title and label above: a JavaScript syntax error no longer says `the script could not start`, and a TypeScript syntax error no longer says `a module could not be transformed`. The new options are opt-in.
- Library changes are breaking for embedders, and Bento publishes no crate yet (`publish = false`), so a changeset records them:
    - `ErrorKind` gains `Configuration`, `Syntax`, and `Internal` and becomes `#[non_exhaustive]`; an exhaustive `match` needs a wildcard arm.
    - Failures change kind: an unreadable `tsconfig.json` from `Transform` to `Configuration`; Oxc parse and semantic errors from `Transform` to `Syntax`; V8 parse errors and CommonJS compile errors from `Startup` to `Syntax`; panics and thread or async-runtime creation failures from `Startup` to `Internal`.
    - `RuntimeConfig` gains `stdio` and `events`. Code that builds it with `..RuntimeConfig::new(root)` is unaffected; a full struct literal must add them.
    - `RunOutcome` gains `Clone`, `PartialEq`, `Eq`, and `Serialize`. `RuntimeError.message` and `location()` are unchanged.
- A guest under `Capture` or `Sink` sees streams that are not terminals, so libraries that color output for terminals stop doing so. Under `Inherit` nothing changes.
- Proposal 0005 rejects `bento run --output json` without `--dry-run` and does not declare `--error-format` on `bento run`. This proposal lifts the first and ships the second; with `--dry-run`, 0005's behavior is unchanged.
- Proposal 0011 says a panic is reported with `ErrorKind::Startup`. If both are accepted, it is `ErrorKind::Internal`, with the same message.
- Later additions, such as a timeout from proposal 0009, follow proposal 0005's compatibility rule, under which the receipt, the events, and the run fields also fall: new fields, enumerated values, events, and codes are compatible, and removing or renaming one is breaking. `RuntimeEvent` and `ErrorKind` are `#[non_exhaustive]` for the same reason.

## Implications on adoption

Nothing new is installed and no flag changes meaning. Agents opt in with `--output json`, `--output ndjson`, or `--error-format json`; embedders opt in through `RuntimeConfig.stdio` and `RuntimeConfig.events` and recompile for the `ErrorKind` and `RuntimeConfig` changes.

Implementation follows proposal 0003, because permission details read Node's denial errors, and assumes proposal 0004's script-only `bento run`. Each part can be reverted by a later proposal without migrating stored data.

## Scope

- `bento_compiler`: `Serialize` for `Diagnostic` and `Severity`, and `TransformError::kind` if proposal 0005 has not added it.
- `bento_runtime`: the `ErrorKind` changes and reclassification; `Exception`, `StackFrame`, `ErrorDetails`, and `Denial`, filled from uncaught exceptions, including their `code`, `permission`, and `resource` properties; `Serialize` on the listed types; `Stdio`, `OutputTarget`, `InputSource`, `OutputSink`, and `CapturedOutput`, applied to the main isolate, workers, `op_print`, and inherited child stdio; `EventSink` and `RuntimeEvent`, including listener observation for Deno's and Node's listen paths.
- `bento_cli`: on `bento run`'s registry entry, the receipt and event results with their schemas and examples, `--max-output`, `--error-format`, the codes it reports, and its exit statuses; the `ErrorCode` variants `STARTUP_FAILED`, `UNCAUGHT_EXCEPTION`, `DISPOSAL_INCOMPLETE`, and `RESOLUTION_FAILED` unless proposal 0006 has added it; `exception` in the failure body; receipts; event streams for plain and watch runs; run failures as failure documents under `--error-format json`; descriptor protection and the behavior for a reader that has gone away; the new text titles.
- Tests: library tests for capture (console, `process.stdout`, workers, truncation counts, a child that outlives the instance), events (order, exactly-once `Started` and `Ended`, `Listening` on port 0 for `node:http` and `Deno.serve`, `TimedOut`), and every error field (name, bare message, `code`, frame origins, `cause`, aggregated errors, custom stacks, denial details through a `fetch` cause); CLI tests for receipts, event streams across watch restarts and failed reloads, run failures as failure documents, usage errors in structured runs, option conflicts, and statuses.
- The guide changes under [Documentation](#documentation), and a changeset.

### Out of scope

- The wire contract itself, the command registry, `cli search`, `schema`, `docs`, `context show`, and `run --dry-run` (proposal 0005); `bento resolve` (0006); inline and stdin evaluation (0008); `--process-timeout` and heap limits (0009); `node:vfs` (0010); async `wait` and `shutdown` (0011); persistent sessions (0012); typed host modules (0013).
- Permission audit mode ([#261](https://github.com/bento-js/bento/issues/261)); this proposal only leaves room for its events.
- Structured failures for `bento install`, which proposal 0005 keeps pass-through.
- Async-operation spans, module-resolution traces, resource statistics, and telemetry export.
- Streaming standard input, events for listeners closing, and detecting the protocol a listener serves.
- Node's `process.emitWarning` warnings as diagnostics; they remain guest output.
- Capturing native writes to file descriptors 1 and 2 per instance, which the operating system does not allow.
- Line-aligned output events, binary-safe output encodings, and wall-clock timestamps.
- Changing exit statuses.
- The `jsx: preserve` diagnostic severity ([#265](https://github.com/bento-js/bento/issues/265)) and the readiness flake ([#257](https://github.com/bento-js/bento/issues/257)), which remain separate defects.

## Preview

- Artifact: the installable `bento` archive from this pull request's macOS CI job (`bento-macos27-arm64-prompt`), and the documentation preview at `pr-preview/pr-277/` for the guides.
- Reason: the CLI is the cheapest real surface. Installing the archive and running `bento run --output json` on a script that throws, `bento run --output ndjson --watch` on a server while editing it, and `bento run --error-format json` on a denied operation under `--permission` exercises receipts, events, failure documents, and the structured errors beneath them. The library surface is exercised by its tests and by the guide's example.
- Access and cleanup: CI artifacts are private to the repository and expire on GitHub's schedule; the documentation preview is removed when the pull request closes.

## Policies and decisions checked

- policy.0001 (prefer VoidZero crates) — honored. Nothing here parses, transforms, resolves, or maps sources with a new crate. Frames are mapped by `deno_core` with the source maps Oxc already produces, and syntax errors are split using Oxc's own diagnostics. `serde` and `serde_json` are existing dependencies.
- `decisions/` holds no records.
- [The vision](../VISION.md) — capture and events are library features the CLI consumes ("Embedding is the default"), and output destinations are the host's decision ("The host decides what code may do"). [The architecture](../ARCHITECTURE.md)'s "structured errors, exceptions, … source maps" for hosts are delivered in part.
- [Proposal 0001](0001-embeddable-typescript-runtime.md) — the runtime still prints nothing of its own, and pass-through `bento run` stays Node-like.
- Proposal 0003 — denial errors keep Node's shapes; this proposal only reads them. [Proposal 0002](0002-web-prompt-api.md)'s `NotAllowedError` is recognized as a denial.
- Proposal 0005 — on 2026-11-27 Mark decided that 0005 owns the output formats, the error format, and the code list, and that this proposal adopts them. This proposal restates none of them and adds only what 0005 leaves to it: its two results, `--max-output`, `exception`, four code rows, and run details.

## Future directions

- Permission audit events ([#261](https://github.com/bento-js/bento/issues/261)), as a `RuntimeEvent` variant and a CLI event.
- `bento install --error-format json`, if `install` becomes structured.
- Async-operation spans, module-resolution traces, and resource statistics as further events, and optional telemetry export.
- Streaming standard input for persistent sessions (proposal 0012), and wall-clock timestamps on events.
- Keeping the tail, or the head and the tail, of a captured stream, if the open question below settles on the head.

## Alternatives considered

- **A separate output contract and code table for runs.** This proposal's first draft defined its own `--error-format`, failure body, framing rules, and run code table beside proposal 0005's, and asked which should own them. Contracts defined one proposal at a time drift: `--output` could mean different things on different commands, and code lists could collide. On 2026-11-27 Mark decided that proposal 0005 owns the output formats, the error format, and the code list, and that this proposal adopts them.
- **A separate `--events ndjson` flag beside `--output json`.** Both own stdout and cannot be combined, so they are two values of one choice; two flags would make an impossible combination expressible.
- **Events on stderr, or on a dedicated descriptor or file, with guest output passed through.** On stderr, guest text mixes with events and a guest line can impersonate one. A descriptor or file is clean but needs plumbing in every agent harness. Wrapping guest output in `output` events, as `go test -json` does, keeps one parseable stream.
- **Separate `exited`, `failed`, `stopped`, and `reloaded` events.** They are one fact, how a run ended, with different values; separate events would repeat the outcome's classification and drift from the receipt. `reloaded` is `restarting` followed by `started`.
- **Flat receipt fields (`stdoutTruncated`, `stdoutTotalBytes`) and top-level `frames` and `suggestion`.** A captured stream is one concept with three facts, frames belong to an exception and its causes each have their own, and `suggestions` is proposal 0005's array.
- **Flat denial fields in `PERMISSION_DENIED`'s details.** Proposal 0005 already gives the code a `permission` in Bento's vocabulary (`fs-read`); putting Node's `permission` (`FileSystemRead`) under the same key would mix two vocabularies in one field.
- **Only sinks in the library, with bounded capture left to hosts.** Every host would reimplement the same bounded buffer and its counts; the CLI and embedders share one implementation instead.
- **Returning captured output from `wait()`.** It would change `wait()`'s type and need a twin for proposal 0011's `wait_async()`. A handle works with any way of waiting.
- **Process-wide redirection with `dup2` for capture.** It cannot tell instances apart. It is used only to keep a structured run's stdout clean.
- **Keeping `ErrorKind` and adding finer codes only in the CLI.** `RuntimeError` does not carry the facts that distinguish a syntax error from a panic, and embedders need them as much as the CLI.
- **Serializing every own property of a thrown error.** It is unbounded and can serialize large guest objects; consumers branch on `code` and on the denial's fields, which are captured.
- **Lossless base64 for output that is not UTF-8.** Scripts run by agents write text, `go test -json` coerces invalid UTF-8 the same way, and binary output belongs in a file.
- **Detecting readiness from guest output or by patching `listen` in JavaScript.** Neither is a fact the runtime observed, and guest code could forge or suppress it.
- **A `protocol` field on `listening`.** The runtime sees a TCP or Unix socket being bound; whether HTTP, HTTPS, or something else is served over it is decided in JavaScript, and `node:https` applies TLS per connection above a plain TCP listener. `transport` names what is actually known.

## Open questions

- [NEEDS CLARIFICATION: What should `--max-output` default to? This draft uses 65536 bytes per stream, so a receipt fits an agent's context; 1 MiB would bound memory without truncating ordinary logs.]
- [NEEDS CLARIFICATION: Which bytes should a truncated capture keep? This draft keeps the head, so the kept bytes are an exact prefix of the stream and the failure itself is in `error`; keeping the tail, or the head and the tail with the gap marked, would show the end of long logs.]

## Acknowledgments

The output formats, failure document, and code table are proposal 0005's, modeled on Mark's `mcp` CLI. `go test -json` informed the event stream's treatment of program output.
