---
id: proposal.0011
title: Async Rust Runtime Lifecycle
authors: [Mark]
status: draft
pull-request: https://github.com/bento-js/bento/pull/276
issues: []
supersedes: []
---

# Async Rust Runtime Lifecycle

## Summary

Add executor-independent async lifecycle operations to `bento_runtime`: a non-blocking shutdown request, a future for the shutdown report, and a future for the run's outcome. Async Rust hosts can then await, cancel, and compose an instance's lifecycle without blocking executor threads or resorting to `spawn_blocking`.

## Motivation

Bento's embedding API serves Rust hosts, and many of the hosts Bento is for are asynchronous: agent harnesses that run guest code per task, servers that host an instance per tenant or request, and supervisors that run many instances side by side. Every lifecycle operation `bento_runtime` offers them blocks the calling OS thread:

- `RunningRuntime::wait(self)` joins the runtime thread, for as long as the guest runs.
- `RuntimeHandle::shutdown(&self)` waits on a condition variable for up to the shutdown deadline plus a disposal margin: 5.1 seconds with `RuntimeConfig::new`'s defaults.
- Dropping a running `RunningRuntime` performs the same blocking shutdown and then joins the thread.

On an executor thread, each of these stalls every other task scheduled on that thread; on a current-thread runtime, they stall the whole runtime. Hosts cannot put an instance's ending in a `tokio::select!` beside a request, a timer, or a cancellation, and cannot stop waiting for an instance without stopping the instance.

Today's workaround is a dedicated OS thread per blocking call. The `bento` CLI spawns one thread to wait for each instance and another, in `Instance::shut_down`, solely to call `shutdown()` and discard the report (`crates/bento_cli/src/instance.rs`). The guest-book example host spawns a thread for `wait()`, a signal-forwarding thread for `shutdown()`, and special-cases a timed-out shutdown because, in its own words, "the runtime thread cannot finish and `wait` would block forever; the report is then the outcome" (`examples/guestbook-host/src/lifecycle.rs`). In Tokio, the equivalent adapter is `spawn_blocking`: it occupies a blocking-pool thread for the whole wait, cannot be cancelled by dropping its `JoinHandle`, and after a `TimedOut` shutdown it holds that thread until native code returns, which may be never.

Host-module handlers have only an indirect workaround. A guest that asks its host to stop it reaches a handler on the instance's own runtime thread, where the blocking `shutdown()` prevents the shutdown it requests and returns `TimedOut` (`guides/embedding-bento.md`, "Expose asynchronous Rust services"). The guide's advice is to request shutdown from another host thread, which the handler must arrange itself.

Meanwhile, `bento_prompt` already gives the same hosts an async-first Rust API: `Session::create(...).await`, a `Session::dispose()` future that resolves on any executor, and `Cancellation`. A host using both crates gets an async model API and a blocking runtime lifecycle.

## Domain grounding

### Established context

