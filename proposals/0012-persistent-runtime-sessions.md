---
id: proposal.0012
title: Persistent Multi-Turn Runtime Sessions
authors: [Mark]
status: draft
pull-request: https://github.com/bento-js/bento/pull/271
issues: []
supersedes: []
---

# Persistent Multi-Turn Runtime Sessions

## Summary

Add `RuntimeSession` to `bento_runtime`. It is a runtime instance that stays open across host-driven turns, evaluating TypeScript snippets with REPL semantics and calling module exports, while it keeps global state, loaded modules, and background work. A failing turn leaves the session usable. Interruption or exit ends it.

## Motivation

Three kinds of host run many pieces of guest code in one living JavaScript environment:

- **Coding-agent harnesses.** Oh My Pi's `eval` tool keeps a persistent JavaScript kernel, currently a Bun worker. An agent defines a helper in one cell, loads data in the next, and inspects it in a third, without repeating setup. Hive X runs model-written scripts against host tools ([ARCHITECTURE.md](../ARCHITECTURE.md#embedding-contracts)) and needs the same continuity across an agent's steps.
- **Notebooks.** A cell builds on every cell executed before it, and its result is displayed.
- **Long-lived script hosts.** A native application or server loads a plugin module once, then calls its exported functions for each event. The plugin's module state persists between calls.

`bento_runtime` serves none of them. Its only execution API is `start(config, entry)`. That runs one entry file and returns a `RunningRuntime` that ends when the program does: when its event loop drains (`RunOutcome::Completed`), when it exits, when an error goes unhandled, or when the host shuts it down. Each new piece of code therefore needs a new instance, with a new isolate, bootstrap, and module graph, and with empty state. Bento's recorded cold launch of the guest-book fixture takes 301.7 ms from spawn to listening, against Node's 189.4 ms (medians of 15, `bench/results/2026-10-12T00-04-54Z/summary.md`). That figure includes the application's imports. A warm session avoids paying a comparable cost on every agent step. An instance also has no way to receive a value from the host and return one, except through a host module. Any error the guest does not handle ends the instance and everything in it.

The available workaround is a guest program that never finishes. Its entry file loops on `await` of a host-module function that returns the next piece of code, runs it with indirect `eval`, and reports the result through another host-module call. This works only for plain JavaScript: the evaluated text gets no TypeScript transform, no `import` declarations, and no top-level `await`. `let` and `const` declarations from one evaluation are invisible to the next. The loop's own `try`/`catch` is all that keeps a stray error from ending the instance. Proposal 0008 ([PR #275](https://github.com/bento-js/bento/pull/275)) lets a host run in-memory source once, and keeps nothing alive afterwards.

## Domain grounding

### Established context

