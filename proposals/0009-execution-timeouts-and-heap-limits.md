---
id: proposal.0009
title: Execution Timeouts and Isolate Heap Limits
authors: [Mark]
status: draft
pull-request: https://github.com/bento-js/bento/pull/272
issues: []
supersedes: []
---

# Execution Timeouts and Isolate Heap Limits

## Summary

A Bento run can be given a wall-clock budget and V8 heap limits. Exceeding either ends that run with a distinct, reported failure instead of hanging or aborting the host process. The CLI adopts Node 26's `--process-timeout`, with exit status 124 and Node's report of what was still running.

## Motivation

Agents and harnesses run scripts they did not write and cannot watch: verification scripts, generated tests, and snippets run through `bento run` or through an embedded `bento_runtime` instance. Two kinds of accident end those runs badly today.

**A run that never ends.** `while (true) {}`, a forgotten `setInterval`, a server that is never closed, or a promise awaiting an event that never comes keeps `bento run` alive indefinitely. Nothing in Bento bounds how long a guest executes; `RuntimeConfig.shutdown_deadline` only bounds a shutdown someone has already requested. The workarounds lose information:

- Wrapping the command in `timeout(1)` sends `SIGTERM`, which Bento forwards to the guest's listener and then waits the five-second shutdown deadline. The status says a timeout happened, but nothing says what was running: which loop, which listener, which timer.
- An embedding host can start its own timer thread and call `RuntimeHandle::shutdown()`. The guest's `SIGTERM` listener then gets another full shutdown deadline to drain, and the outcome (`Graceful` or `Interrupted`) is indistinguishable from a host that stopped the instance for any other reason.

Node 26.11 added `--process-timeout` for exactly this: the process exits with `timeout(1)`'s status, 124, and prints the JavaScript stack or the resources that kept the event loop alive. Agents that already know Node expect that status and that report.