- Rust futures are driven by polling. [`core::future::Future`](https://doc.rust-lang.org/1.99.0/core/future/trait.Future.html) says that "an implementation of `poll` should strive to return quickly, and should not block", and that "some Futures may represent a value that is being computed in a different task. In this case, the future's underlying computation is simply acting as a conduit for a value being computed by that other task, which will proceed independently of the Future."
- An async caller cancels by dropping a future. Destructors are synchronous: async drop exists only as the unstable `async_drop` feature ([rust-lang/rust#126482](https://github.com/rust-lang/rust/issues/126482)), so a `Drop` implementation that must wait for something can only block.
- Tokio's [task documentation](https://docs.rs/tokio/1.53.2/tokio/task/index.html#blocking-and-yielding) says that a blocking operation in a task "would block the entire thread, preventing other tasks from running", and that this "includes destructors of objects destroyed in async code". [`spawn_blocking`](https://docs.rs/tokio/1.53.2/tokio/task/fn.spawn_blocking.html) tasks "cannot be aborted once they start running", and runtime shutdown "will wait indefinitely" for them.
- [`tokio::select!`](https://docs.rs/tokio/1.53.2/tokio/macro.select.html#cancellation-safety) returns when the first branch completes and drops the rest. Its documentation defines cancellation safety as "what happens when a future is dropped before it completes".
- Established handles separate owning work from awaiting it. Dropping a [`std::thread::JoinHandle`](https://doc.rust-lang.org/1.99.0/std/thread/struct.JoinHandle.html) or a [`tokio::task::JoinHandle`](https://docs.rs/tokio/1.53.2/tokio/task/struct.JoinHandle.html) detaches the work, and awaiting `&mut JoinHandle` is cancel safe. A [`tokio::process::Child`](https://docs.rs/tokio/1.53.2/tokio/process/struct.Child.html) keeps running after its handle is dropped, "unlike the futures paradigm of dropping-implies-cancellation"; `start_kill()` requests termination without waiting, `kill().await` requests and then waits, and `wait()` is cancel safe.
- [`tokio::sync`](https://docs.rs/tokio/1.53.2/tokio/sync/index.html#runtime-compatibility): "All synchronization primitives provided in this module are runtime agnostic. You can freely move them between different instances of the Tokio runtime or even use them from non-Tokio runtimes." A future woken only through its `Waker`, never through a reactor, timer, or spawned task, can be polled by any executor.

### Relevant constraints and principles

- An instance's isolate belongs to its thread. Each instance runs on its own `bento-runtime` OS thread inside a current-thread Tokio runtime and `LocalSet` (`crates/bento_runtime/src/lib.rs`), and [ARCHITECTURE.md](../ARCHITECTURE.md) requires that engine-bound values stay on their owning thread while cross-thread callers use message passing. An async lifecycle is therefore a notification from the runtime thread to the host, not the instance running on the host's executor.
- The shutdown deadline is enforced by a per-instance `bento-shutdown` monitor thread (`crates/bento_runtime/src/lifecycle.rs`), not by whoever waits. The bound on the report holds whatever thread or executor the caller uses.
- No thread can be forced out of native code. `ShutdownOutcome::TimedOut` means "the thread may still be inside native code", so anything that waits for that thread to exit may wait forever. `Drop` detaches the thread in that case; `wait()` joins it anyway.
- A shutdown request is a one-way transition. The first request fixes the deadline, later requests change nothing, and a recorded report is never cleared (`Control::request_shutdown`, `Control::complete`).
- A waiter must register for a wakeup before it checks state, or an update made between the check and the registration is missed. `bento_prompt`'s `Latch` (`crates/bento_prompt/src/latch.rs`) subscribes before checking for exactly this reason.
- Bento's existing async convention, in `bento_prompt`, applies a state change when its method is called and returns a future that only observes it. "State-changing operations apply in the order their methods were called, independently of the order their futures are awaited." `Session::destroy()` requests without waiting; `Session::dispose()` destroys and returns a `Disposal` that "resolves wherever it is awaited, including on another runtime and after the session or operation it came from is gone"; dropping a session cancels it without blocking.
- Disposal drops a handler's pending future (`guides/embedding-bento.md`). The guest's `host:` wrapper awaits `op_bento_host_call` without unreferencing it (`crates/bento_runtime/src/host.mjs`), so a pending host call keeps the guest's event loop alive. The first fact is documented behavior; the second is an inference from `deno_core`'s referenced-op semantics.

### Quality bar

- No poll and no future destructor blocks or waits on the instance. These operations never park an executor thread.
- Dropping a future loses nothing and changes nothing about the instance; awaiting again yields the same value.
- Async and blocking forms report from one state, so they agree about the same instance.
- The bound on the shutdown report holds independently of the caller's executor.
- The public API names no Tokio type and needs no Tokio runtime context; the futures work on Tokio's current-thread and multi-thread runtimes and on executors that are not Tokio.
- Hosts that use only the blocking API see no change.

### Remaining uncertainty

- Whether dropping a still-running `RunningRuntime` should keep blocking, whether the blocking `wait()` should adopt the `TimedOut` rule specified here for `wait_async()`, and which names the async forms take are open questions below.
- The runtime thread publishes its outcome as its last action, after which only the OS thread's own exit remains, so `is_finished()` can report `false` for a moment after `wait_async()` has resolved. This is inference from the design, not yet observed.

## Existing baseline

`start` spawns the `bento-runtime` thread, which builds a current-thread Tokio runtime and runs `assembly::run` on a `LocalSet`. `lifecycle::Control::new` spawns the `bento-shutdown` monitor thread. `Control` holds the instance's state in a `parking_lot::Mutex<State>` beside one `Condvar`, and sends `Command::Shutdown` and `Command::Dispose` to the runtime thread over an unbounded channel.

`Control::request_shutdown` does nothing for an instance that is disposed or already asked; otherwise it sets the deadline, wakes the monitor, and sends `Shutdown`. The runtime thread's `execute` delivers `SIGTERM` to guest listeners or, without one, ends execution through the default shutdown. The monitor waits until the deadline; if the instance has not been disposed by then, it terminates the isolates, sends `Dispose`, waits the 100 ms disposal margin, and records `TimedOut`. `Control::complete` records the report with `get_or_insert`, so the first report wins and is retained for every later `shutdown()` call. `RuntimeHandle::shutdown` is a request followed by a condition-variable wait for the report. `RuntimeHandle::signal` makes the same request when the guest has no listener for the signal.

`RunningRuntime::wait(self)` joins the thread and reports a panic as `RunOutcome::Failed` with `ErrorKind::Startup` and the message `runtime thread panicked: <payload>`. `Drop` calls `shutdown()` and joins unless the report is `TimedOut`, in which case it detaches the thread. The test `repeated_shutdown_and_drop_release_the_same_port` relies on a dropped instance having released its port by the time `drop` returns. `RunOutcome` derives only `Debug`; `ShutdownReport` and `ShutdownOutcome` derive `Clone`, `PartialEq`, and `Eq`.

What to preserve: the single `Control` state machine, the monitor thread's deadline enforcement, the first-report-wins retention, the meaning of `TimedOut`, `Drop`'s disposal guarantee, and every blocking method's behavior. What is incidental: the condition variable as the only way to learn of a report, the join as the only way to learn the outcome, and `RunOutcome`'s missing `Clone`. The thread-per-call adapters in the CLI and the guest-book host are workarounds, not design.

`bento_prompt` is the baseline for the async shape. Its `Disposal` is a clonable observer whose future is `Send`, survives its source, and runs on any executor; its `Latch` is the register-before-check notification built on `tokio::sync::Notify`.

## Proposed solution

Add three operations and one derive. `RuntimeHandle::request_shutdown()` asks for the bounded shutdown and returns at once, like `Child::start_kill` or `Session::destroy`. `RuntimeHandle::shutdown_async()` makes the same request when called and returns a future of the shutdown report, like `Session::dispose`. `RunningRuntime::wait_async()` returns a future of the run's outcome that resolves as soon as the outcome is known, including the moment a shutdown times out, without anything waiting on a thread stuck in native code. `RunOutcome` becomes `Clone`, `PartialEq`, and `Eq`, so any number of waiters can receive it.

The futures are observers. They own nothing, borrow nothing, and run on any executor. Dropping one stops observing and changes nothing else; the `RunningRuntime` value remains the instance's only owner, and its `Drop` behaves exactly as it does today.

```rust
let runtime = bento_runtime::start(config, &entry)?;
let handle = runtime.handle();
let outcome = tokio::select! {
    outcome = runtime.wait_async() => outcome,
    _ = tokio::signal::ctrl_c() => {
        let report = handle.shutdown_async().await;
        eprintln!("shutdown: {:?}", report.outcome);
        // The wait future dropped by `select!` lost nothing; a new one returns the outcome.
        runtime.wait_async().await
    }
};
// The outcome is known, so dropping `runtime` here waits for neither a deadline nor the guest.
```

```text
select! drops a pending wait_async() future     → the instance keeps running; a later wait_async() returns the same outcome
shutdown_async() is called and its future dropped → shutdown proceeds; the report is retained for every later caller
the shutdown deadline expires and disposal stalls → wait_async() resolves with RunOutcome::Shutdown(TimedOut) at once
a host-module handler calls request_shutdown()   → it returns at once; the instance shuts down as if a host thread had asked
```

## Detailed design

### Public surface

```rust
impl RuntimeHandle {
    pub fn request_shutdown(&self);
    pub fn shutdown_async(&self) -> impl Future<Output = ShutdownReport> + Send + 'static + use<>;
}

impl RunningRuntime {
    pub fn wait_async(&self) -> impl Future<Output = RunOutcome> + Send + 'static + use<>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RunOutcome { /* variants unchanged */ }
```

- `RuntimeHandle::signal`, `RuntimeHandle::shutdown`, `RuntimeHandle::watch_inputs`, `RunningRuntime::handle`, `RunningRuntime::diagnostics`, `RunningRuntime::is_finished`, `RunningRuntime::wait`, and `Drop for RunningRuntime` keep their current behavior.
- No new public type, no new dependency, and no Tokio type appears in any public signature.
- Neither future is guaranteed to be `Unpin`. A host that polls one by reference across loop iterations pins it first, with `tokio::pin!`, `std::pin::pin!`, or `Box::pin`.

### Requesting shutdown

- `request_shutdown()` requests the instance's bounded shutdown and returns without waiting. It makes the same request that `shutdown()`, `shutdown_async()`, `Drop`, and `signal()`'s default action make.
- The first request from any of those, on any thread, fixes the shutdown deadline at the moment of that request plus `RuntimeConfig::shutdown_deadline`. Later requests by any of them neither restart nor extend the deadline. A request after the instance has ended has no effect, and the retained report stands.
- `request_shutdown()` never waits: it updates the instance's state and sends a message to its runtime thread. It is safe to call from any thread, from inside any executor, and from a host-module handler running on the instance's own runtime thread.
- What the guest sees does not change: a guest with a `SIGTERM` listener receives `SIGTERM` and may drain until the deadline; a guest without one is ended by Bento's default shutdown.

### Awaiting the shutdown report

- `shutdown_async()` makes the request when it is called, before it returns, not when its future is first polled. The returned future only observes the result.
- The future resolves to the instance's `ShutdownReport`, equal (`==`) to the report `shutdown()` returns to every caller. If the report is already recorded, because the instance ended on its own or an earlier shutdown completed, the first poll returns it.
- The report is recorded no later than the deadline plus the disposal margin (100 ms) after the first request, by the instance's monitor thread. The future resolves at its executor's next poll after that; the caller's executor adds only its own scheduling delay to the bound the blocking `shutdown()` already has.
- Any number of `shutdown_async()` futures and blocking `shutdown()` calls on one instance, from any threads and in any interleaving, all receive equal reports, never deadlock one another, and never extend the deadline.
- The future holds its own reference to the instance's state. It may outlive the `RuntimeHandle` it came from and the `RunningRuntime`, and may move between threads and executors.

### Awaiting the outcome

- `wait_async()` returns a future that resolves to the `RunOutcome` that `wait()` returns for the same instance.
- Normally it resolves once the runtime thread has finished the instance's work and produced its outcome: after the isolate, its resources, and the instance's internal Tokio runtime have been dropped. That is the point at which `wait()`'s join returns, apart from the OS thread's own exit. Listeners are closed by then, so a port the guest held can be bound again.
- Once the instance's report is `ShutdownOutcome::TimedOut`, the future resolves with `RunOutcome::Shutdown` carrying that report, whether or not the runtime thread has returned. That is also the outcome `wait()` returns if the thread later returns. Nothing waits for the stuck thread on the future's behalf.
- A panic on the runtime thread resolves the future with the `RunOutcome::Failed` that `wait()` returns for it: `ErrorKind::Startup`, with the message `runtime thread panicked: <payload>`, or `runtime thread panicked` when the payload carries no text. A `TimedOut` recorded before the panic takes precedence, as the previous rule says.
- Every call, before or after resolution, repeated or concurrent, yields an equal outcome. A later blocking `wait()` on the same instance returns an equal outcome too, except when the runtime thread panics after a `TimedOut` report: `wait()`, which still joins the thread, then reports the panic.
- The future does not borrow the `RunningRuntime`. It can be spawned onto an executor, and it still resolves if the `RunningRuntime` is dropped or consumed by `wait()` first. Dropping a running `RunningRuntime` while a `wait_async()` future is outstanding shuts the instance down as `Drop` always has, and the future resolves with that run's outcome.
- `is_finished()` still reports whether the runtime thread has ended. It can report `false` for a moment after `wait_async()` resolves normally, while the thread exits, and stays `false` after a `TimedOut` resolution for as long as the thread is stuck.

### Cancellation and ownership

The `RunningRuntime` value owns the instance; the futures are conduits for state that the runtime and monitor threads produce.

- Dropping a `wait_async()` future, whether before its first poll, while it is pending, or as a losing `select!` branch, has no effect on the instance, its outcome, or any other future. A later `wait_async()` returns the same outcome.
- Dropping a `shutdown_async()` future does not withdraw the request. Shutdown proceeds and its report is retained for later callers.
- Dropping either future never blocks and never requests shutdown.
- Dropping a `RunningRuntime` behaves as today. If the instance is still running, the dropping thread requests shutdown and blocks until the report exists, which is bounded by the deadline plus the disposal margin, then joins the runtime thread, or detaches it when the report is `TimedOut`. In async code this destructor blocks its executor thread. A host keeps it off executor threads by finishing the instance first: awaiting `shutdown_async()` when it is stopping the instance, then `wait_async()`.
- Once `wait_async()` has resolved for an instance, dropping its `RunningRuntime` waits for neither the shutdown deadline nor the guest. After a normal resolution it joins a thread with no instance work left; after a `TimedOut` resolution it detaches the thread, as today.
- Bento accepts no cancellation token for these operations. Stopping an await is the host's business, through `select!` or its own cancellation, and is distinct from stopping the instance, which only a shutdown request does.

### Executor independence

- Both futures are woken only through the `Waker` passed to `poll`, by the runtime thread or the monitor thread. They use no timer, I/O reactor, task spawning, or runtime handle, and need no Tokio runtime context.
- They work on a Tokio current-thread runtime, a Tokio multi-thread runtime, and an executor that is not Tokio, such as `futures::executor::block_on` or a thread-parking waker, including on a thread that has entered no runtime.
- `poll` takes the instance's internal lock only for constant-time reads and waker registration. No lock is held across a suspension point or while waiting, and a poll never waits for the runtime or monitor thread.
- A future registers its waker before it checks the instance's state, so a report or outcome recorded concurrently with a poll is never missed.
- Both futures are `Send`. `RuntimeHandle` stays `Clone`, `Send`, and `Sync`; `RunningRuntime` keeps its auto traits.

### Inside host-module handlers

A handler runs on its instance's runtime thread, and the instance's disposal drops its pending future. A handler can reach a `RuntimeHandle` only if the host shares one after `start`, for example through an `Arc<OnceLock<RuntimeHandle>>` captured by the handler closure; this proposal does not change that.

| Call inside a handler of instance A | Behavior |
| --- | --- |
| A's `request_shutdown()` | Supported and recommended. Returns at once; A shuts down as if a host thread had asked. Whether A's guest observes the handler's result before disposal is unspecified. |
| A's `shutdown_async().await` | Requests shutdown when called. The future cannot resolve inside the handler: disposal drops the handler's future, cancelling the await, before the report exists, so code after the await never runs and the guest's call never settles. While the call is pending it keeps A's event loop alive, so if A's `SIGTERM` listener waits for the event loop to drain, the shutdown ends at the deadline as `ShutdownOutcome::Interrupted`; a listener that calls `process.exit` is unaffected. No thread blocks. |
| A's `wait_async().await`, if the host made A's `RunningRuntime` reachable | The pending call keeps A's event loop alive, so A cannot end by draining it. A ends when its guest exits explicitly or something requests shutdown; its disposal then drops the handler's future. No thread blocks. |
| A's blocking `shutdown()` | Unchanged: blocks A's runtime thread, prevents the shutdown it requests, and returns `TimedOut` after the deadline plus the margin. |
| Another instance B's `shutdown_async().await` or `wait_async().await` | Supported. A's runtime thread keeps running A's guest while the handler waits; the pending call keeps A's event loop alive until it settles. |

Bento does not detect a handler awaiting its own instance's futures.

### Documentation and existing hosts

- Each new method has rustdoc stating the rules above, with a compiled example for `wait_async()` and `shutdown_async()`. `RunningRuntime`'s rustdoc no longer says `wait` is the only way to read the outcome, and the rustdoc of `wait()` and `shutdown()` points async callers to their async forms.
- `guides/embedding-bento.md` gains a section on controlling an instance from async Rust: the `select!` pattern, the observer semantics of dropped futures, the `TimedOut` difference between `wait_async()` and `wait()`, the blocking `Drop` in async code, and the handler table. Its handler advice changes from requesting shutdown on another host thread to calling `request_shutdown()`.
- `bento_cli`'s `Instance::shut_down` calls `request_shutdown()` instead of spawning a thread that calls `shutdown()` and discards the report. Its behavior is unchanged. The rest of the CLI and the guest-book example host stay synchronous.

## Compatibility

The change is additive. Every existing method keeps its behavior, so hosts that use only the blocking API are unaffected. `RunOutcome` gains `Clone`, `PartialEq`, and `Eq`; adding trait implementations breaks no existing caller. The `bento_cli` change replaces a thread whose result was discarded with the request that thread made; the CLI's observable behavior does not change. Bento publishes no crates (`publish = false`), so there is no semver release to coordinate.

Proposal 0003 (PR #259) changes `RuntimeConfig::new`'s parameters; the two changes touch different parts of the API, and examples written for this one use whichever constructor `main` has at implementation time. Outcome or report variants that Proposal 0009 (PR #272) may add reach the async forms unchanged, because they return exactly what the blocking forms return. The handler rules above rely on disposal dropping pending handler futures; Proposal 0013 (PR #273), which covers cancellation propagation into typed handlers, must keep that true or revise those rules.

## Implications on adoption

No new dependency, Cargo feature, toolchain, or platform requirement. Async hosts adopt by calling the new methods; nothing else changes for them. A Tokio host that aborts a task owning a running `RunningRuntime` still runs the blocking `Drop` on that executor thread, as it would today, until the first open question below is decided. Adoption is reversible: the operations are additive, and a host can return to the blocking forms at any time.

## Scope

- `RuntimeHandle::request_shutdown`, `RuntimeHandle::shutdown_async`, and `RunningRuntime::wait_async`, as specified.
- `Clone`, `PartialEq`, and `Eq` for `RunOutcome`.
- Publishing the runtime thread's outcome, including the panic case, so that `wait()` and `wait_async()` agree.
- Rustdoc for the new and affected methods, and the `guides/embedding-bento.md` section described above.
- `bento_cli`'s `Instance::shut_down` using `request_shutdown()`.
- The preview example described below.
- Integration tests in `crates/bento_runtime/tests/` for the rules in Detailed design: mixed concurrent blocking and async shutdown callers, dropped futures, the `TimedOut` resolution with a stuck handler, a non-Tokio executor, a current-thread runtime that keeps running other tasks during a shutdown, port release after `wait_async()`, and the handler table's `request_shutdown()` and `shutdown_async()` rows.

### Out of scope

- Changing `Drop` for a running instance, and changing when the blocking `wait()` returns after `TimedOut`. Both are open questions; until they are settled, both keep today's behavior.
- An async `start`. Startup canonicalizes three paths, reads the project's compiler configuration, and spawns two threads; it is bounded work that does not wait on the guest.
- Observing the outcome through `RuntimeHandle`. The outcome stays with the instance's owner; the owner can hand a `wait_async()` future to anyone.
- Cancellation-token parameters for lifecycle operations, since `select!` and the host's own cancellation already compose with the futures.
- Moving the CLI or the guest-book example host onto an async executor.
- Detecting a blocking `shutdown()` called on the instance's own runtime thread.
- Execution timeouts and V8 heap limits (Proposal 0009), structured errors, stdio, and `RuntimeEvent` streams (Proposal 0007), inline evaluation (Proposal 0008), persistent multi-turn sessions (Proposal 0012), and typed host modules with cancellation propagated into handlers (Proposal 0013).

## Preview

- Repository shape: a Rust library that publishes no package, embedded by the CLI and by example hosts.
- Artifact: a runnable Cargo example, `crates/bento_runtime/examples/async_lifecycle.rs`, built from this pull request's branch on macOS arm64, together with the pull request's documentation preview of the updated guide at `pr-preview/pr-276/`.
- Exercise: `cargo run -p bento_runtime --example async_lifecycle -- --guest <graceful|ignores-sigterm|blocks-native>`. The example writes a small guest HTTP server into a temporary directory and runs it on a Tokio current-thread runtime beside a heartbeat task that prints every 100 ms. It selects on `wait_async()` and Ctrl-C; on Ctrl-C it awaits `shutdown_async()`, prints the report, awaits `wait_async()`, and prints the outcome. `graceful` drains through a `SIGTERM` listener, `ignores-sigterm` ends `Interrupted`, and `blocks-native` blocks its runtime thread in a host-module handler so that the shutdown ends `TimedOut` and `wait_async()` resolves at once. The heartbeat continuing throughout shows that no executor thread blocked.
- Reason: the crate is not published, so there is no prerelease package to install, and the CLI's observable behavior does not change, so the installable `bento` archive would not exercise the new API. A real async host program is the cheapest realistic exercise.
- Access and cleanup: the example lives in the repository and stays as the guide's runnable companion, compiled by `mise run rust` through `cargo clippy --all-targets`. The documentation preview is private to the pull request and is removed by `.github/workflows/preview.yml` when the pull request closes.

## Policies and decisions checked

- policy.0001 (Prefer VoidZero crates) — The change adds no dependency and touches no parsing, transformation, source-map, resolution, or bundling code. Notification uses `tokio::sync` and `parking_lot`, which `bento_runtime` already depends on.
- `decisions/` holds no recorded decisions.
- Bento's hard rules in `AGENTS.md` — the pinned Deno crate set, the Rust toolchain pin, and the macOS 27 default build target are untouched.
- [VISION.md](../VISION.md)'s principle that public capabilities have "explicit ownership and lifecycles", and the runtime lifecycle section of [ARCHITECTURE.md](../ARCHITECTURE.md), which anticipates `runtime.shutdown().await` and message passing to engine-bound threads, are honored: the instance's owner and its observers are distinct, and the futures are fed by messages from the instance's threads.

## Future directions

- An outcome observer on `RuntimeHandle`, for hosts where several components each need to learn how an instance ended without the owner distributing futures.
- An async `start`, if startup grows work that can wait on something other than the local filesystem.
- A non-blocking destructor once Rust stabilizes async drop, or an explicit method that hands an instance's remaining disposal to its monitor thread.
- Accepting `bento_prompt::Cancellation` for lifecycle operations, if hosts come to share one cancellation type across Bento's crates.
- Async streams for the runtime events Proposal 0007 defines.

## Alternatives considered

- **`IntoFuture` for `RunningRuntime`, or a consuming `wait_async(self)`.** Awaiting the instance itself reads well, but the future would own the instance: dropping it in a losing `select!` branch would drop the `RunningRuntime`, running the blocking shutdown on the executor thread and losing the outcome. It conflates stopping the await with stopping the instance, which is exactly the distinction a cancellable API needs.
- **A borrowing `wait_async(&mut self)`, as `tokio::process::Child::wait` does.** Cancel safe, but the borrow ties the future to its owner: it cannot be spawned, and the owner cannot call `handle()` or `diagnostics()` while it is pending. `bento_prompt`'s `Disposal` already established the observer that outlives its source as Bento's shape.
- **Requesting shutdown on the future's first poll, as Tokio's `async fn kill` does.** That follows the "futures are inert" convention, but it contradicts `bento_prompt`, where state changes happen when the method is called, and it makes a created-but-unpolled shutdown future silently request nothing. Requesting at call time makes every drop of the future irrelevant to the instance, and `request_shutdown()` covers the fire-and-forget case by name.
- **Documenting `spawn_blocking` wrappers instead.** This works only on Tokio. Each wait holds a blocking-pool thread, a dropped `JoinHandle` leaves the blocking call running, and after `TimedOut` the thread is held until native code returns, which may be never. It moves the problem into every host.
- **Running the instance on the caller's executor.** The isolate is bound to its thread and needs a `LocalSet`; driving it from a host's work-stealing executor would break that, and it would make the deadline depend on the host's executor rather than Bento's monitor thread. It is a much larger change for no lifecycle benefit.
- **Returning Tokio channel types**, such as a `watch::Receiver<Option<ShutdownReport>>`. This ties the public API to Tokio's types and versions, leaks the internal notification design, and leaves each host to re-derive the `TimedOut` rule.
- **Detecting a handler awaiting its own instance and panicking.** The request half of the call is legitimate, the outcome is already bounded by the deadline, and a thread-identity check would catch only one form of a mistake that `request_shutdown()` and the guide already address.

## Open questions

- [NEEDS CLARIFICATION: Should dropping a `RunningRuntime` that is still running keep blocking until disposal completes, which is today's contract and what `repeated_shutdown_and_drop_release_the_same_port` relies on, or should it request shutdown and return at once, leaving the monitor thread to finish disposal, so that an async host aborting a task that owns an instance never blocks an executor thread? This draft keeps the blocking `Drop` and documents how async hosts avoid it.]
- [NEEDS CLARIFICATION: Should the blocking `wait()` also return `RunOutcome::Shutdown` with `TimedOut` as soon as that report is recorded, detaching the stuck thread as `Drop` does, instead of joining it? That would make the two waits agree and remove the guest-book host's special case, but it changes documented behavior of the blocking API. This draft leaves `wait()` unchanged.]
- [NEEDS CLARIFICATION: Keep the additive names `wait_async` and `shutdown_async` beside the blocking `wait` and `shutdown`, or, since no crate is published, give the async forms the primary names and rename the blocking forms `wait_blocking` and `shutdown_blocking`, migrating the CLI, the guest-book example host, and the tests?]