- ECMAScript evaluates two kinds of source:
    - A **script**'s top-level `var` and function declarations become properties of the global object. Its `let`, `const`, and `class` declarations go into the realm's global declarative environment, which every later script shares. A later script that declares one of those names again throws `SyntaxError` before any of it runs ([GlobalDeclarationInstantiation](https://tc39.es/ecma262/#sec-globaldeclarationinstantiation)). A script has a completion value ([ScriptEvaluation](https://tc39.es/ecma262/#sec-runtime-semantics-scriptevaluation)).
    - A **module** has its own scope, so nothing it declares is visible to later code unless that code imports it. Only a module may use `import` declarations and top-level `await`. Evaluating a module produces a promise, not a completion value. Modules are cached by resolved specifier: importing one again yields the same instance, and a module whose evaluation threw stays errored.
- V8's **REPL mode** changes script evaluation for consoles. Per the [Chrome DevTools Protocol's `Runtime.evaluate`](https://chromedevtools.github.io/devtools-protocol/tot/Runtime/#method-evaluate), it "enables `let` re-declaration and top-level `await`", and "`let` variables can only be re-declared if they originate from `replMode` themselves". The protocol marks the parameter experimental. Chrome's console has relied on it since DevTools 80 ([let and class redeclarations in the Console](https://developer.chrome.com/blog/new-in-devtools-80)).
- Deno 2.9.8's REPL evaluates every line in REPL mode. Observed:
    - A later line can declare a `let`, `const`, or `class` name again. The new declaration replaces the binding in place, so a function defined earlier that reads `t` sees the value of a later `let t = 2`. Instances created before a `class` was redeclared keep the old class.
    - A later line cannot assign to a `const`: it throws `TypeError: Assignment to constant variable.` `let` declarations do not become global object properties, and `var` declarations do. A `var` or function name cannot be redeclared with `let`, and a `let` name cannot be redeclared with `var`: each throws `SyntaxError`.
    - Declaring the same name twice in one line is still a `SyntaxError`.
    - After `let q = f()` throws, reading `q` throws `ReferenceError`, and a later `let q = 1` repairs it.
    - Top-level `await` works, and the line's value is its completion value after the awaits finish. A promise that is itself the completion value is reported as a promise (`Promise { 5 }`), not awaited. A statement's completion value counts: `if (true) { 5 }` gives `5`. Top-level `this` is `globalThis`, `import.meta` is a `SyntaxError`, and `require` is undefined.
    - A thrown error fails only that line, and the next line runs with all earlier state. When a timer the line scheduled throws later, the REPL prints `Uncaught Error: boom` and continues.
- Deno's REPL source ([`cli/tools/repl/session.rs` at v2.9.7](https://github.com/denoland/deno/blob/v2.9.7/cli/tools/repl/session.rs)) reaches REPL mode by opening a local inspector session on its main worker and sending `Runtime.evaluate` with `replMode: true` and `awaitPromise: true`.
    - It first transpiles TypeScript with `deno_ast` and `var_decl_imports: true`, which rewrites `import` declarations into declarations initialized by `await import(…)`. It sets `ImportsNotUsedAsValues::Preserve`, so an import the line does not use is kept for later lines. It prefixes `'use strict'; void 0;`.
    - After posting each evaluation it performs a microtask checkpoint. Its comment explains why: under V8's explicit microtask policy, garbage collection can otherwise collect REPL mode's weakly held result promise and fail the evaluation with "Promise was collected".
    - It polls the worker's event loop whenever it waits for input, so timers and I/O progress between lines. It learns of errors from that work through the inspector's `Runtime.exceptionThrown` notification.
- Node 26.13.1's REPL rewrites source instead of using REPL mode. Observed:
    - Redeclaring a `let`, `const`, or `class` name in a later line throws `SyntaxError: Identifier 'x' has already been declared`.
    - An `import` declaration fails with `Cannot use import statement inside the Node.js REPL, alternatively use dynamic import`.
    - Top-level `await` works.
    - An error thrown from a timer is printed, and input continues.

    Its documentation says its `await` support "will invalidate the lexical scoping of the `const` keywords": after `const m = await …`, `m = 234` succeeds. By default it prints uncaught exceptions and continues; `uncaughtException` listeners and the `handleError` option change that ([Node REPL](https://github.com/nodejs/node/blob/v26.13.1/doc/api/repl.md)).

- Oh My Pi's `eval` JavaScript kernel also rewrites source. Observed on Bun 1.4.4:
    - Top-level `let` and `const` from one cell can be redeclared in the next, because they become global object properties: `typeof globalThis.omp_probe` is `"number"`.
    - A `const` from an earlier cell can be reassigned.
    - Cells may use a top-level `return`.
    - An error thrown by a timer after its cell finished killed the kernel ("JS eval worker exited with code 1"). The next cell ran in a fresh kernel, with the notice "the previous JS worker is dead; state is unavailable".
- V8's `TerminateExecution` throws an uncatchable termination that unwinds every JavaScript frame. `CancelTerminateExecution` lets the embedder run JavaScript in that isolate again (`v8` 150.4.0, `IsolateHandle`). Unwinding skips `finally` blocks. Node 26.13.1's `vm` timeout terminates the isolate and then keeps using it. In an observed run, the `finally` that would have reset `state.inProgress` never ran, and the host continued with `inProgress: true`.

Repository evidence, from `deno_core` 0.412.0 at Bento's pinned fork revision `8252649`, `deno_runtime` 0.267.0, `deno_lib` 0.77.0, and Oxc 0.153.0:

- The documentation of `JsRuntime::execute_script` says it runs a classic script in the global scope, "and it is possible to maintain local JS state and invoke this method multiple times".
- Every `MainWorker` is created with an inspector (`inspector: true` in `deno_runtime/worker.rs`). `LibMainWorker::create_inspector_session` opens a `LocalInspectorSession` on it. `node:inspector`'s `Session.connect` opens separate sessions on the same inspector.
- When guest code reports an uncaught exception, `deno_core::error::dispatch_exception` records it and calls `terminate_execution` to unwind. Converting the recorded exception into a `JsError` cancels the termination and, when asked, clears it, so the runtime can run again. The same function skips termination while the inspector is dispatching a message; its comment reads "This indicates that the fn is being called from a REPL". The exception then reaches only the inspector. Termination is therefore also deno_core's own unwinding mechanism, and does not by itself mean that a host interrupted the isolate.
- `JsRuntime::call_with_args` calls a function and resolves its returned promise while the event loop is polled. `with_event_loop_promise` fails with `PendingPromiseResolution` when the event loop runs out of work before the promise settles. `get_module_namespace` returns a module's exports.
- Oxc 0.153's transformer has no counterpart to `var_decl_imports`.

### Relevant constraints and principles

- [Embedding Bento](../guides/embedding-bento.md) says "a runtime interrupted by the deadline is never restarted in the same isolate" and "do not reuse a timed-out or interrupted instance". The `finally` observation shows why. After termination, guest state can be half-updated in ways no guest code had a chance to repair. A session cannot continue after an interruption.
- The vision asks for "one execution model across development, production, and embedding": "a module should not acquire different semantics merely because it runs inside … an agent harness." Modules a session imports must run exactly as they do in a one-shot run. Only snippet text, which has no counterpart elsewhere, gets REPL semantics.
- The vision also says "engines are implementations, not promises of equivalence." REPL mode is V8's. A JavaScriptCore backend would have to prove it has an equivalent, or replace it.
- [ARCHITECTURE.md](../ARCHITECTURE.md) keeps engine-bound values on their owning thread and has cross-thread callers exchange serializable results. Values therefore cross the session boundary as JSON, as they do for host modules.
- [policy.0001](../policies/0001-prefer-voidzero-crates.md) requires Oxc for parsing and transformation. Snippet compilation and import rewriting cannot use `deno_ast`, which is what Deno's REPL uses for both.
- Bento's existing async shape comes from `bento_prompt::Session`: "State-changing operations apply in the order their methods were called, independently of the order their futures are awaited". Proposal 0011 ([PR #276](https://github.com/bento-js/bento/pull/276)) drafts the same conventions for the runtime: a state change happens when the method is called, the future only observes it, and futures are `Send`, `'static`, independent of any executor, and free of Tokio types.
- Two words are already taken. "Session" names a Prompt API conversation (`bento_prompt::Session`) and an inspector connection. "Turn" also names one run-to-completion step of the event loop.

### Quality bar

- Snippet declarations, redeclarations, top-level `await`, completion values, and error recovery behave as Deno 2.9's REPL behaves for the same lines. The only exceptions are listed under [Where snippets differ](#where-snippets-differ). Tests replay the observations above line by line.
- When a turn throws, rejects, fails to compile, fails to resolve an import, or is denied a permission, the session still has every binding, module, and piece of background work it had before. The next turn runs.
- No turn ever runs in an isolate whose execution was interrupted.
- Once a session is open, a turn creates no isolate and runs no bootstrap. The readiness report gives measured latencies for `open_session` followed by a trivial first turn, for a trivial later turn on the same session, and for `start` of an equivalent file.

### Remaining uncertainty

- `replMode` is experimental in the protocol. Chrome DevTools and Deno's REPL depend on it, but V8 could still change it.
- Snippets evaluate through an inspector dispatch, during which deno_core skips termination for uncaught exceptions and only notifies the inspector. Implementation tests must show that every error raised during a turn reaches Bento in a form it can classify as either the turn's completion or an uncaught error. That includes unhandled rejections detected during the dispatch.
- Whether a local inspector session on the main worker changes anything that a guest's own `node:inspector` session observes is unverified.
- The 301.7 ms figure includes process start and the application's imports. How much of it is isolate creation and bootstrap alone has not been measured.

## Existing baseline

`start` canonicalizes the project root, working directory, and entry. It validates host modules, loads compiler settings into `Sources` (which also holds source maps and the classes of recorded failures), creates `Control` with its shutdown-deadline monitor thread, and spawns the `bento-runtime` thread. That thread's current-thread Tokio runtime and `LocalSet` run `assembly::run`:

1. Create the `LibMainWorker` with Bento's loader, extensions, and permissions.
2. Run `execute_load_phase`, then `run_event_loop_to_completion`, racing shutdown commands.
3. Classify the outcome with `RuntimeError::guest`.
4. Close resources, workers, and Prompt API sessions within the deadline.
5. Record the `ShutdownReport`.

`RuntimeHandle` signals and shuts the instance down from any thread. `Drop` shuts it down and joins the thread.

Worth preserving:

- `RuntimeConfig` as the whole of an instance's authority.
- One `Control` state machine per instance, with its deadline enforcement, first-report-wins retention, and the meaning of `TimedOut`.
- The loader: resolution, the read-permission check for each module, `host:` modules, and watch inputs.
- The compiler settings and source maps in `Sources`, and the `RuntimeError` and `ErrorKind` classification.
- Never reusing an interrupted isolate.
- `bento_prompt`'s async shape.

Incidental for sessions:

- The required entry file, and its path as the prefix of failure messages.
- Ending when the event loop drains, and `run_event_loop_to_completion`'s `beforeExit` loop.
- `RunOutcome`, which names how a program ended.

No Bento API keeps an instance alive after its program ends, evaluates text that is not a file, or calls a function inside an instance from Rust.

## Proposed solution

A host opens a session from the same `RuntimeConfig` that `start` takes. A session is a runtime instance with no entry file, kept open by the host until the host closes it. The host sends it turns. A turn evaluates a TypeScript snippet with V8's REPL semantics, or calls an export of a module. Turns run one at a time in the order they were requested. Each returns its own result or failure, and everything the guest declared, imported, or started stays in place for the next turn.

```rust
use bento_runtime::{EvaluatedValue, Snippet, TurnError, open_session};
use serde_json::json;

let session = open_session(config)?; // returns while the isolate boots
session.evaluate(Snippet::new("import { join } from 'node:path'; let visits: number = 0")).await?;
let visit = session.evaluate(Snippet::new("visits += 1; join('a', String(visits))")).await?;
assert_eq!(visit.value, EvaluatedValue::Json(json!("a/1")));
assert_eq!(visit.display, "'a/1'");

let summary = session.call("./tools.ts", "summarize", vec![json!({ "path": "README.md" })]).await?;

let failed = session.evaluate(Snippet::new("JSON.parse('{')")).await;
assert!(matches!(failed, Err(TurnError::Failed(_)))); // only that turn failed
let visits = session.evaluate(Snippet::new("visits")).await?; // still 1

let report = session.close().await;
```

```text
let count = 0                                   → completes; later turns see count
count += 1; count                               → 1
let count = 10                                  → replaces the binding; later turns see 10
throw new Error("x")                            → this turn fails; the session stays open
setTimeout(() => console.log("tick"), 100); 1   → 1 at once; "tick" prints between turns
setTimeout(() => { throw new Error("y") }, 0)   → the session ends with ShutdownOutcome::Failed
process.exit(3)                                 → the session ends; reset() opens a fresh one
```

## Detailed design

### Concepts

| Concept | What it is | Identity | Begins | Ends |
| --- | --- | --- | --- | --- |
| Runtime instance | One isolate, its thread, its `RuntimeConfig`, and the resources it owns | One `start` or `open_session` call | That call | Disposal |
| Run | An instance executing one entry program (the existing `RunningRuntime`) | Its instance | `start` | The program finishes, or the host shuts it down |
| Runtime session | An instance with no entry file, held open by the host across turns | Its instance | `open_session` | Close, guest exit, an unhandled error, or interruption |
| Turn | One host request to a session: evaluate a snippet, or call an export | Its number within the session | When the turn before it settles | When its own completion settles, or the host abandons it |
| Snippet | Source text that one turn evaluates | Its turn | — | — |
| Background work | Timers, listeners, workers, pending host calls, and promises that are not a running turn's completion | — | When guest code starts it | When it finishes, or the session ends |
| Session scope | The global object, plus the top-level declarations of every snippet so far | Its session | `open_session` | Session end |
| Module cache | The modules loaded in the session | Resolved specifier | First import | Session end |

These distinctions are deliberate:

- **A run and a session are both instances, with different endings.** A run ends when its program ends. A session ends when its host ends it, or when something makes it unusable. They share `RuntimeConfig`, `RuntimeHandle`, `ShutdownReport`, and `RuntimeError`, but neither is a mode of the other. `RunOutcome::Completed`, "the event loop drained", has no meaning for a session.
- **A session turn is not an event-loop turn.** One session turn usually spans many event-loop turns, and background work runs in event-loop turns between session turns. Guides and rustdoc say "session turn" wherever the event loop is also under discussion.
- **A turn owns only its completion.** Work a turn starts and does not await outlives it as background work.
- **Abandoning, cancelling, and interrupting are different.** Abandoning a turn means the host stops waiting for it; the guest is unaffected. Interrupting terminates execution and ends the session. Running JavaScript cannot be cancelled without one of the two.
- **A snippet is neither a module nor an inline source.** A module is a file in the module cache. An inline source (Proposal 0008) is the entry program of a one-shot run, evaluated by Node's `-e` rules. A snippet is a REPL-mode script inside a session.
- **Reset produces a different session.** A host that wants one identity across restarts, like a Jupyter kernel or Oh My Pi's named kernel, owns that identity itself.
- The type is named `RuntimeSession`, and the documentation says "runtime session", so that it is not mistaken for a Prompt API session or an inspector session.

### Opening a session

```rust
pub fn open_session(config: RuntimeConfig) -> Result<RuntimeSession, StartError>;
```

- `open_session` takes the same `RuntimeConfig` as `start`, including the permissions argument that Proposal 0003 ([PR #259](https://github.com/bento-js/bento/pull/259)) adds to `RuntimeConfig::new`. It returns `StartError` in the cases `start` does, apart from the entry path: an unresolvable project root or working directory, invalid host-module specifiers, an unreadable `tsconfig.json`, or a thread that cannot be spawned.
- It returns once the runtime thread is spawned, without waiting for bootstrap, as `start` does. Turns requested before bootstrap finishes wait in the queue. If creating the instance fails, the session ends with `ShutdownOutcome::Failed` carrying an `ErrorKind::Startup` error, and every turn resolves to `TurnError::SessionEnded` with that report.
- The instance is created as `start` creates one, but evaluates no entry. The guest sees the configured environment, permissions, host modules, workers, and Prompt API grant. `process.cwd()` is `config.cwd`. `process.argv` is `[process.execPath, ...config.argv]`, as for Node's REPL and for inline sources.
- `RuntimeSession::diagnostics()` returns the `tsconfig.json` diagnostics, as `RunningRuntime::diagnostics()` does.
- `RuntimeSession::handle()` returns the instance's `RuntimeHandle`. `signal`, `shutdown`, `watch_inputs`, and the async forms that Proposal 0011 adds behave as they do for a run, subject to the rules below.
- `RuntimeSession::is_ended()` reports whether the session has ended, meaning every turn would resolve to `TurnError::SessionEnded`.
- `RuntimeSession` is `Send` and `Sync`, so hosts can share it, for example through `Arc`.

### Turns

```rust
impl RuntimeSession {
    pub fn evaluate(&self, snippet: Snippet)
        -> impl Future<Output = Result<Completion, TurnError>> + Send + 'static + use<>;
    pub fn call(&self, specifier: impl Into<String>, export: impl Into<String>, arguments: Vec<serde_json::Value>)
        -> impl Future<Output = Result<EvaluatedValue, TurnError>> + Send + 'static + use<>;
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum TurnError {
    /// The turn ran, or tried to, and failed. The session is still open.
    #[error("{0}")]
    Failed(RuntimeError),
    /// The session had already ended, or it ended while this turn was queued or running.
    #[error("the runtime session ended: {0:?}")]
    SessionEnded(ShutdownReport),
}
```

- **Queueing.** A turn is queued when its method is called, not when its future is first polled. Turns are numbered from 1, in the order they are queued, across `evaluate` and `call` together and across threads. They run one at a time, in that order. A turn starts once bootstrap has finished and the turn before it has settled or been abandoned. The queue has no length limit, and a turn is never rejected for arriving while another runs.
- **Settling.** A turn settles when its own completion settles. For `evaluate`, that is after the snippet's last top-level `await`. For `call`, it is when the export's return value is available, after awaiting it if it is a promise. A turn never waits for the event loop to drain.
- **Futures.** The futures are observers, as Proposal 0011 drafts them. They are `Send` and `'static`, need no executor or Tokio context, and keep nothing alive: a future can outlive the session and still resolves.
- **Abandoning a turn.** Dropping a turn's future abandons the turn:
    - A turn abandoned before it starts never runs, and keeps its number.
    - A running turn is abandoned at the first moment the runtime thread is not executing its JavaScript, which is its next `await` that does not resolve synchronously. The next turn may start then. The abandoned turn's remaining work continues as background work. Its eventual value or exception is discarded, and never ends the session.
    - Synchronous JavaScript cannot be abandoned. Only interrupting the instance stops it, and that ends the session.
    - Abandoning a turn does not cancel the host-module calls, timers, or other work it started.
- **Unsettleable turns.** If a running turn's completion is pending while nothing in the session is pending that could settle it, the turn fails with `ErrorKind::Uncaught`. Pending things include timers, I/O, workers, and host-module calls that keep the event loop alive. The message is `turn <n> awaits a promise that nothing pending can settle`. Node reports the same condition for a module as "Detected unsettled top-level await". The session stays open. Work that keeps the event loop alive, such as a listening server, keeps the turn waiting, as it would in Node.
- **Failures and partial effects.** A turn's failure is a `RuntimeError` classified as a run's failures are, through `ErrorKind::Transform`, `Resolution`, `Permission`, `Uncaught`, and `Startup`. A failure undoes nothing: statements that ran before it keep their effects.
    - Its message is formatted as a run's guest failure is. In place of the entry path it carries the turn's source name (below) for `evaluate`, and the specifier for `call`.
    - `RuntimeError::location()` finds the first frame in a project source, as it does for a run. A snippet counts as a project source at its source name, so an error thrown by a function that turn 3 defined and turn 7 called is located in turn 3's snippet.

### Evaluating a snippet

```rust
pub struct Snippet {
    pub code: String,
    pub syntax: Syntax, // Proposal 0008's enum: JavaScript, TypeScript, Tsx
}

impl Snippet {
    /// TypeScript source.
    pub fn new(code: impl Into<String>) -> Self;
}

#[derive(Clone, Debug, PartialEq)]
pub struct Completion {
    /// The completion value as JSON, or why it has none.
    pub value: EvaluatedValue, // Proposal 0008's enum: Json(Value) or NotJson { reason }
    /// The completion value as `node:util`'s `inspect` renders it, without color.
    pub display: String,
}
```

**Compilation.** `bento_compiler` compiles each snippet with Oxc, before anything runs:

- The snippet is parsed in its syntax, as module code, so that `import` declarations and top-level `await` parse. A TypeScript or TSX snippet is transformed with the compiler settings that a `.ts` or `.tsx` file in the project root gets, including the JSX and decorator settings. A JavaScript snippet gets no type or JSX transform, as a `.js` file gets none.
- Fails with `ErrorKind::Transform`, located in the snippet:
    - a syntax error;
    - a name declared twice in the snippet;
    - `export` in any form, with a message saying that a snippet is not a module and cannot export;
    - `import.meta`;
    - a top-level `return`.
- Types are removed and JSX is compiled, as for files. `import type` declarations and `type`-qualified specifiers are removed. Every other import is kept even when the snippet does not use it, because later turns may.
- Each import declaration is rewritten into a declaration initialized by an awaited dynamic import, and the rewritten imports are moved to the start of the snippet in source order. A module's imports likewise load and evaluate before its body. The rewritten imports contribute nothing to the completion value.

| Declaration                        | Becomes                                                  |
| ---------------------------------- | -------------------------------------------------------- |
| `import d from "m"`                | `const { default: d } = await import("m")`               |
| `import { a, b as c } from "m"`    | `const { a, b: c } = await import("m")`                  |
| `import * as ns from "m"`          | `const ns = await import("m")`                           |
| `import d, { a } from "m"`         | `const { default: d, a } = await import("m")`            |
| `import "m"`                       | `await import("m")`, with no completion value            |
| `… from "m" with { type: "json" }` | The same, with `import("m", { with: { type: "json" } })` |

- The compiled snippet runs in strict mode, as module code does, under the source name `<cwd>/[turn-<n>]`. Following Proposal 0008's convention for in-memory source, the name is a location in the working directory that no file backs. Stack frames and `RuntimeError::location()` map positions back to the snippet's original text through a source map that the session keeps until it ends. `watch_inputs()` never includes a snippet's source name.
- A snippet needs no read permission. Modules it imports need the same permission they need in a run. Relative specifiers resolve from `config.cwd`. For watch inputs, a snippet's imports count as imports from a project source.

**Running.** The snippet runs as a script in V8's REPL mode:

- Top-level `let`, `const`, and `class` declarations, and the bindings of rewritten imports, live in the session scope.
    - A later snippet may declare any of these names again. That replaces the binding in place, so functions defined by earlier turns see the new value. Objects created before a `class` was redeclared keep the old class.
    - Outside a redeclaration, a `const` cannot be assigned.
    - None of these declarations becomes a property of `globalThis`.
- Top-level `var` and function declarations become properties of `globalThis`, and a later snippet may declare them again with `var` or `function`. A lexical declaration of a `var` or function name from an earlier turn, or a `var` or function declaration of a lexical name from an earlier turn, throws `SyntaxError` before the snippet runs.
- When a snippet fails before its first statement runs, it fails with `ErrorKind::Transform`. This covers Bento's compilation failures and an engine `SyntaxError`, including a conflict with an earlier turn's declarations.
- When a snippet fails after it has started, its declarations that the failure prevented from initializing remain declared but uninitialized. Reading one throws `ReferenceError` until a later snippet declares it again.
- Top-level `this` is `globalThis`.
- The turn settles when the snippet's top-level `await`s have finished.
- Bento reaches REPL mode as Deno's REPL does. The session opens one local inspector session on the main worker's existing inspector and sends each compiled snippet as `Runtime.evaluate` with `replMode` and `awaitPromise` set, with a microtask checkpoint after posting it. Neither the guest nor the host can reach that inspector session, and a guest's own `node:inspector` sessions are separate from it.

**The completion value.** A snippet's completion value is the value of the last statement that produced one, as ECMAScript defines it. `const x = 1` produces `undefined`, and `if (true) { 5 }` produces `5`. A completion value that is itself a promise is not awaited. Once the turn settles, and before the next turn starts, Bento converts the value and keeps no reference to it:

- `value` follows Proposal 0008's rules for `EvaluatedValue`. The value is converted inside the guest with `JSON.stringify` and parsed into `serde_json::Value`.
    - A promise that has already fulfilled is converted as its fulfillment value.
    - It is `NotJson` when `JSON.stringify` returns `undefined` or throws, or when the value is a pending or rejected promise. The reason is `undefined`, `function`, `symbol`, `pending promise`, `rejected promise`, or the message of the thrown error.
    - A JSON conversion failure never fails the turn.
- `display` is `inspect(value)` from `node:util`, with its default options and `colors: false`, as Node's REPL renders values. If inspection throws, for example from a `[util.inspect.custom]` method, the turn fails with that exception as `ErrorKind::Uncaught`.
- Conversion runs guest code, such as `toJSON` methods and custom inspectors. That code runs as part of the turn.

#### Where snippets differ

From a module:

- Top-level declarations persist into later snippets, and later snippets may declare them again.
- `var` and function declarations become global object properties.
- Import bindings are constant snapshots of the exports at import time, not live bindings.
- There is no `export` and no `import.meta`.
- Top-level `this` is `globalThis`.
- A snippet has a completion value.

From Node's REPL:

- `let`, `const`, and `class` can be redeclared, and a `const` stays constant.
- `import` declarations work.
- A snippet is strict.
- There is no `require`, `module`, built-in module globals, `_`, `_error`, or dot command. A host that wants any of these defines them in an early turn.
- A snippet that begins with `{` is a block, as in any script. Node's and Deno's REPLs evaluate it as an object literal, and a host REPL may make that rewrite itself.

### Calling an export

- `call(specifier, export, arguments)` is a turn that behaves as the snippet `const { [export]: f } = await import(specifier); await f(...arguments)` would, with one difference: it compiles no source text.
    - `specifier` resolves as a dynamic import from a snippet does. Relative specifiers resolve from `config.cwd`. Bare package names, `node:` built-ins, and registered `host:` modules resolve as usual.
    - Each argument's JSON text is parsed in the guest with `JSON.parse`, so objects have ordinary prototypes. `f` is called with `this` undefined.
- The module is loaded and evaluated the first time any turn imports it. Later calls and snippet imports share that module instance and its state.
- Loading failures fail the turn with the kind they have in a run: `Resolution`, `Transform`, `Permission`, or a module evaluation's exception. A module whose evaluation threw stays errored in the module cache, as ESM requires. Later imports and calls fail with that same error until the session is reset.
- If `export` does not name a function, the turn fails with `ErrorKind::Uncaught` and a `TypeError` whose message is `<specifier> export <export> is not a function`.
- An exception thrown or a promise rejected by `f` fails the turn, classified as any turn failure is.
- The settled return value is converted to `EvaluatedValue` by the rules for a snippet's `value`, and a conversion failure never fails the turn. `call` produces no `display`.

### Background work

- The session's event loop runs during turns and between them. When the loop goes idle, the session does not end, and `beforeExit` is never emitted while the session is open. Timers fire, listeners accept connections, workers run, and host-module calls settle whether or not a turn is running.
- Guest output from background work goes where all guest output goes, which today is the host process's standard output and standard error.
- An exception or rejection that no guest code handles, and that is not the completion of a running turn, is an **uncaught error**. A promise a turn created and neither awaited nor returned produces an uncaught error if it rejects. An abandoned turn's own completion does not, because it is discarded.
- An uncaught error ends the session with `ShutdownOutcome::Failed(error)`, as it ends a run, and its message is formatted with `config.cwd` in place of the entry path. Guest `process.on('uncaughtException')` and `process.on('unhandledRejection')` listeners handle such errors as they do in Node, and the session then stays open. A host that wants REPL-style resilience installs such listeners in an early turn, and can forward what they catch through a host module. The open question below asks whether this should be the default.

### Turn failures and session ends

| What happens | Result of the affected turn | Session |
| --- | --- | --- |
| The snippet does not compile, or the engine rejects it before it runs | `Failed`, `ErrorKind::Transform` | Open and unchanged |
| An import cannot be resolved or loaded | `Failed`, `Resolution`, `Transform`, `Permission`, or `Startup`, as in a run | Open; declarations not yet initialized stay uninitialized |
| The turn throws, its completion rejects, or a denial reaches it uncaught | `Failed`, `Uncaught` or `Permission` | Open; effects before the failure remain |
| Rendering `display` throws | `Failed`, `Uncaught` | Open |
| The turn awaits a promise that nothing pending can settle | `Failed`, `Uncaught` | Open |
| The host drops the turn's future | None: the turn is abandoned | Open |
| The guest calls `process.exit(code)`, in a turn or in background work | `SessionEnded`, `Graceful { exit_code: Some(code) }` | Ended |
| An uncaught error | `SessionEnded`, `Failed(error)` | Ended |
| Execution is interrupted by the shutdown deadline or by a limit from Proposal 0009, or ends on a feature failure | `SessionEnded`: `Interrupted` for the deadline, `Failed(error)` for a feature failure, or the outcome Proposal 0009 defines | Ended; the isolate is never used again |
| The host closes, resets, or drops the session, calls `handle().shutdown()`, or sends a signal that has no guest listener | `SessionEnded`, with the report that shutdown records | Ended |
| The runtime thread panics | `SessionEnded`, `Failed` with `ErrorKind::Startup` and `runtime thread panicked: …` | Ended |

- Once a session has ended, every queued, running, and later turn resolves to `TurnError::SessionEnded` with a report equal (`==`) to the one `handle().shutdown()` returns. A turn's future resolves no later than when that report is recorded, and never waits for a runtime thread stuck in native code.
- A session ends because of an interruption only when Bento or the host caused it: the shutdown deadline, a feature failure, or a limit. deno_core's own use of termination to unwind an uncaught exception is classified by the exception it carries, as one of the rows above. `Control` records which cause applied, and the session checks that record rather than `is_execution_terminating()`.

### Closing, dropping, and resetting

```rust
impl RuntimeSession {
    pub fn close(self) -> impl Future<Output = ShutdownReport> + Send + 'static + use<>;
    pub fn reset(self)
        -> impl Future<Output = (ShutdownReport, Result<RuntimeSession, StartError>)> + Send + 'static + use<>;
}
```

- `close` requests the instance's bounded shutdown when it is called, exactly as `RuntimeHandle::shutdown` requests it. Its future resolves to the report that `shutdown` returns.
- From the moment shutdown is requested, no further turn starts, and every unsettled turn resolves to `SessionEnded` once the report is recorded. That includes a turn whose completion settles during shutdown.
- The instance then behaves as a run that has been asked to shut down:
    - With a `SIGTERM` listener, the guest keeps running until its event loop drains, it exits, or the deadline interrupts it.
    - Without one, Bento's default shutdown ends it.
    - Disposal, `TimedOut`, and the report's `exit_code` follow the existing rules in [Embedding Bento](../guides/embedding-bento.md).
- Dropping a `RuntimeSession` does what dropping a running `RunningRuntime` does, including any change Proposal 0011 makes to that. Queued turns resolve to `SessionEnded`.
- `reset` requests shutdown as `close` does. Once the report is recorded, it opens a new session from the same `RuntimeConfig`, as `open_session` would, and returns both.
    - The new session shares nothing in JavaScript with the old one: no bindings, modules, or background work. Its turn numbers start at 1. Its permissions are the configured ones, so permissions dropped through `process.permission.drop()` are restored.
    - It has the same host-module registrations, so host services that the handlers capture are shared, as they are between any two instances the host configures alike.
    - After a `TimedOut` report, `reset` still opens the new session. The old runtime thread may still be inside native code and may still hold resources, such as a listening port, and the returned report says so.

### Composition with sibling proposals

- **Proposal 0003, permissions ([PR #259](https://github.com/bento-js/bento/pull/259)).** A session takes the permissions of its `RuntimeConfig`, fixed for its lifetime. They govern every turn and all background work. A restricted session needs read permission for every module it loads, including `call` targets, and none for snippets. `process.permission.drop()` lasts across turns until reset.
- **Proposal 0007, structured observability ([PR #277](https://github.com/bento-js/bento/pull/277)).** Turn failures are `RuntimeError`s, so 0007's structured form applies to them. Output written between turns belongs to the session, not to any turn, so a per-turn capture must say how it attributes background output. This proposal adds no event stream; 0007 may add events for turn start, turn settlement, and session end.
- **Proposal 0008, inline evaluation ([PR #275](https://github.com/bento-js/bento/pull/275)).** This proposal reuses 0008's `Syntax` and `EvaluatedValue`, and its convention of naming in-memory source as an unbacked file in a directory. Whichever proposal is implemented first adds them, and the other reuses them. An inline source follows Node's `-e` rules for a one-shot program, and a snippet follows REPL mode in a session. The difference is intended.
- **Proposal 0009, timeouts and heap limits ([PR #272](https://github.com/bento-js/bento/pull/272)).** Any limit 0009 defines ends a session when it interrupts the isolate, and no turn runs afterwards. If 0009 adds a per-turn limit, its clock runs from the turn's start to its settlement, so neither queue time nor idle time between turns counts against any turn. Abandoning a turn bounds how long the host waits, without ending the session. A limit bounds how long the guest runs, by ending it.
- **Proposal 0011, async lifecycle ([PR #276](https://github.com/bento-js/bento/pull/276)).** Turn, close, and reset futures follow 0011's conventions. `RuntimeHandle` methods that 0011 adds, such as `request_shutdown` and `shutdown_async`, apply to a session unchanged. A session's `Drop` follows whatever 0011 settles for `RunningRuntime`'s `Drop`.
- **Proposal 0013, typed host modules ([PR #273](https://github.com/bento-js/bento/pull/273)).** Host modules, typed or untyped, are available to every turn. Ending the session disposes the instance, which cancels pending host calls. Abandoning a turn cancels none.

### Documentation and example

- `guides/embedding-bento.md` gains a section on keeping a session across turns, written from this proposal. It covers turns and the queue, snippet semantics and their differences, `call`, background work and uncaught errors, the failure table, abandoning versus interrupting, and close and reset. The guide's `describes` lists `proposal.0012`.
- Every new public item has rustdoc stating the rules above. `RuntimeError`'s rustdoc says that it describes the failure of a run or of a turn, and `ErrorKind::Uncaught`'s says "no guest code handled it".
- `crates/bento_runtime/examples/session.rs` is a line-oriented host for previewing. It opens a session on a project directory and treats each line read from standard input as a turn. It accepts `.call <specifier> <export> <json-array>`, `.reset`, and `.close`, and prints each completion's `display` and `value`, or the turn's error.

## Compatibility

No compatibility impact. The change only adds API. `start`, `RunningRuntime`, `RunOutcome`, `RuntimeHandle`, and `RuntimeConfig` keep their behavior. The rustdoc of `RuntimeError` and `ErrorKind::Uncaught` broadens to cover turns without changing their meaning. If Proposal 0008 is implemented first, this proposal reuses its `Syntax` and `EvaluatedValue`; otherwise it adds them, exactly as 0008 drafts them.

## Implications on adoption

No new dependency, Cargo feature, toolchain, or platform requirement. Snippet compilation uses the Oxc crates `bento_compiler` already depends on, and REPL mode uses the inspector that `deno_runtime` already creates for every main worker. Hosts adopt by calling `open_session` and can return to `start` at any time.

Two things a host must know before relying on a session. Under the recommended answer to the first open question, an uncaught background error ends the session unless the guest listens for it. A session also keeps every module it loaded and every snippet's source map until it ends, so its memory grows with its history until the host resets it.

## Scope

- `open_session` and `RuntimeSession`, with `evaluate`, `call`, `close`, `reset`, `handle`, `diagnostics`, and `is_ended`. `Snippet`, `Completion`, and `TurnError`. `Syntax` and `EvaluatedValue`, if Proposal 0008 has not added them.
- Snippet compilation in `bento_compiler` with Oxc: the rejection rules, import preservation, import rewriting and hoisting, and source maps.
- The session's runtime-thread loop: the turn queue, abandoning turns, an idle event loop that does not end the session, uncaught errors, interruption, and close and reset.
- Integration tests in `crates/bento_runtime/tests/` for every rule in Detailed design:
    - the Deno REPL observations replayed as turns;
    - every row of the failure table;
    - abandoning queued and running turns;
    - queue order across threads;
    - `process.exit` in background work;
    - an uncaught background error with and without a guest listener;
    - interrupting a synchronous loop by shutdown deadline, then confirming no further turn runs;
    - `reset`;
    - restricted permissions;
    - a test on a non-Tokio executor.
- Unit tests in `bento_compiler` for the rejections and every row of the import-rewriting table.
- The guide section, the rustdoc, and the preview example.
- The latency measurements the quality bar names, reported in the readiness report.

### Out of scope

- An interactive `bento repl` command, and the CLI's `-e`, `-p`, and `-`. The former is a possible future direction; the latter belong to Proposal 0008.
- Structured events, standard-stream capture, and per-turn output (Proposal 0007).
- Configuring timeouts and heap limits (Proposal 0009).
- Async lifecycle for `RunningRuntime` (Proposal 0011), typed host modules (Proposal 0013), and `node:vfs` (Proposal 0010, [PR #270](https://github.com/bento-js/bento/pull/270)).
- Reloading modules whose files changed while the session is open. A session sees a file as it was first loaded, as Node's and Deno's REPLs do.
- Snapshotting, forking, or serializing a session's state.
- Node REPL conveniences (`require`, built-in module globals, `_`, `_error`, object-literal wrapping), autocompletion, and rich display formats.
- Calling functions that snippets defined, by name, from Rust.
- Engines other than V8.

## Preview

- Repository shape: a Rust library that publishes no package. The CLI does not change.
- Artifact: the runnable example `crates/bento_runtime/examples/session.rs`, built from this pull request's branch on macOS arm64 with `cargo run -p bento_runtime --example session -- <project-dir>`. With it, the pull request's documentation preview of the guide section at `pr-preview/pr-271/`.
- Exercise: in the example, the reviewer:
    - declares and redeclares bindings;
    - imports a project module and `node:path` with `import` declarations, and uses top-level `await`;
    - throws and continues;
    - schedules a timer that prints between turns;
    - calls an export with `.call`;
    - runs `process.exit(3)` to watch the session end, then `.reset` to continue fresh;
    - runs `setTimeout(() => { throw new Error("x") })` to watch an uncaught background error end the session.
- Reason: the crate publishes no prerelease package, and the `bento` archive would not exercise a library API. A host program is the cheapest realistic exercise.
- Access and cleanup: the example stays in the repository as the guide's runnable companion, and `mise run rust` compiles it through `cargo clippy --all-targets`. The documentation preview is private to the pull request, and `.github/workflows/preview.yml` removes it when the pull request closes.

## Policies and decisions checked

- policy.0001 (Prefer VoidZero crates). Snippets are parsed, transformed, and rewritten with Oxc in `bento_compiler`. Deno's REPL does the same work with `deno_ast` and `var_decl_imports`, which is exactly what the policy excludes. The inspector session that reaches REPL mode is engine substrate, not a parsing or transformation role.
- `decisions/` holds no decisions.
- Bento's hard rules in `AGENTS.md`. The pinned Deno crate set does not change: the session uses `deno_core` 0.412.0 APIs that are already present. The toolchain and the macOS 27 build target do not change.
- [VISION.md](../VISION.md) and [ARCHITECTURE.md](../ARCHITECTURE.md). This proposal honors "make the runtime a library", "one execution model" (modules a session loads run as they do in a run; only snippet text gets REPL semantics), "engines are implementations" (REPL mode is named as V8's), and the architecture's embedding contract of "asynchronous evaluation", "structured result exchange", and "scoped scripts used by coding agents".
- [Embedding Bento](../guides/embedding-bento.md)'s rule never to reuse an interrupted isolate: every interruption ends the session.

## Future directions

- An interactive `bento repl` built on sessions, which might adopt Node REPL conveniences that the library leaves out.
- A per-turn `AbortSignal` that abandoning a turn aborts, so guest code can stop the work it started.
- Events pushed to the host for turns and uncaught errors, alongside Proposal 0007's streams.
- Calling functions from the session scope by name, or passing values other than JSON between turns and the host.
- Reloading modules whose files changed, for notebook and development workflows.
- Making reset cheap with a warm spare isolate or a startup snapshot of a prepared session.
- A source-rewriting snippet mode for engines without REPL mode, such as JavaScriptCore, with its semantic differences stated.
- TypeScript declarations for the session scope, alongside Proposal 0013's generated declarations, so an agent can see what earlier turns defined.

## Alternatives considered

- **Evaluate each turn as an ES module with a unique specifier.** Static imports and top-level `await` work natively. But declarations persist only through `globalThis` or exports, a module has no completion value, and each turn leaves a module record in the cache for the rest of the session.
- **Evaluate classic scripts without REPL mode (`execute_script`).** State persists natively, but there is no top-level `await` and no `import`. A redeclaration throws, as Node's REPL shows, and a failed initializer leaves its name unusable for the rest of the session.
- **Rewrite source, as Node's REPL does for `await` and Oh My Pi's kernel does for declarations.** This works on any engine, but it changes what code means. In both, a `const` becomes assignable, and in Oh My Pi's kernel lexical declarations become global properties. Temporal dead zones and hoisting change too. "Compatibility is behavior." It remains the candidate for engines without REPL mode.
- **Report uncaught background errors and keep the session open, as Node's and Deno's REPLs do.** This is the subject of the first open question.
- **Resume an interrupted isolate with `CancelTerminateExecution`, as Node's `vm` timeouts do.** Termination skips `finally` blocks and leaves guest state half-updated, and Bento's documented contract forbids reusing an interrupted isolate.
- **Reject a turn while another is running.** Every host would then build the queue itself. `bento_prompt::Session` queues, and so do these sessions.
- **Keep one session identity across resets, as a Jupyter kernel restart does.** That would add a transition from ended back to open, queue semantics across instances, and a handle whose instance changes underneath it. The host already owns that higher identity.
- **Strip `export` from snippets, as Deno's REPL does.** That silently turns module code into session declarations, and `export default` and `export { … }` would still have no meaning. A clear error costs one retry.
- **Treat a snippet that starts with `{` as an object literal.** This REPL heuristic changes the meaning of JavaScript. A host REPL can apply it before submitting a snippet.
- **Blocking turn methods, like `start` and `wait`.** A queued, abandonable turn is asynchronous by nature, and blocking forms would need a thread per waiter, which is the problem Proposal 0011 describes. A blocking host can drive the futures with any `block_on`.
- **A keep-alive flag on `RunningRuntime`.** This conflates a program's ending with a host's decision, and `RunOutcome::Completed` would mean nothing for a kept-alive run.

## Open questions

- [NEEDS CLARIFICATION: Should an uncaught error from background work end the session, as Node ends a process and as Oh My Pi's kernel ends its worker? That is this draft's recommendation: it adds no API, keeps the execution model one, and guest listeners opt into resilience. Or should the session report the error and stay open, as Node's and Deno's REPLs do? That second option needs a channel to deliver the errors, which Proposal 0007 would own.]
- [NEEDS CLARIFICATION: Is it acceptable to depend on V8's REPL mode, reached through the inspector protocol's experimental `replMode` on the main worker's inspector, as Chrome DevTools and Deno's REPL do? Or should snippet semantics be defined by a source rewrite that any engine could run, accepting the semantic differences that Node's REPL and Oh My Pi's kernel show?]