**A run that exhausts memory takes the host with it.** No host setting limits the main isolate's heap. When a guest reaches V8's default limit, V8 aborts the whole process. A local debug build of `bento` running a script that keeps growing an array aborted after four seconds at about 4 GiB with V8's `Fatal JavaScript out of memory: Ineffective mark-compacts near heap limit` and status 133. In an embedding, the aborted process is the host: the harness dies with its guest. Workers are no safer. A worker started without `resourceLimits` gets no near-heap-limit handling (`workers.rs` installs it only for limited workers), so reaching V8's default limit aborts the process the same way. A guest that asks for a tiny worker heap, `new Worker(src, { eval: true, resourceLimits: { maxOldGenerationSizeMb: 1 } })`, aborted the same build at once (status 133, V8's "Last few GCs" report).

The people affected are coding agents and the harnesses that run them, CI jobs that verify generated code, and native applications embedding Bento. The cost today is hung jobs that only an outer supervisor can kill, aborted host processes, and failure reports that say nothing about the cause.

## Domain grounding

### Established context

- **Node's process timeout.** `--process-timeout=<duration>` was added in Node 26.11.0 (Stability 1.1, [CLI documentation](https://nodejs.org/api/cli.html#--process-timeoutduration)). The duration is "measured from the start of the process"; at expiry the process exits with 124, "the one used by the `timeout(1)` command" ([GNU coreutils](https://www.gnu.org/software/coreutils/manual/html_node/timeout-invocation.html)). Before exiting Node prints to stderr what the main thread was doing and which resources kept the event loop alive. It skips `beforeExit` and `exit`, still writes coverage and profiles, applies per run under `--watch`, is not allowed in `NODE_OPTIONS`, and cannot be combined with the inspector. If the main thread does not respond within two seconds, Node exits without a stack or resource list.
- **Observed on Node 26.13.1** (`mise exec node@26.13.1`):
    - A busy loop prints `(node:PID) Process timed out after 500ms (--process-timeout). Exiting with code 124.`, then `Main thread was executing JavaScript:`, the stack frames indented by four spaces, and `No resources keeping the event loop alive were found.`; status 124.
    - A pending `setInterval` prints `Main thread was not executing JavaScript.`, `Resources keeping the event loop alive:`, and `    Timeout (next due in 443ms)`. A listening `node:http` server adds `    TCPServerWrap (listening on [::]:53091, fd 12)`. A spinning worker appears as `    Worker (thread 1, name 'WorkerThread')`.
    - A main thread blocked in `execSync('sleep 6')` prints `The main thread did not respond within 2000ms. It is likely blocked in a synchronous native operation, e.g. child_process.execSync() or a native addon, so no JavaScript stack or resource information is available.` and exits 124 after about two seconds more.
    - The report goes to stderr only. `beforeExit` and `exit` listeners do not run, and `process.exitCode = 7` still yields 124.
    - The duration is a positive integer followed by exactly `ms`, `s`, `m`, or `h`. `300`, `0`, `0ms`, `-1s`, `+1s`, `1.5s`, `1e3ms`, `1S`, `300MS`, `2d`, `300us`, `1m30s`, and a leading space are rejected with `invalid value for --process-timeout: '<value>'. Expected a positive integer followed by a unit (ms, s, m or h), e.g. 30s` and status 9; an empty value with `--process-timeout= requires an argument`. Leading zeros are accepted, and the header echoes the value as given (`after 01s`). The largest accepted values are `9223372036854ms` and `2562047h`: the duration in nanoseconds must fit in a signed 64-bit integer.
    - `--process-timeout 300ms` (space-separated) works. Repeating the flag uses the last value, as Node documents for every single-value option. In `NODE_OPTIONS` it fails with `--process-timeout= is not allowed in NODE_OPTIONS`, status 9.
    - `--process-timeout=1ms` times out a script that only logs a line: the clock includes bootstrap.
    - Under `--watch`, a timed-out run prints the report, then `Failed running 'timer.mjs'. Waiting for file changes before restarting...`; an edit restarts it with a fresh timeout.
- **Node 27.0.0-alpha.0.2.0** adds `--process-timeout-worker-stacks`, which also prints each worker's stack and resources after the main thread's summary, and lists workers that do not respond within two seconds without them. Without `--process-timeout` it fails with `--process-timeout-worker-stacks must be used with --process-timeout`. `--report-on-process-timeout` (26.11) additionally writes a diagnostic report.
- **V8 heap limits.** V8 limits each isolate's heap through `ResourceConstraints`, set at isolate creation (`v8::CreateParams::set_max_old_generation_size_in_bytes`, `heap_limits`, `heap_limits_from_system_memory` in `v8` 150.4.0). Near the limit V8 performs garbage collections and invokes the isolate's near-heap-limit callback; "if the garbage collections do not help and the callback does not increase the limit, then V8 will crash with V8::FatalProcessOutOfMemory". Only the most recently added callback is invoked. The callback runs during garbage collection, where no JavaScript can run and no stack can be captured. `IsolateHandle::terminate_execution` is thread-safe but asynchronous: it takes effect when V8 next checks for interrupts, so the heap must be allowed to grow a little past the limit while termination unwinds. `request_interrupt` runs a callback on the isolate's thread the next time JavaScript checks for interrupts; it does not reach an idle event loop.
- **Node's heap behavior** (26.13.1, observed):
    - `--max-old-space-size=32` with a growing array aborts with `FATAL ERROR: Ineffective mark-compacts near heap limit Allocation failed - JavaScript heap out of memory` and status 134.
    - A worker with `resourceLimits: { maxOldGenerationSizeMb: 32 }` ends with an `error` event `ERR_WORKER_OUT_OF_MEMORY` ("Worker terminated due to reaching memory limit: JS heap out of memory") and exit code 1, and the main thread continues. Node installs that handling on every worker isolate, limited or not, and extends the limit by 16 MiB per call while the worker terminates (`Worker::NearHeapLimit`, [`src/node_worker.cc`](https://github.com/nodejs/node/blob/v26.13.1/src/node_worker.cc#L272-L294)).
    - `--max-old-space-size` is a process-wide V8 flag: with `--max-old-space-size=64` every worker got the 64 MiB limit, overriding `resourceLimits` in both directions, while `worker.resourceLimits` still reported the requested values.
    - An isolate whose limit is too small aborts the process while deserializing its startup snapshot (`Fatal JavaScript out of memory: GC during deserialization`): `--max-old-space-size=1`, `2`, and `4` aborted Node itself, `8` started, and a worker asking for 1 MiB aborted the whole process.
    - The limit counts V8's heap only. A worker limited to 32 MiB held 1024 MiB of `Buffer`s and exited normally, because array-buffer backing stores live outside the V8 heap. The reported `heap_size_limit` is the old-generation limit plus the young generation (64 MiB of old space gave 160 MiB).

### Relevant constraints and principles

- [VISION.md](../VISION.md): "Not a sandbox merely because it uses isolates." [ARCHITECTURE.md](../ARCHITECTURE.md): "Isolate memory limits and interrupt hooks are not a complete defense against untrusted native code, denial of service, or engine vulnerabilities." In-process limits protect a host from accidental runaway loops and allocations in guest JavaScript, not from hostile code, native addons, blocking native calls, or memory outside V8's heap.
- Termination cannot interrupt native code. A blocking host function, a synchronous native operation, or an addon call keeps the runtime thread until it returns; the existing `ShutdownOutcome::TimedOut` already reports that case for shutdowns.
- `deno_core` 0.412.0 (Bento's fork at `825264984ee3`) exposes `JsRuntime::add_near_heap_limit_callback` (replacing any earlier callback on that isolate) and `runtime_activity_stats_factory()`, which captures pending async operations by name, open resources by name, and active timers and intervals. It records neither a timer's due time nor whether it is referenced.
- `deno_lib` 0.77.0's `LibMainWorkerFactory::create_custom_worker` always creates the main isolate with `create_isolate_create_params(sys)`, a limit derived from physical memory. Its `unconfigured_runtime` parameter accepts a `deno_runtime::UnconfiguredRuntime`, whose `UnconfiguredRuntimeOptions` carry their own `create_params`. `deno_runtime` 0.267.0 silently discards that runtime when stack traces in ops (`DENO_TRACE_PERMISSIONS`) or op metrics are enabled.
- The upstream Deno crates are pinned as a set (AGENTS.md). Changing how one of them creates isolates means using an existing upstream entry point or owning the construction in Bento, not patching one crate.

### Quality bar

- An agent can rely on status 124 meaning the run outlived its timeout, and on stderr saying, in Node's words, what was running.
- A guest that exhausts its heap ends its run, or only its worker, and the host process keeps running, within a documented envelope.
- Limit failures are distinguishable from guest exceptions, from host-requested shutdowns, and from incomplete disposal.
- A configured limit is in force or the instance does not start. There is no silent fallback to running unlimited.
- What the limits do not cover is stated where an embedder reads it.

### Remaining uncertainty

- The exact names `deno_core` reports for Node-compatibility resources, such as a listening `node:http` server, are not verified here; the design specifies how resources are listed, not their runtime names.
- The smallest heap limit with which Bento's startup snapshot bootstraps is unmeasured. Node's own snapshot needed more than 4 MiB and at most 8 MiB; Bento's is larger.
- Supplying the main isolate's parameters through `UnconfiguredRuntime` changes how extensions are registered. Whether that preserves every current behavior must be shown by the existing runtime suite; if it cannot, Bento constructs its main worker directly.
- The local evidence of today's aborts comes from a debug build of the in-progress Proposal 0003 implementation; its worker heap-limit and main-isolate creation code is identical to `main`.

## Existing baseline

`crates/bento_runtime/src/lifecycle.rs` is the model to extend. `Control` owns one state machine per instance and a `bento-shutdown` monitor thread. `request_shutdown` fixes a deadline; at the deadline `enforce_deadline` terminates the main isolate and every registered worker (`State::terminate_isolates`), sends `Command::Dispose`, waits the 100 ms `DISPOSAL_MARGIN`, and records `ShutdownOutcome::TimedOut` if disposal has not finished. `fail_feature` records a failure and terminates every isolate, and `assembly::run` reports that recorded failure instead of the execution result. Worth preserving: one `Control`, deadlines enforced by the monitor rather than by whoever waits, the first recorded report winning, termination of every registered isolate, and the meaning of `TimedOut` as incomplete disposal. This proposal adds a second deadline to the same monitor and generalizes the recorded feature failure into the cause that ended the run.

`crates/bento_runtime/src/assembly.rs` creates the main worker with `create_custom_worker(…, None)`, so its isolate gets the physical-memory default, and installs no near-heap-limit callback. Reaching that limit aborts the process.

`crates/bento_runtime/src/workers.rs` builds a worker's parameters from `v8::CreateParams::default()` plus the guest's `resourceLimits`, and installs a near-heap-limit callback (copied from `deno_lib`) that marks the worker out of memory, terminates it, and doubles the limit, but only when the guest passed `resourceLimits`. A limited worker that runs out ends with `ERR_WORKER_OUT_OF_MEMORY` as in Node (observed). An unlimited worker aborts the process, and nothing stops a guest from requesting a limit too small to bootstrap. The doubling and the unguarded request are incidental; the termination path through `oom_triggered` is worth keeping.

`crates/bento_runtime/src/error.rs` defines `ErrorKind` (`Startup`, `Resolution`, `Transform`, `Permission`, `Uncaught`), `RuntimeError` (a ready-to-show `message` and an optional `SourceLocation`), and `RunOutcome`.

In `crates/bento_cli`, `script.rs` runs one instance and loops over events; `ending.rs` maps outcomes to statuses; `Stop` keeps its own copy of the signal deadline and exits `bento` when it passes, a CLI-side backstop for a runtime that has not reported. `watch/restarts.rs` starts a fresh instance from a cloned `RuntimeConfig` for each run and reports how each run ended. Nothing bounds a run's time.

The guides already state the gap. [Embedding Bento](../guides/embedding-bento.md) says the main isolate and workers without `resourceLimits` "have no configured heap quota" and that allocation can "abort the embedding process". [Running with Bento](../guides/running-with-bento.md) has no way to bound a run.

## Proposed solution

Give each run two host-set limits, both enforced inside the instance and both reported as failures of their own kind.

The **execution timeout** is a wall-clock budget for a run's execution. When it expires, Bento records why the run is ending, captures what the main thread was doing, terminates JavaScript in every isolate without letting the guest drain, and disposes of the instance. `wait()` returns `RunOutcome::Failed` with `ErrorKind::ExecutionTimeout`, and `RuntimeError::activity()` holds Node's two answers: the JavaScript stack, or the resources keeping the event loop alive.

**Heap limits** set the V8 old-generation limit of the main isolate and the most any worker may have. Every isolate gets near-heap-limit handling, limited by the host or not. A worker that reaches its limit ends with `ERR_WORKER_OUT_OF_MEMORY` while the run continues, as in Node. The main isolate reaching its limit ends the run with `ErrorKind::HeapLimit`. In neither case is the host process aborted.

The CLI is one host of these limits. `bento run --process-timeout=30s` is Node's flag, with Node's syntax, status 124, Node's report, and per-run semantics under `--watch`.

```text
bento run --process-timeout=2s spin.ts
  → stderr: (bento:4242) Process timed out after 2s (--process-timeout). Exiting with code 124.
            Main thread was executing JavaScript:
                at spin (file:///work/spin.ts:2:9)
                at file:///work/spin.ts:4:1
            No resources keeping the event loop alive were found.
  → status 124

bento run --watch --process-timeout=2s server.ts
  → each run gets two seconds; a timed-out run is reported and the watch waits for changes

RuntimeConfig { execution_timeout: Some(Duration::from_secs(2)), .. }
  → wait() == RunOutcome::Failed(error), error.kind == ErrorKind::ExecutionTimeout, error.activity() == Some(..)

RuntimeConfig { heap_limits: HeapLimits { main: Some(256 << 20), worker: None }, .. } + runaway allocation
  → wait() == RunOutcome::Failed(error), error.kind == ErrorKind::HeapLimit; the host keeps running
```

## Detailed design

### Vocabulary

- **Run**: one instance, from the call to `start` until it has been disposed of. Under `--watch`, every restart is a new run.
- **Execution**: the part of a run in which guest code may run, from the call to `start` until the guest's event loop runs out of work, the guest calls `process.exit`, an uncaught failure ends it, or a requested shutdown ends it. Disposal follows execution.
- **Execution timeout**: the host's wall-clock budget for one run's execution (`RuntimeConfig.execution_timeout`). The CLI calls it the **process timeout**, Node's name, because for the CLI a run is what Node calls a process. They are one concept.
- **Shutdown deadline**: unchanged. It bounds a requested shutdown and the settling of work after execution ends. Time running out never starts a shutdown, and the shutdown deadline never bounds execution.
- **Disposal margin**: unchanged, 100 ms after Bento terminates JavaScript.
- **Response window**: two seconds, how long Bento waits for the runtime thread to say what it is doing when the execution timeout expires (Node's value).
- **Heap limit**: the most V8 old-generation heap one isolate may use, in the sense of Node's `--max-old-space-size` and `resourceLimits.maxOldGenerationSizeMb`. The young generation and memory outside V8's heap are not included.
- **Heap allowance**: the temporary extension of an isolate's heap limit that lets termination take effect.
- **Ending cause**: why a run ended. The guest ending on its own, a requested shutdown that ends the run (by Bento's default action or at the shutdown deadline), a feature failure (existing), the execution timeout, and the main isolate's heap limit are ending causes. The first one recorded decides the run's outcome.
- **Activity**: what the main thread was doing when the execution timeout expired.
- `ShutdownOutcome::TimedOut` keeps its meaning, incomplete disposal, and never reports an execution timeout. The guides say "timed out" only for the execution timeout and "disposal is incomplete" for `TimedOut`.

### Rust API

```rust
pub struct RuntimeConfig {
    // project_root, cwd, grants, env, argv, host_modules, shutdown_deadline: unchanged
    /// How long the guest may execute, measured from the call to `start`, before the run is
    /// ended with `ErrorKind::ExecutionTimeout`. `None` never ends it.
    pub execution_timeout: Option<Duration>,
    /// V8 heap limits for the instance's isolates.
    pub heap_limits: HeapLimits,
}

/// V8 old-generation heap limits in bytes, in the sense of Node's `--max-old-space-size`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HeapLimits {
    /// The main isolate's limit. `None` keeps the default derived from physical memory.
    pub main: Option<usize>,
    /// The most any worker may have: the limit of a worker that asks for none, and the cap on
    /// one whose `resourceLimits.maxOldGenerationSizeMb` asks for more. `None` leaves each
    /// worker its own request, or V8's default.
    pub worker: Option<usize>,
}

/// The smallest heap limit with which an isolate can start.
pub const MIN_HEAP_LIMIT: usize = /* measured; see Heap limits */;

pub enum ErrorKind {
    Startup, Resolution, Transform, Permission, Uncaught, // unchanged
    /// The run's execution timeout expired while the guest was still executing.
    ExecutionTimeout,
    /// The main isolate reached its heap limit.
    HeapLimit,
}

impl RuntimeError {
    /// What the main thread was doing when the execution timeout expired. `Some` exactly for
    /// `ErrorKind::ExecutionTimeout`.
    pub fn activity(&self) -> Option<&Activity>;
}

/// What the main thread was doing when a run's execution timeout expired.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Activity {
    /// JavaScript was running. Frames are innermost first.
    Executing { stack: Vec<String>, resources: Vec<String> },
    /// No JavaScript was running.
    Waiting { resources: Vec<String> },
    /// The runtime thread did not answer within the response window.
    Unresponsive,
}

impl std::fmt::Display for Activity { /* Node's report lines, below */ }
```

`RuntimeConfig::new` sets `execution_timeout: None` and `heap_limits: HeapLimits::default()`. `RuntimeError` keeps deriving `Clone`, `PartialEq`, and `Eq`. `ErrorKind`'s documentation changes from "which stage of a run an error belongs to" to "what kind of failure ended or prevented a run".

### Execution timeout

- `start` rejects `execution_timeout: Some(Duration::ZERO)` with `ErrorKind::Startup` and the message `execution_timeout must be greater than zero`, and a timeout whose deadline cannot be represented (`Instant::checked_add` fails) with `execution_timeout is too large`. Neither starts a thread.
- The deadline is the instant `start` was called plus the timeout. Module loading and transformation count, as Node's count bootstrap. Execution that ends before the deadline is unaffected, and the deadline does not bound the disposal that follows; the shutdown deadline still does.
- The instance's monitor thread watches the deadline. When it passes, and execution has not ended and no ending cause has been recorded, Bento:
    1. records `ExecutionTimeout` as the ending cause;
    2. captures the activity, as described below;
    3. terminates JavaScript in the main isolate and every worker, and terminates at once any worker started afterwards;
    4. disposes of the instance as it does when a shutdown deadline passes: resources are closed, workers are closed, and the disposal margin applies.
- The guest receives no `SIGTERM`, its `beforeExit` and `exit` listeners do not run, and its `process.exitCode` is ignored.
- A requested shutdown in progress does not suspend the deadline. If the execution deadline passes before the shutdown deadline, the run ends with `ExecutionTimeout`; if the shutdown deadline passes first, it ends as `Interrupted`, as today.
- A responsive run, one whose runtime thread answers the activity request, ends no earlier than its deadline and no more than 500 ms after it. An unresponsive run has its report recorded no more than the response window plus the disposal margin after the deadline.
- The ending is observed like any other: `is_finished()`, `wait()`, and `handle.shutdown()` report it, and a `shutdown()` call made before or after the expiry returns the same report.

### Activity

- At expiry Bento asks the runtime thread what it is doing in two ways at once: with a V8 interrupt, which runs when JavaScript checks for interrupts, and through the instance's command channel, which the event loop receives when it is idle. Whichever runs first captures the activity; the other does nothing.
- From the interrupt, the activity is `Executing`. `stack` holds up to ten innermost frames of the current JavaScript stack, source-mapped to the original sources as uncaught-exception stacks are, as plain text without `at ` and without terminal escapes, for example `spin (file:///work/spin.ts:2:9)`. Runtime-internal frames are included as V8 reports them.
- From the command channel, the activity is `Waiting`.
- `resources` lists, for the main isolate:
    - each pending `setTimeout` as `Timeout` and each `setInterval` as `Interval`;
    - each open resource by its runtime resource name;
    - each pending asynchronous operation as `<operation name> (pending operation)`;
    - each live worker as `Worker (thread <threadId>)`, or `Worker (thread <threadId>, name '<name>')` when it has a name, using the `threadId` and `name` the guest sees.
- Identical lines are collapsed into one with the suffix ` x<count>`, as Node writes `Timeout x2`. Lines are sorted by their text. Unreferenced timers and resources may be listed, because the runtime does not record which ones are referenced.
- If neither request has run when the response window ends, the activity is `Unresponsive`. Bento then terminates and disposes anyway. A runtime thread inside native code notices the termination only when it returns, so disposal usually does not finish within the margin (see [Ending causes and outcomes](#ending-causes-and-outcomes)).
- Workers' own activity is not captured.
- `Activity`'s `Display` writes, with lines separated by `\n` and no trailing newline:
    - `Executing`: `Main thread was executing JavaScript:`, then each frame as `    at <frame>`, then the resource lines.
    - `Waiting`: `Main thread was not executing JavaScript.`, then the resource lines.
    - The resource lines: `No resources keeping the event loop alive were found.` when there are none, and otherwise `Resources keeping the event loop alive:` followed by each line indented by four spaces.
    - `Unresponsive`: `The main thread did not respond within 2000ms. It is likely blocked in a synchronous native operation, such as a blocking host function or a native addon, so no JavaScript stack or resource information is available.`
- The `ExecutionTimeout` error's `message` is `<entry path>: execution timed out after <timeout>`, a newline, and the activity's `Display`. `<timeout>` is the configured `Duration` in Rust's `Debug` format, such as `2s`, `1.5s`, or `500ms`. Its `location()` is the first frame in a project source, chosen as for uncaught exceptions, when the activity is `Executing`, and `None` otherwise.

### Heap limits

- `start` rejects `heap_limits.main` or `heap_limits.worker` below `MIN_HEAP_LIMIT` with `ErrorKind::Startup`, naming the field and the minimum in bytes.
- `MIN_HEAP_LIMIT` is the smallest old-generation limit, rounded up to a whole MiB, with which Bento's startup snapshot deserializes and bootstraps a main isolate and a worker on the pinned V8. It is measured during implementation and recorded as the constant.
- The main isolate is created with old-generation limit `heap_limits.main`, or with the default derived from physical memory when it is `None`. The young generation and code range keep V8's defaults. If Bento cannot apply the configured limit, for example because the upstream runtime discards the isolate parameters it was given, `start` fails with `ErrorKind::Startup` instead of running without the limit.
- A worker's old-generation limit is resolved from the guest's request (`resourceLimits.maxOldGenerationSizeMb` greater than zero) and `heap_limits.worker`:

    | Guest request | `heap_limits.worker` | Worker's limit         |
    | ------------- | -------------------- | ---------------------- |
    | none          | none                 | V8's default           |
    | none          | `cap`                | `cap`                  |
    | `request`     | none                 | `request`              |
    | `request`     | `cap`                | the smaller of the two |

    A resolved request below `MIN_HEAP_LIMIT` is raised to it instead of aborting the process. `worker.resourceLimits.maxOldGenerationSizeMb` reports the resolved limit in whole MiB. Young-generation, code-range, and stack sizes are resolved as today.

- Every isolate, the main one and every worker whether or not it has a configured limit, gets Bento's near-heap-limit handling:
    - On the first call for an isolate: for the main isolate, Bento records `HeapLimit` as the ending cause and terminates the main isolate and every worker; for a worker, it marks the worker out of memory and terminates that worker only.
    - Every call returns the current limit plus 16 MiB, as Node does for workers, as long as the result is at most twice the isolate's initial limit. Beyond that it returns the current limit and V8 aborts the process. This is the residual case of a single operation allocating that much without returning to a point where termination takes effect.
- A `node:worker_threads` worker that reaches its limit ends as it does today with `resourceLimits`: its parent receives an `error` event with code `ERR_WORKER_OUT_OF_MEMORY` and message `Worker terminated due to reaching memory limit: JS heap out of memory`, then an `exit` event with code 1. A Web Worker that reaches its limit is terminated. In both cases the main isolate keeps running, and the run's outcome depends only on what the guest then does.
- When the main isolate reaches its limit, the run ends with `RunOutcome::Failed`, whose error has `kind: ErrorKind::HeapLimit`, the message `<entry path>: JavaScript heap out of memory: the main isolate reached its heap limit of <N> MiB` (`<N>` the limit in effect, rounded down), no location, and no activity. Disposal is as after an execution timeout.
- Heap limits count V8's heap only. `ArrayBuffer`, `Buffer`, and `WebAssembly.Memory` backing stores, native addon memory, host-module and Prompt API allocations, and Bento's own allocations are not counted, and exhausting them can still abort the process or get it killed by the operating system. Each isolate is limited separately, so a guest that starts many workers can use many times the worker limit.

### Ending causes and outcomes

- The first recorded ending cause decides the outcome; later causes change nothing. A guest that calls `process.exit` just before the deadline ends with `Exited`; a heap-limit termination in progress when the deadline passes stays `HeapLimit`.
- A run ended by the execution timeout or by the main isolate's heap limit, whose disposal then finishes, reports `RunOutcome::Failed(error)` from `wait()` and `ShutdownOutcome::Failed(error)` from `shutdown()`, with the same error.
- When disposal does not finish within the bounds after such an ending, the run reports `RunOutcome::Shutdown` with `ShutdownOutcome::TimedOut`, as every run with incomplete disposal does today, and dropping the `RunningRuntime` detaches its thread. The limit that ended it is then not reported.

### CLI: `--process-timeout`

- `bento run --process-timeout=<DURATION> <script>` and `bento run --process-timeout <DURATION> <script>` set the run's execution timeout. Like every `bento` option, it comes before the target; after the target it belongs to the script. Given more than once, the last value applies. `bento run --help` lists it under Options as `Exit with status 124 if the script is still running after DURATION, a positive integer followed by ms, s, m, or h, as in 30s`.
- `<DURATION>` is one or more ASCII digits with a value greater than zero, followed by exactly `ms`, `s`, `m`, or `h`. Leading zeros are allowed. The duration in nanoseconds must fit in a signed 64-bit integer: at most `9223372036854ms`, `9223372036s`, `153722867m`, or `2562047h`.
- Any other value stops `bento` before it runs anything, with status 1 and `error: invalid value for --process-timeout: '<value>'. Expected a positive integer followed by a unit (ms, s, m or h), e.g. 30s`. A missing value stops it with `error: --process-timeout requires an argument`.
- With a task target, `bento` stops before running anything, with status 1 and `` error: --process-timeout applies to scripts only, so it cannot run task `<name>` ``, as `--watch` does, for as long as task targets exist.
- When the instance ends with `ErrorKind::ExecutionTimeout`, directly or as `ShutdownOutcome::Failed` while a signal is stopping it, `bento` writes to stderr `(bento:<pid>) Process timed out after <DURATION> (--process-timeout). Exiting with code 124.`, a newline, and the error's activity, and exits with status 124. `<pid>` is the `bento` process's ID and `<DURATION>` the text given on the command line. The usual `error:` line and code frame are not printed.
- `bento` keeps its own copy of the deadline, measured from when it started the instance, as it does for signals. If the instance has not reported how it ended three seconds after the deadline (the response window, the disposal margin, and slack), `bento` writes the same header and the `Unresponsive` text and exits with 124 without waiting further. If the instance reports incomplete disposal (`RunOutcome::Shutdown` with `ShutdownOutcome::TimedOut`) after the deadline, `bento` does the same.
- While a signal is stopping the script, the signal's deadline and the timeout's backstop both run; whichever passes first ends `bento` with its status, 130 or 143 for the signal and 124 for the timeout. A second signal still ends `bento` at once with 130 or 143.
- Under `--watch`, every run gets the whole timeout from its own start. A run that times out is reported with the header `(bento:<pid>) Process timed out after <DURATION> (--process-timeout).`, without the exit sentence, and its activity, followed by `watch: waiting for changes before restarting`. The next change starts a new run. A run that times out while it is being restarted is reported the same way before its replacement starts. The three-second backstop does not apply under `--watch`; an instance stuck in native code holds the watch as it does today.
- `bento` reads no options from `NODE_OPTIONS`, so `NODE_OPTIONS` cannot set a process timeout. A nested `bento run` has only the timeout its own command line gives, and processes the script started are not stopped when `bento` exits with 124, as in Node.

### CLI: heap limits

- `bento run --max-old-space-size=<MiB> <script>` (also with a space) sets both `heap_limits.main` and `heap_limits.worker` to that many MiB. `<MiB>` is one or more ASCII digits with a value greater than zero; the last value applies. A malformed value stops `bento` with status 1 and `error: invalid value for --max-old-space-size: '<value>'. Expected a positive integer number of MiB`, and a value below `MIN_HEAP_LIMIT` with `error: --max-old-space-size must be at least <minimum> MiB`.
- Without the flag, the instance uses the default limits, and reaching them now ends the run or the worker instead of aborting `bento`.
- When the main isolate reaches its limit, `bento` writes `error: JavaScript heap out of memory: <message>` and exits with status 1. Under `--watch` the failure is reported like any other and the watch continues.

### Exit codes

The exit-code table in [Running with Bento](../guides/running-with-bento.md) gains a row for 124, "`--process-timeout` expired before the script ended; Node and `timeout(1)` use the same status", and the row for 1 adds "ran out of JavaScript heap".

The CLI's generic failure descriptions gain `execution timed out` for `ErrorKind::ExecutionTimeout` and `JavaScript heap out of memory` for `ErrorKind::HeapLimit`.

### Guides

- [Running with Bento](../guides/running-with-bento.md) gains a section on bounding a run's time and memory, covering the flag, both report forms, `--watch`, signals, the exit statuses, and the limits below, and its Limits section names what the flags do not bound.
- [Embedding Bento](../guides/embedding-bento.md) documents the two fields in its construction example and defaults, the two error kinds and `activity()` under observing execution, and rewrites "Limits and native addons" around what heap limits and the execution timeout do and do not cover.
- [Node Compatibility](../guides/node-compatibility.md) lists the differences below.
- All three add `proposal.0009` to `describes`.

### Where Bento differs from Node

- The clock starts when `bento` starts the run's instance, after it has selected the target and loaded the environment, not when the process starts. Library, CLI, and `--watch` share that definition.
- The timeout bounds execution, not teardown. A script whose execution ended before the deadline is reported as it ended even if its disposal finished after the deadline, unless disposal is incomplete or still running at the three-second backstop. Node reports 124 for any process still alive at its deadline.
- The prefix is `(bento:<pid>)`. Resource lines use Bento's runtime resource and operation names rather than libuv handle types, timers carry no "next due in" time, and unreferenced items may appear.
- Under `--watch` the header omits `Exiting with code 124.`, because `bento` keeps running.
- An unresponsive main thread is described in terms of host functions and native addons, and `bento` exits three seconds after the deadline rather than two.
- There is no `--report-on-process-timeout` and no inspector to exclude. `NODE_OPTIONS` is ignored rather than rejected.
- Node aborts with status 134 at its main heap limit; Bento ends the run and exits with 1. Node's `--max-old-space-size` overrides workers' `resourceLimits` in both directions while reporting the requested values; Bento caps larger requests, honors smaller ones, and reports the limit in effect. A worker request too small to bootstrap is raised rather than aborting the process.
- `--process-timeout-worker-stacks` is not supported.
- Option names take dashes only; Node also accepts underscores, as in `--process_timeout`.

## Compatibility

- `RuntimeConfig` gains two public fields. Callers that build it field by field must add them or use `..RuntimeConfig::new(root)`; in this repository that is the crate documentation example, the embedding guide's example, and `crates/bento_runtime/tests/support`. The guest-book host assigns fields after `RuntimeConfig::new` and is unaffected.
- `ErrorKind` gains two variants. An exhaustive `match` must handle them; in this repository that is the CLI's `runtime_report`.
- Without configuration, two behaviors change for the better. A main isolate that reaches its default heap limit ends the run with `ErrorKind::HeapLimit` instead of aborting the process. A worker without `resourceLimits` that reaches V8's default ends with `ERR_WORKER_OUT_OF_MEMORY` instead of aborting the process. A guest whose worker asks for a limit below `MIN_HEAP_LIMIT` gets the minimum instead of aborting the process.
- The CLI has no default timeout and no default heap flag, so existing commands behave as before apart from the heap behaviors above.
- Every crate is `publish = false`; no release coordination is needed.
- In-flight proposals: Proposal 0003 ([PR #259](https://github.com/bento-js/bento/pull/259)) changes `RuntimeConfig::new` to take permissions; the new fields are orthogonal and default the same way. Proposal 0004 ([PR #268](https://github.com/bento-js/bento/pull/268)) removes task targets, which retires the task-target error above. Proposal 0011 ([PR #276](https://github.com/bento-js/bento/pull/276)) keeps `TimedOut`'s meaning, which this proposal relies on; its `wait_async()` reports limit failures as `wait()` does. Proposal 0007 ([PR #277](https://github.com/bento-js/bento/pull/277)) can carry the error kinds and `Activity` in its structured errors. Proposal 0008 ([PR #275](https://github.com/bento-js/bento/pull/275)) already states that these limits apply to inline launches.

## Implications on adoption

No new requirements, version floors, or setup. Both limits are opt-in per instance or per command and are removed by not setting them. The CLI's syntax follows Node 26.13.1, the version Bento's Node alignment targets.

## Scope

- `bento_runtime`: the two `RuntimeConfig` fields, `HeapLimits`, `MIN_HEAP_LIMIT`, `Activity`, the two `ErrorKind` variants and `RuntimeError::activity`; the execution deadline in `Control`'s monitor; activity capture through an interrupt and the command channel; ending-cause recording that generalizes the feature failure; main-isolate creation with a configured limit and its verification; near-heap-limit handling on every isolate; worker limit resolution and the minimum.
- `bento_cli`: `--process-timeout` and `--max-old-space-size` parsing and validation, the timeout report, status 124, the backstop, signal and `--watch` interplay, the task-target error, and the new failure descriptions.
- Runtime and CLI tests for the behavior in Detailed design, side by side with Node 26.13.1 where the behavior is shared.
- The three guides above, and a changeset.

### Out of scope

- Structured error envelopes, standard-I/O capture, and `RuntimeEvent` streams (Proposal 0007, [PR #277](https://github.com/bento-js/bento/pull/277)). This proposal's error kinds and `Activity` are what those would carry.
- Inline evaluation (Proposal 0008, [PR #275](https://github.com/bento-js/bento/pull/275)), async `wait` and `shutdown` (Proposal 0011, [PR #276](https://github.com/bento-js/bento/pull/276)), persistent sessions (Proposal 0012, [PR #271](https://github.com/bento-js/bento/pull/271)), and cancellation inside host functions (Proposal 0013, [PR #273](https://github.com/bento-js/bento/pull/273)).
- Operating-system limits: resident memory, cgroups, `setrlimit`, CPU quotas, process or container sandboxing. Nothing here limits memory outside V8's heap.
- CPU-time budgets. The execution timeout is wall-clock time, as Node's is.
- Workers' activity in the timeout report, `--report-on-process-timeout`, and diagnostic reports in general.
- Limits on young-generation size, code range, stack size, worker count, or the total heap of all isolates.
- Stopping processes the guest started when its run times out.
- Time or memory limits for task commands.

## Preview

- Repository shape: CLI and library.
- Artifact: the installable `bento` executable from the pull request's macOS CI job (`bento-macos27-arm64-prompt`), plus the pull request's documentation preview for the revised guides.
- Access: the CI run's artifacts and the documentation preview under `pr-preview/pr-272/`, both reached from the pull request.
- Reason: the CLI is a host of the same library limits, so running it exercises both, and the executable is the cheapest real artifact. The reviewer runs a busy loop, a pending interval, a listening server, and a blocking `execSync` under `bento run --process-timeout=2s` beside `node --process-timeout=2s`, compares statuses and reports, repeats the server under `--watch`, and runs a growing array under `--max-old-space-size=64` and in a worker to see the run, or only the worker, end without an abort. The embedding guide's example compiles as a documentation test in `mise run rust`.
- Cleanup: the preview workflow removes the documentation preview when the pull request closes, and CI artifacts expire on GitHub's retention schedule.

## Policies and decisions checked

- `policy.0001`, [Prefer VoidZero crates](../policies/0001-prefer-voidzero-crates.md): no parsing, transformation, resolution, source-map, or bundling dependency is added. The design uses V8 and `deno_core` APIs, the engine substrate the policy leaves to Deno.
- `decisions/` holds no decisions yet.
- [VISION.md](../VISION.md), "Not a sandbox merely because it uses isolates": the design claims protection from accidental runaway guest JavaScript only, and Detailed design and the guides list what it does not cover. "Make the runtime a library" and "Embedding is the default": both limits are library configuration, and the CLI is one host of them.
- [ARCHITECTURE.md](../ARCHITECTURE.md), "Isolate memory limits and interrupt hooks are not a complete defense…": consistent; the residual abort and the uncounted memory are stated.
- AGENTS.md, "The upstream Deno crates are pinned as a set": no upstream crate changes. The main isolate's limit uses `deno_lib`'s existing `unconfigured_runtime` parameter, or Bento's own construction of the main worker, never a patch to one crate.

## Future directions

- Workers' stacks and resources in the timeout report, following Node 27's `--process-timeout-worker-stacks` once Node 27 is stable.
- A heap snapshot or diagnostic report written near a limit, like Node's `--heapsnapshot-near-heap-limit` and `--report-on-process-timeout`.
- Typed stack frames and resources in `Activity`, if Proposal 0007's structured errors need more than text.
- Accounting for memory outside V8's heap, such as array-buffer backing stores, through a custom array-buffer allocator with a quota.
- A per-call budget for host functions, which would make blocking host code interruptible.

## Alternatives considered

- **A host timer that calls `shutdown()`**, today's workaround. The guest's `SIGTERM` listener gets another shutdown deadline, the outcome looks like any host stop, there is no report of what was running, and the CLI has no 124. Rejected as the design; it remains available to hosts that want a cooperative stop.
- **Reusing `shutdown_deadline` as the execution budget.** The two begin at different moments, one at `start` and one at a shutdown request, and mean different things: a budget for running and a budget for stopping. Conflating them would make a long-running server's shutdown deadline a lifetime limit.
- **Wrapping `bento` in `timeout(1)` or another supervisor.** It gives 124 but no report, and an embedding has no process to supervise.
- **A process-wide V8 flag such as `--max-old-space-size`.** It applies to every instance in the host process, cannot differ per instance, and on its own still aborts the process at the limit. Node's flag also overrides workers' requests in both directions while misreporting them.
- **Limiting the total heap with `CreateParams::heap_limits` rather than the old generation.** That is V8's own unit, but Node users reason in old-generation MiB, and the worker request being capped is old-generation. Old generation was chosen for parity, with the young generation documented as extra.
- **Separate default and ceiling for workers.** Both answer "the most heap a worker may have", and keeping them apart creates combinations with no use. One cap that also serves as the default was chosen.
- **A new `RunOutcome::LimitExceeded` variant.** It would need a parallel `ShutdownOutcome` variant, and hosts' existing handling of `Failed` would miss it. A limit ending a run is a failure of the run, so it is an `ErrorKind`, with activity attached as diagnostic detail like a source location.
- **Near-heap-limit handling only when a limit is configured**, today's worker behavior. It leaves the default limit aborting the process, the case most hosts hit.
- **Doubling the limit at once**, as `deno_lib`'s callback does. It lets a 512 MiB isolate reach 1 GiB before anything bounds it; Node's 16 MiB steps reach termination with far less overshoot, and the twice-the-limit bound keeps the worst case finite.
- **Node's clock from process start.** For an embedded instance there is no process start, and under `--watch` Node itself measures each run. Starting at the instance gives the library, the CLI, and `--watch` one definition.
- **Two proposals, one per limit.** They share the motivation, the termination and reporting path, and the ending-cause model, and each alone leaves agent runs unbounded in one dimension.

## Open questions

- [NEEDS CLARIFICATION: When a limit ends a run and disposal then does not finish within its bounds, should the run keep today's precedence and report only `ShutdownOutcome::TimedOut`, as Detailed design specifies? That preserves the incomplete-disposal contract that `Drop` and Proposal 0011 rely on, and the CLI still exits with 124. The alternative is a report that also carries the limit that ended the run, which needs a new field on `ShutdownReport` or a new variant.]
- [NEEDS CLARIFICATION: Should `bento run` accept `--max-old-space-size=<MiB>`, Node's own spelling, mapped to both the main and worker limits, as Detailed design specifies? The alternative keeps heap limits library-only for now, and the CLI uses the default limits, which still end runs and workers cleanly.]
- [NEEDS CLARIFICATION: Which status should `bento` exit with when the main isolate reaches its heap limit: 1, as Detailed design specifies because Bento ends the run cleanly and reports it like any other failure, or 134, the status Node's abort produces?]
- [NEEDS CLARIFICATION: Should `--process-timeout-worker-stacks` be adopted now from Node 27.0.0-alpha.0.2.0, or wait, as this draft proposes, until Node 27 is stable and Bento's Node alignment target moves past 26?]
