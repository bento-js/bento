---
id: proposal.0013
title: Self-Describing Typed Host Modules
authors: [Mark]
status: draft
pull-request: https://github.com/bento-js/bento/pull/273
issues: []
supersedes: []
---

# Self-Describing Typed Host Modules

## Summary

Make `HostModule` self-describing: schema-described host functions validated at the guest boundary, TypeScript declarations and a JSON manifest generated from the same descriptors, cooperative cancellation from `AbortSignal` and instance disposal, and backpressured async-iterable stream functions. Untyped registration keeps working.

## Motivation

A Rust host exposes its own services to guest code as `host:` modules. Two kinds of host depend on that surface. Coding-agent harnesses such as Hive X expose approved agent tools as host functions ([ARCHITECTURE.md](../ARCHITECTURE.md#embedding-contracts)), so a model writes TypeScript against them instead of issuing one tool call per turn. Native applications expose a scripting API to the scripts they run. Both need the guest, its author, and a type checker to know exactly what each function accepts and returns, and both need long or incremental work to stop when the guest no longer wants it.

`bento_runtime::HostModule` cannot express any of that. A host function is a closure from a JSON array of positional arguments to a future of one JSON value or a `HostError`. Bento's own worked example shows the consequences:

- **The contract lives in three places that drift.** `examples/guestbook-host/src/module.rs` unpacks `arguments[0]` by hand and maps a `serde_json` failure to a `TypeError`-named `HostError`. A separate hand-written file, `fixtures/remix-guest-book/app/data/host-guestbook.d.ts`, declares the TypeScript. They already disagree: `append` is declared `Promise<void>` but resolves to `null`, and `list` is declared with no parameters but silently accepts any. An agent harness registering dozens of tools, often from schemas that arrive at run time, cannot hand-maintain declarations at all.
- **Every handler reimplements input checking, inconsistently.** Missing fields surface raw `serde_json` messages, extra arguments are ignored, and the guest receives an `Error` whose `name` merely says `TypeError` rather than an actual `TypeError`.
- **Handlers cannot observe cancellation.** A pending handler future is dropped when the instance is disposed, but work it handed to a blocking pool or a remote service keeps running. Guest code has no way to abandon a call: there is no `AbortSignal` parameter, and a timed-out agent script leaves its tool calls running.
- **Results are all-or-nothing.** Log tails, search matches, database cursors, and tool progress must be buffered in full before the promise resolves, or exposed through hand-rolled cursor functions.

Every embedding host currently works around this by writing its own validation, its own declaration generator, its own cancel-by-request-id functions, and its own paging protocol. The work is identical across hosts, and none of it can see the runtime's isolate lifecycle, which is what correct cancellation needs.

## Domain grounding

### Established context

- **Code Mode and programmatic tool calling.** [Cloudflare's Code Mode](https://blog.cloudflare.com/code-mode/) converts tool schemas into a TypeScript API, places the generated declarations in the model's context, and runs model-written code in an isolate whose only authority is bindings to those tools. [Anthropic's programmatic tool calling](https://platform.claude.com/docs/en/agents-and-tools/tool-use/programmatic-tool-calling) has the model write code that calls tools inside a sandbox, keeping intermediate results out of the model's context. Both report that models handle larger tool surfaces as typed code APIs than as individual tool calls. Both generate the TypeScript from the tool schemas rather than maintaining it separately.
- **One schema per operation.** For machine consumers, one authoritative description per operation should drive validation, generated interfaces, and discovery metadata; an advertised parameter that does not behave as described is a defect. Discovery should be progressive: list what exists, then inspect one operation in full.
- **JSON Schema 2020-12 is the shared dialect.** The [Model Context Protocol](https://modelcontextprotocol.io/specification/2025-11-25/schema) describes tool `inputSchema` and `outputSchema` in JSON Schema and defaults to [Draft 2020-12](https://json-schema.org/draft/2020-12/json-schema-core) since its 2025-11-25 revision. [`schemars` 1.x](https://docs.rs/schemars) derives Draft 2020-12 schemas from Rust types, placing shared and recursive types under `$defs` and referring to them with local `$ref` pointers. In 2020-12, keywords beside `$ref` apply alongside the referenced schema, `prefixItems` describes positional elements, and `items` covers only the elements after them.
- **JSON Schema does not translate losslessly to TypeScript.** JSON Schema constrains values; TypeScript describes structural types. Structural keywords (`type`, `properties`, `required`, `additionalProperties`, `items`, `prefixItems`, `enum`, `const`, `anyOf`, `oneOf`, `allOf`, local `$ref`) have TypeScript counterparts. Value constraints (`minimum`, `maxLength`, `pattern`, `uniqueItems`) do not, and must be enforced at run time. Conditional and negative keywords (`if`/`then`/`else`, `not`, `dependentSchemas`, `patternProperties`, `unevaluatedProperties`) have no sound TypeScript form; generators that emit `unknown` or `any` for them advertise a contract the runtime does not enforce.
- **JSON crosses the boundary as ECMAScript values.** `JSON.stringify` calls `toJSON`, drops `undefined`, functions, and symbols inside objects, writes `NaN` and `±Infinity` as `null`, and throws on `BigInt` and cycles ([ECMA-262](https://tc39.es/ecma262/#sec-json.stringify)). Every JSON number becomes an IEEE-754 double in the guest, so integers beyond ±(2⁵³−1) lose precision.
- **Leaving `for await` closes the iterator.** When a `for await` body exits by `break`, `return`, or `throw`, ECMA-262's `AsyncIteratorClose` calls and awaits the iterator's `return()` method. A rejection from `next()` itself propagates without calling `return()` ([ECMA-262](https://tc39.es/ecma262/2026/multipage/control-abstraction-objects.html)).
- **Backpressure needs a producer that actually waits.** The [Streams Standard](https://streams.spec.whatwg.org/)'s `highWaterMark` and `desiredSize` are advisory; a producer that ignores them buffers without bound. Across a native boundary, pulling one item per request from a bounded channel makes the producer wait when the consumer does.
- **`AbortSignal` carries an arbitrary reason.** Aborting runs the signal's abort algorithms synchronously; `signal.reason` is any JavaScript value, defaulting to an `AbortError` `DOMException`. A promise abandoned by its signal is conventionally rejected with that exact reason.

### Relevant constraints and principles

- **The host decides what a guest can reach** ([VISION.md](../VISION.md)). Registering a module is the host granting those functions to one instance; nothing about a module may become visible to a guest unless the host chose it.
- **Public interfaces stay engine-independent** ([ARCHITECTURE.md](../ARCHITECTURE.md)). Host-facing types may not expose `deno_core` or V8 types. ARCHITECTURE.md assigns cancellation to `bento_host`, the crate with no engine or toolchain dependency.
- **Repository: `bento_prompt` already solved the hard parts once** ([Proposal 0002](0002-web-prompt-api.md)). `bento_prompt::Cancellation` (`crates/bento_prompt/src/cancel.rs`) is a cloneable token with parent/child derivation on `tokio::sync::Notify`. Its Deno adapter keeps a cancellation handle per call in the calling isolate's resource table, subscribes to the guest's `AbortSignal` through the signal's internal abort algorithm rather than its `abort` event, rejects with the signal's reason, and cancels every handle when the isolate's resources close. Its streams return a resource id, pull one chunk per op call from a bounded channel, and cancel the producer when the resource closes. Its schema subset (`crates/bento_prompt/src/schema.rs`) rejects unsupported keywords instead of discarding them, and its JavaScript side snapshots guest values with `JSON.stringify`/`JSON.parse` before any op sees them, so guest getters and proxies never run inside native code.
- **Repository: host modules are shared across an instance's isolates.** `crates/bento_runtime/src/workers.rs` installs the same `HostModules` in every Web Worker and `node:worker_threads` worker. Per-call state must live in the calling isolate's own resource table, so one isolate cannot reach another's calls.
- **Repository: responses travel as JSON text.** `host_ops.rs` serializes the response and `host.mjs` parses it, because `serde_v8` creates null-prototype objects and `JSON.parse` creates ordinary ones.
- **Repository: the pinned `deno_core` can stop a promise from keeping the event loop alive.** `core.unrefOpPromise` exists in the pinned fork and is used by `deno_web`'s `BroadcastChannel`.

### Quality bar

- Runtime validation, generated declarations, and the manifest are projections of one descriptor per function; none can say something the others contradict.
- Every schema construct Bento accepts is both enforced at run time and represented in the declarations, as a type or, for value constraints, as documentation. Everything else is refused when the host registers it.
- Generated declarations type-check under `tsc --strict` and `tsrs` for every accepted schema.
- A guest passing a wrong value gets a real `TypeError` naming the module, function, location, and mismatch, and the handler never runs.
- An abandoned call, an abandoned stream, a terminated worker, and a disposed instance all cancel the host work they started; a slow guest stalls a stream's producer instead of growing a buffer.
- Hosts that register untyped functions see no change.

### Remaining uncertainty

- `schemars` 1.x output for every serde shape (adjacently and internally tagged enums, `#[serde(flatten)]`, recursive types) has not been checked against the subset below. Implementation must test representative types and extend the subset, through this proposal, if a common shape falls outside it.
- Whether the guest's TypeScript configuration provides `AbortSignal` and `AsyncIterableIterator` depends on its `lib` and `types` settings. The repository's Prompt API example (`examples/prompt-api/tsconfig.json`) uses `ES2024`, `DOM`, `DOM.AsyncIterable`, and `@types/node`, which provide both.

## Existing baseline

`crates/bento_runtime/src/host.rs` defines `HostError { name, message }`, `HostModule`, and the internal `HostModules`. `HostModule::new(specifier)` holds a `BTreeMap` from export name to `Arc<dyn Fn(Value) -> Pin<Box<dyn Future<Output = Result<Value, HostError>> + Send>>>`. `start` rejects a specifier that does not start with `host:` or names nothing after it, and a duplicate specifier, as `ErrorKind::Startup`. Export names are not checked; the generated ES module exports each function under a quoted string name. `HostModules::source` emits that module; `loader.rs` resolves a `host:` specifier only when the instance registered it and otherwise fails with `ErrorKind::Resolution`, never falling back to packages or the network. `host_ops.rs`'s `op_bento_host_call` takes the arguments through `serde_v8`, awaits the handler, and returns `{ value, error }` as JSON text; `host.mjs` turns an `error` into an `Error` whose `name` is the host's.

`examples/guestbook-host/src/module.rs` is the only real host module. `list` ignores its arguments and returns `Vec<Entry>`; `append` deserializes `arguments[0]` into `NewEntry` and returns `Value::Null`. The fixture's hand-written `host-guestbook.d.ts` declares both, with the drift described above.

Worth keeping:

- Instance-local registration, resolution without fallback, and specifier uniqueness.
- The same modules in every isolate of an instance, with handlers capturing `Arc`-owned host services that outlive the instance.
- `HostError` rejections whose `name` the guest branches on.
- JSON text responses, so guest objects have ordinary prototypes.
- `bento_prompt`'s cancellation handles, abort subscription, pull-based streams, and strict schema subset, as the repository's proven patterns.

Incidental, and replaced for schema-described functions:

- Positional argument arrays with no declared shape.
- Hand-written declarations and no machine-readable description.
- Handler futures dropped at disposal with no signal to the work they started, and no guest-side cancellation.
- Single-value results only.

## Proposed solution

A host function can now carry a description and JSON Schemas. Typed Rust registration derives them from `serde` + `schemars` types; dynamic registration takes them as JSON for tools whose schemas arrive at run time, such as a bridged MCP server. From those descriptors Bento validates every guest call before the handler runs, generates `declare module "host:…"` TypeScript declarations and a versioned JSON manifest without starting an isolate, hands each handler a `Cancellation` that fires when the guest aborts, leaves a stream, or the isolate goes away, and lets a function yield a backpressured stream of values the guest reads with `for await`.

```rust
use bento_runtime::{Cancellation, HostModule, HostStreamSender, schemars::JsonSchema};

/// Input for appending a new guest-book entry.
#[derive(Deserialize, JsonSchema)]
struct AppendInput {
    /// Display name of the signer.
    name: String,
    /// Guest-book message body.
    message: String,
}

/// One saved entry.
#[derive(Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct GuestBookEntry {
    id: i64,
    name: String,
    message: String,
    /// RFC 3339 UTC timestamp.
    created_at: String,
}

/// Which entries to watch for.
#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct WatchInput {
    /// Only entries with a greater id are yielded.
    after_id: i64,
}

let module = HostModule::new("host:guestbook")
    .description("Guest-book storage owned by the Rust embedding host.")
    .typed_function("list", "List every saved entry, oldest first.", {
        let store = store.clone();
        move |cancellation: Cancellation, (): ()| {
            let store = store.clone();
            async move { store.list(cancellation).await.map_err(host_error) }
        }
    })
    .typed_function("append", "Append a new entry to the guest book.", {
        let store = store.clone();
        move |cancellation: Cancellation, input: AppendInput| {
            let store = store.clone();
            async move { store.append(input, cancellation).await.map_err(host_error) }
        }
    })
    .typed_stream("watch", "Yield entries as they are appended.", {
        let store = store.clone();
        move |cancellation: Cancellation, input: WatchInput, entries: HostStreamSender<GuestBookEntry>| {
            let store = store.clone();
            async move { store.watch(input.after_id, entries, cancellation).await.map_err(host_error) }
        }
    });

let declarations: String = module.typescript_declarations()?;
let manifest = module.manifest()?;
```

`module.typescript_declarations()` produces these declarations; [Declarations](#declarations) fixes their content and order, and the generator's own layout may wrap lines differently:

```ts
/** Guest-book storage owned by the Rust embedding host. */
declare module "host:guestbook" {
    /** Options every schema-described host function accepts. */
    export interface HostCallOptions {
        signal?: AbortSignal;
    }

    /** Input for appending a new guest-book entry. */
    export interface AppendInput {
        /** Display name of the signer. */
        name: string;
        /** Guest-book message body. */
        message: string;
    }

    /** One saved entry. */
    export interface GuestBookEntry {
        id: number;
        name: string;
        message: string;
        /** RFC 3339 UTC timestamp. */
        createdAt: string;
    }

    /** Which entries to watch for. */
    export interface WatchInput {
        /** Only entries with a greater id are yielded. */
        afterId: number;
    }

    /** Append a new entry to the guest book. */
    export function append(input: AppendInput, options?: HostCallOptions): Promise<void>;

    /** List every saved entry, oldest first. */
    export function list(options?: HostCallOptions): Promise<GuestBookEntry[]>;

    /** Yield entries as they are appended. */
    export function watch(
        input: WatchInput,
        options?: HostCallOptions,
    ): AsyncIterableIterator<GuestBookEntry>;
}
```

```text
await append({ name: "Ada", message: "Hello from 1843" })     → resolves undefined
await append({ name: 42, message: "x" })                      → TypeError: host:guestbook.append: invalid input at /name: expected string, got number; the handler never runs
await list({ signal: AbortSignal.timeout(50) }) on a slow store → rejects with the signal's TimeoutError at 50 ms; the store's Cancellation fires
for await (const e of watch({ afterId: 0 })) { …; break; }    → the producer's Cancellation fires and its next send returns StreamClosed
```

## Detailed design

### Concepts

- A **host module** is one `host:` specifier registered for one runtime instance, with an optional description and a set of named host functions. Its identity is its specifier, unique within the instance.
- A **host function** is one named export of a host module. It has two independent properties:
    - its **result shape**: an **async function** returns a `Promise` of one value; a **stream function** returns an async iterator of values;
    - its **contract**: a **schema-described** function has a description and JSON Schemas for its input and its output or items; an **untyped** function has neither.

    Untyped functions are always async functions. A stream function is always schema-described, because its item schema and its cancellation are what make it a stream.

- A **descriptor** is the description and schemas of one schema-described function. Validation, declarations, and the manifest are all derived from it; none is stored or edited separately.
- A **call** is one invocation of a schema-described host function by one isolate. Each call has its own `Cancellation`, which its handler receives.
- A **stream** is the iterator one call to a stream function returns, together with the producer feeding it. It belongs to the isolate that made the call.

Registration remains the host's grant: a host function's own policy, such as the guest book's read-only mode, stays in its handler and rejects with `HostError`.

### Registration

`HostModule` gains these methods. All are builders that return `Self` and never panic; problems with what was registered are reported when the module is used, as [Registration errors](#registration-errors) describes. Registering a name that already exists replaces the earlier function, as `function` does today.

```rust
impl HostModule {
    /// The module's description, emitted as documentation and in the manifest.
    pub fn description(self, description: impl Into<String>) -> Self;

    /// An async function whose schemas are derived from `Input` and `Output`.
    pub fn typed_function<Input, Output, F, Fut>(
        self,
        name: impl Into<String>,
        description: impl Into<String>,
        handler: F,
    ) -> Self
    where
        Input: DeserializeOwned + JsonSchema + 'static,
        Output: Serialize + JsonSchema + 'static,
        F: Fn(Cancellation, Input) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Output, HostError>> + Send + 'static;

    /// A stream function whose schemas are derived from `Input` and `Item`.
    pub fn typed_stream<Input, Item, F, Fut>(
        self,
        name: impl Into<String>,
        description: impl Into<String>,
        handler: F,
    ) -> Self
    where
        Input: DeserializeOwned + JsonSchema + 'static,
        Item: Serialize + JsonSchema + 'static,
        F: Fn(Cancellation, Input, HostStreamSender<Item>) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<(), HostError>> + Send + 'static;

    /// An async function described by JSON Schemas supplied at run time.
    pub fn schema_function<F, Fut>(self, spec: HostFunctionSpec, handler: F) -> Self
    where
        F: Fn(Cancellation, Value) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Value, HostError>> + Send + 'static;

    /// A stream function described by JSON Schemas supplied at run time.
    pub fn schema_stream<F, Fut>(self, spec: HostStreamSpec, handler: F) -> Self
    where
        F: Fn(Cancellation, Value, HostStreamSender<Value>) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<(), HostError>> + Send + 'static;
}

pub struct HostFunctionSpec {
    pub name: String,
    pub description: String,
    /// `None`: the function takes no input.
    pub input_schema: Option<Value>,
    /// `None`: the function returns `Promise<void>`.
    pub output_schema: Option<Value>,
}

pub struct HostStreamSpec {
    pub name: String,
    pub description: String,
    /// `None`: the function takes no input.
    pub input_schema: Option<Value>,
    pub item_schema: Value,
}
```

- `typed_function` and `typed_stream` generate each schema with `schemars`' Draft 2020-12 settings and then behave exactly like `schema_function` and `schema_stream` with those schemas, deserializing the validated input into `Input` and serializing `Output` or each `Item`.
- When `Input` is `()`, the function takes no input: its descriptor has no input schema. When `Output` is `()`, the function returns `Promise<void>`: its descriptor has no output schema.
- A `schema_function` or `schema_stream` handler for a function without input receives `Value::Null`. For a `schema_function` without an output schema, the guest receives `undefined` whatever value the handler returns.
- A schema of `true` accepts any JSON value and is declared as `unknown`. A host that wants cancellation without describing its input uses `input_schema: Some(json!(true))`.
- `HostModule::function` is unchanged: untyped, positional, without a `Cancellation`.
- `bento_runtime` depends on `schemars` 1.x unconditionally and re-exports it as `bento_runtime::schemars`, so a host derives `JsonSchema` from the version the bounds name.

### Registration errors

- A schema-described function's name must be an ASCII identifier (`[A-Za-z_$][A-Za-z0-9_$]*`) that is not an ECMAScript reserved word, and its description must not be empty.
- Every schema must lie within [the supported subset](#the-schema-subset).
- `start` checks every module and fails with `ErrorKind::Startup` on the first problem, naming the specifier, the function, and, for a schema, the JSON Pointer and keyword.
- `HostModule::typescript_declarations`, `HostModule::manifest`, `host_typescript_declarations`, and `host_manifest` perform the same checks without an isolate and return `HostModuleError` naming the same things. They also fail for an untyped function whose name is not such an identifier, because no TypeScript declaration can name it; `start` and the runtime still accept such untyped functions as they do today.
- Module specifiers keep today's rule. Generated declarations write a specifier as an escaped string literal, so any specifier is representable.

### The shared `Cancellation`

`bento_prompt::Cancellation` and its latch move to `bento_host::Cancellation`. `bento_prompt` and `bento_runtime` re-export it, so one type serves both. `bento_host` gains a dependency on `tokio`'s synchronization primitives and nothing engine-related.

- `Cancellation::new()` is an uncancelled root.
- `child()` becomes public: cancelling a parent cancels the child; cancelling a child leaves the parent and siblings alone.
- `cancel()` is idempotent; `is_cancelled()` never blocks; `cancelled()` resolves once the token or any ancestor is cancelled.

### Calling a schema-described function

The generated module exports each schema-described function so that:

- A function **with input** takes `(input, options?)`; a function **without input** takes `(options?)`. Arguments are strictly positional: when a function has input, its first argument is always the input, so `f(undefined, { signal })` passes options alone.
- The input is **optional** when its schema, after resolving a root `$ref`, has `"type": "object"` and no required properties. An omitted or `undefined` optional input is `{}`. Omitting a required input fails with `input is required`.
- `options` may be `undefined`, `null`, or an object. Its `signal`, when present and not `undefined`, must be an `AbortSignal`. Other keys are ignored, as WebIDL dictionaries ignore unknown members.
- More arguments than the function takes is an error naming both counts.
- The input is converted with `JSON.stringify` semantics and parsed back before any op sees it. A value `JSON.stringify` cannot encode (a `BigInt`, a cycle) is an error. Because `JSON.stringify` would silently turn them into `null`, a non-finite number anywhere in the input is also an error.

**Errors and their timing.** Every error above, and every [validation failure](#validation), is a `TypeError` whose message begins with `<specifier>.<name>: `. An async function never throws: it returns a promise rejected with the error. A stream function performs these checks and validation synchronously and throws, so `try` around a `for await` catches both kinds of failure identically. An already-aborted `signal` likewise rejects the async function's promise, or is thrown by the stream function, with `signal.reason`, and the handler never runs.

### The schema subset

Bento accepts one closed subset of JSON Schema Draft 2020-12, defined once in Rust and used for validation and for declarations. It covers the output of `schemars` for ordinary serde types and the tool schemas of common agent ecosystems.

| Keywords | Validated as | Declared as |
| --- | --- | --- |
| `$schema` (absent or the Draft 2020-12 URI), `title`, `description`, `default`, `examples`, `deprecated`, `format`, `x-*` | Annotations; never change what is valid. `format` is not validated. | `description` becomes the documentation text; `deprecated: true` adds `@deprecated`; `default` adds `@default <compact JSON>`. A root object schema's `title` names its interface. `examples`, `format`, and `x-*` appear only in the manifest. |
| `type`: a name or an array of names among `null`, `boolean`, `object`, `array`, `number`, `integer`, `string` | JSON type; `integer` is any finite number with no fractional part | `null`, `boolean`, `number`, `string`, an object type, an array type; an array of names is a union |
| `enum`, `const`, with string, number, boolean, or null values | Equality with JSON semantics | Literal type, or union of literal types |
| `properties`, `required`, `additionalProperties` (`false`, `true`, or a schema) | Required keys present; each declared property valid; with `false`, no other keys; with a schema, every other key's value valid against it | Interface or object type with `name: T` or `name?: T`. Keys that are not identifiers are quoted. `true` adds `[key: string]: unknown`. A schema `S` adds `[key: string]: S \| P₁ \| … \| Pₙ`, where `Pᵢ` are the declared property types, because TypeScript requires declared properties to fit the index signature. Absent adds nothing. |
| `items` (a schema or `false`), `prefixItems`, `minItems`, `maxItems`, `uniqueItems` | Positional elements against `prefixItems`, the rest against `items` (`false` forbids any), length bounds, distinct elements by JSON equality | `T[]`; with `prefixItems`, a tuple `[A, B, ...R[]]` whose rest is `items`, omitted for `false`, `unknown` when absent; elements at or beyond `minItems` are optional (`[A, B?]`) |
| `anyOf`, `oneOf`, `allOf` | At least one, exactly one, every subschema | `A \| B`, `A \| B`, `A & B` |
| `$defs`, `$ref` to `#` or `#/$defs/<name>` | The referenced schema, recursion included. Beside `$ref`, only annotations are accepted. | A named type per `$defs` entry, as [Declarations](#declarations) describes |
| `minimum`, `maximum`, `exclusiveMinimum`, `exclusiveMaximum`, `minLength`, `maxLength`, `pattern` | Bounds on numbers; lengths in Unicode code points; `pattern` unanchored, compiled by the Rust `regex` crate | The base type, plus `@minimum`, `@exclusiveMinimum`, `@maximum`, `@exclusiveMaximum`, `@minLength`, `@maxLength`, `@pattern`, and for arrays `@minItems`, `@maxItems`, `@uniqueItems`, in that order, after the description |
| Boolean schemas `true`, `false` | Anything; nothing | `unknown`; `never` |

Everything else is refused at registration with the keyword and its JSON Pointer, including: `$ref` to anything but `#` or `#/$defs/…`; `$id`, `$anchor`, `$dynamicRef`, `$dynamicAnchor`, `$vocabulary`; `if`, `then`, `else`, `not`, `dependentRequired`, `dependentSchemas`, `patternProperties`, `propertyNames`, `unevaluatedProperties`, `unevaluatedItems`, `contains`, `minContains`, `maxContains`, `multipleOf`, `minProperties`, `maxProperties`; Draft 7's `definitions` and `additionalItems`; any other `$schema`; object or array values in `enum` or `const`; a constraint keyword that applies to none of the types the schema allows, such as `minLength` on a schema whose `type` is `integer`; a `pattern` the `regex` crate cannot compile, such as one with look-around; and any unknown keyword not prefixed `x-`. Bento never fetches a schema.

As in JSON Schema, a constraint keyword applies only to values of the types it constrains: `{ "type": ["integer", "null"], "minimum": 0 }`, which `schemars` emits for `Option<u8>`, accepts `null`.

Integers beyond ±(2⁵³−1) lose precision in the guest whatever the schema says. The guides tell hosts to send such values as strings.

### Validation

- Before a schema-described handler runs, Bento validates the snapshotted input against the input schema on the calling isolate's thread. On failure the handler never runs.
- A failure is a `TypeError` with the message `<specifier>.<name>: invalid input at <pointer>: <reason>`, where `<pointer>` is the RFC 6901 JSON Pointer of the offending value. A failure of the input as a whole omits `at <pointer>`. `<reason>` names the violated keyword and the expected and actual values or types, for example `expected string, got number`, `missing required property "message"`, `unexpected property "nmae"`, `must be at most 100`. When an input violates several constraints, the message reports one of them, the same one each time for the same schema and input.
- After validation, a typed function deserializes the input into `Input`. A deserialization failure is a `TypeError` with the same prefix and serde's message.
- A typed function's output is serialized from `Output` and not validated: its schema was derived from that type. When serialization fails, for instance for a map with non-string keys, the call rejects with an `Error` named `HostContractError` naming the function and serde's message. Whether `schema_function` and `schema_stream` validate what their handlers return is an [open question](#open-questions).
- A handler's `HostError` rejects the call with an `Error` carrying its `name` and `message`, exactly as today.

### Declarations

`HostModule::typescript_declarations(&self) -> Result<String, HostModuleError>` returns one `declare module` block; `bento_runtime::host_typescript_declarations(&[HostModule]) -> Result<String, HostModuleError>` returns every module's block, ordered by specifier and separated by a blank line, and fails on a duplicate specifier. Neither starts an isolate.

- The output is a pure function of the registered descriptors: the same modules produce byte-identical text.
- A block contains, in order: the module description as a `/** … */` comment above `declare module`; `HostCallOptions`, when the module has a schema-described function; the named types, sorted by name; then the functions, sorted by name, each with its description as documentation.
- A schema-described async function is declared `name(input: I, options?: HostCallOptions): Promise<O>`, with `input?:` when the input is optional, without `input` when there is none, and `Promise<void>` without an output schema. A stream function returns `AsyncIterableIterator<T>`. An untyped function is declared `name(...args: unknown[]): Promise<unknown>`.
- Named types come from `$defs` entries and from root object schemas with a `title`. A root object schema's `title` must be an identifier to name an interface; otherwise the object is declared inline. A `$defs` key's non-identifier characters become `_`. An object schema becomes an `interface`; any other schema becomes a `type` alias. A root referenced through `$ref: "#"` must have such a title.
- Types are scoped to their module, so two modules may each have an `Entry`. Within one module, a name whose schemas are equal once `$schema`, `title`, and `$defs` are set aside is declared once; the same name with different schemas, or a name equal to `HostCallOptions`, fails with `HostModuleError`.
- Text from schemas is escaped: `*/` in documentation becomes `*\/`, and specifiers and quoted keys are written as JSON string literals.
- The declarations assume the guest's TypeScript provides `AbortSignal` and `AsyncIterableIterator`, as a configuration with `ES2018` or later plus `DOM` or `@types/node` does. The guides say so.

### Manifest

`HostModule::manifest(&self) -> Result<HostModuleManifest, HostModuleError>` and `bento_runtime::host_manifest(&[HostModule]) -> Result<HostManifest, HostModuleError>` describe the same modules as data. Both types implement `Serialize`, `Deserialize`, `Clone`, `Debug`, and `PartialEq`, and serialize as:

```json
{
    "manifestVersion": 1,
    "modules": [
        {
            "specifier": "host:guestbook",
            "description": "Guest-book storage owned by the Rust embedding host.",
            "functions": [
                {
                    "name": "append",
                    "kind": "async",
                    "contract": "schema",
                    "description": "Append a new entry to the guest book.",
                    "inputSchema": {
                        "$schema": "https://json-schema.org/draft/2020-12/schema",
                        "title": "AppendInput",
                        "description": "Input for appending a new guest-book entry.",
                        "type": "object",
                        "properties": {
                            "name": {
                                "type": "string",
                                "description": "Display name of the signer."
                            },
                            "message": {
                                "type": "string",
                                "description": "Guest-book message body."
                            }
                        },
                        "required": ["name", "message"]
                    },
                    "outputSchema": null
                },
                {
                    "name": "watch",
                    "kind": "stream",
                    "contract": "schema",
                    "description": "Yield entries as they are appended.",
                    "inputSchema": { "…": "…" },
                    "itemSchema": { "…": "…" }
                }
            ]
        }
    ]
}
```

- `manifestVersion` is `1`; only `host_manifest` emits it, wrapping each `HostModuleManifest`.
- Modules are ordered by specifier and functions by name. `description` is `null` when the module has none.
- `kind` is `"async"` or `"stream"`; `contract` is `"schema"` or `"untyped"`.
- A schema-described async function has `inputSchema` and `outputSchema`, each `null` when absent. A stream function has `inputSchema` and `itemSchema`. An untyped function has `"description": null` and no schema fields.
- Schemas appear exactly as registered or as `schemars` generated them, annotations and `x-*` included, so a harness can hand them to a model or an MCP client unchanged.
- Nothing exposes the manifest or the declarations to guest code; whether a host should be able to opt in is an [open question](#open-questions).

### Cancellation

Each call to a schema-described function creates a `Cancellation` for its handler. It is cancelled, if the call has not settled, when any of these happens:

- **The guest aborts.** When `options.signal` aborts, the call's promise rejects at once with `signal.reason` (a pending stream read rejects the same way, as [Streams](#streams) describes), and the handler's `Cancellation` fires. Bento subscribes to the signal's abort algorithm, not its `abort` event, so a guest-dispatched `abort` event does nothing. A settled call unsubscribes, so a long-lived signal accumulates nothing.
- **The guest leaves a stream**, as [Streams](#streams) describes.
- **The isolate that made the call is disposed.** For the main isolate this is the end of the instance's execution: completion, `process.exit`, an uncaught failure, the default shutdown when no `SIGTERM` listener exists, or interruption at the deadline. A requested shutdown whose `SIGTERM` listener is draining does not cancel calls; the guest may still need its host functions to drain. For a worker it is the worker's termination.

After an abort, the handler's future keeps being polled until it returns or its isolate is disposed, and its result is discarded. An aborted call no longer keeps the instance's event loop alive, so a guest that aborts a call to a handler that ignores cancellation can still finish.

At disposal, Bento cancels every outstanding call and stream before dropping their handler futures and producers, as today's disposal drops handler futures. It does not wait for host work: work a handler handed to another thread or service observes the `Cancellation` it was given. Shutdown reports and their deadlines are unchanged.

Untyped functions receive no `Cancellation` and keep today's behavior.

### Streams

A stream handler receives a `HostStreamSender<Item>`:

```rust
impl<T: Serialize> HostStreamSender<T> {
    /// Waits while 16 sent items are unread, then queues `item` for the guest.
    pub async fn send(&self, item: T) -> Result<(), StreamClosed>;

    /// Whether the guest has stopped reading.
    pub fn is_closed(&self) -> bool;
}
```

- The producer starts when the stream function is called and runs concurrently with the guest. At most 16 items wait unread; `send` waits for room, so a guest that stops reading stops the producer. The capacity is fixed.
- The returned iterator's `[Symbol.asyncIterator]()` returns the iterator itself, so one call is read once.
- `next()` resolves `{ value, done: false }` for each item in the order it was sent. When the handler returns `Ok(())`, `next()` resolves `{ value: undefined, done: true }` once the sent items are read. When it returns `Err(HostError)`, the sent items are still delivered, then one `next()` rejects with an `Error` carrying the `HostError`'s `name` and `message`. Concurrent `next()` calls settle in the order they were made. An item that fails to serialize ends the stream with a `HostContractError`.
- `return()`, which `for await` calls on `break`, `return`, or `throw`, closes the stream: the producer's `Cancellation` fires, unread items are discarded, every later `send` returns `Err(StreamClosed)`, and `return()` resolves `{ value: undefined, done: true }`. Closing twice, or after the stream finished, does nothing.
- When `options.signal` aborts, the stream closes as by `return()` and a pending `next()` rejects with `signal.reason`.
- Once a stream has finished, failed, or closed, every later `next()` resolves `{ value: undefined, done: true }`.
- A stream the guest abandons without closing keeps its producer, held by the bounded buffer, until its signal aborts or its isolate is disposed. The guides recommend `for await` or an explicit `return()`.
- A panic in a stream producer fails the instance as a panic in a handler does today.

### Workers

Every isolate of an instance can import the instance's host modules, as today. Calls, cancellation handles, and streams live in the calling isolate's resource table. One isolate cannot read, close, or cancel another's, and a worker's termination cancels only its own.

### Examples and guides

- `examples/guestbook-host` registers `host:guestbook` with `typed_function`, removing its hand-written argument parsing; `append` resolves `undefined`.
- `fixtures/remix-guest-book/app/data/host-guestbook.d.ts` becomes the output of `guest_book_module(...).typescript_declarations()`, and a test in `examples/guestbook-host` fails when the checked-in file differs from that output.
- `guides/embedding-bento.md` and `guides/guest-book-embedding-example.md` are rewritten from this proposal: typed and dynamic registration, the schema subset, validation errors, declarations and their TypeScript requirements, the manifest, cancellation, streams, and large integers.

## Compatibility

- Existing hosts compile and behave as before. `HostModule::new`, `HostModule::function`, `HostError`, and untyped calls are unchanged.
- `bento_prompt::Cancellation` remains available under that path, as a re-export; `child()` becomes public.
- `bento_runtime`'s public API names `schemars::JsonSchema`, so a host deriving it must use `schemars` 1.x, which `bento_runtime::schemars` re-exports. A future `schemars` major version is a breaking change for Bento's API.
- `fixtures/remix-guest-book/app/data/host-store.ts` discards `append`'s result, so `append` resolving `undefined` instead of `null` changes nothing for it.
- Proposal 0003 (in flight, PR #259) changes how `RuntimeConfig` is constructed. This proposal changes only `host_modules`, so the two compose in either order.
- The `bento` CLI registers no host modules and is unaffected. Nothing stored changes.

## Implications on adoption

Adopting schema-described functions requires `schemars` 1.x for typed registration; dynamic registration needs nothing new. The runtime adds `schemars` and a direct dependency on `regex`, which the workspace already builds. Guests type-checking against generated declarations need `AbortSignal` and `AsyncIterableIterator` in their TypeScript libraries. Everything is additive, so a release containing it can be reverted without migration.

## Scope

- `bento_host::Cancellation`, moved from `bento_prompt` and re-exported by both runtime crates.
- `typed_function`, `typed_stream`, `schema_function`, `schema_stream`, `description`, `HostFunctionSpec`, `HostStreamSpec`, `HostStreamSender`, `StreamClosed`, and `HostModuleError`.
- The schema subset, its validator, and its declaration lowering, defined once.
- `typescript_declarations`, `host_typescript_declarations`, `manifest`, `host_manifest`, and the manifest types.
- The guest calling convention, validation errors, cancellation, and streams in the main isolate and in workers.
- Tests for each behavior above, including `tsc --strict` and `tsrs` over generated declarations for every row of the subset table, and `schemars` output for representative serde types.
- The guest-book migration, the drift test, and the two guides.

### Out of scope

- A CLI discovery registry or `bento schema` command (proposal 0005).
- Changing `HostError`'s shape, structured error codes, stdio capture, and runtime event streams (proposal 0007).
- Inline evaluation (proposal 0008); execution timeouts and heap limits (proposal 0009); `node:vfs` (proposal 0010); an asynchronous `RunningRuntime` lifecycle (proposal 0011); persistent multi-turn sessions (proposal 0012).
- C and Swift bindings for host modules (`bento_ffi`).
- Binary payloads. Host functions still exchange JSON values.
- Synchronous host functions.
- JSON Schema beyond the subset, and fetching remote schemas.
- Typed functions with several positional parameters; see [Alternatives considered](#alternatives-considered).

## Preview

- Artifact: the `bento-guestbook-host` executable built from this branch, plus a new example host, `examples/typed-host-modules`, whose executable has three commands: `declarations` prints its modules' generated declarations, `manifest` prints its manifest, and `run <script>` runs a guest script against them. Its module has a typed async function, a deliberately slow cancellable function, and a stream function. The example directory includes a guest script and a `tsconfig.json` that type-checks the script against the printed declarations with `tsc`.
- Reason: host modules are a Rust embedding API with no CLI surface. Building a real host, type-checking a guest against its generated declarations, and running that guest through validation failures, an aborted call, and a stream left early is the cheapest way to exercise every part of this change. The guest book shows the migration of an existing host. Build outputs and generated files stay out of Git.

## Policies and decisions checked

- [VISION.md](../VISION.md): embedding is the default, TypeScript is the working language, and the host decides what code may do. Declarations are checked with real TypeScript tooling, and nothing about a module reaches the guest unless the host registers it.
- [ARCHITECTURE.md](../ARCHITECTURE.md): engine-independent public interfaces, and cancellation in `bento_host`. `Cancellation`, `HostStreamSender`, the specs, and the manifest types expose no engine types.
- [policy.0001](../policies/0001-prefer-voidzero-crates.md): no parsing, transformation, resolution, or bundling role is involved; `schemars` and `regex` are neither Deno nor SWC crates, and `mise run deps:policy` stays clean.
- `decisions/`: no decision records exist.

## Future directions

- Binary host functions and byte streams, for file and media bridging without JSON encoding.
- An adapter that turns an MCP server's `tools/list` into a host module through `schema_function`.
- Handing generated declarations to an in-process type checker without writing files, once Bento has one.

## Alternatives considered

- **One input object instead of positional parameters.** Selected. Rust closures over several parameters lose their names, so generated declarations would read `arg0: string, arg1: number`; optional trailing parameters collide with a trailing options argument; and MCP and the guest book's `append({ name, message })` already use one input object. A struct input keeps its field names and documentation through `schemars`.
- **A closed subset with Bento's own validator, instead of a full validator such as the `jsonschema` crate with best-effort declarations.** Selected. A full validator would accept schemas whose declarations could only say `unknown`, which is the drift this proposal exists to remove, and would add a large dependency. `bento_prompt` already showed that a closed subset can be defined once and enforced exactly.
- **`AsyncIterableIterator` instead of `ReadableStream` for streams.** Selected, pending [an open question](#open-questions). An iterator reads naturally in `for await`, closes through `return()` exactly as ECMA-262 specifies, and declares simply; `ReadableStream.from()` converts it when a guest wants a stream. `ReadableStream` would match `LanguageModel.promptStreaming()` and bring `pipeTo` and `tee`, at the cost of reader locks that matter little for JSON records.
- **A handler context object instead of a bare `Cancellation`.** Rejected. Cancellation is the only per-call fact a handler needs; a context struct would exist for fields nobody has asked for.
- **A `#[host_module]` procedural macro.** Rejected. It cannot describe modules assembled at run time, such as a bridged tool server, and builders already compose with captured services. A macro could be layered on the builders later.
- **Cancelling calls when a shutdown is requested.** Rejected. A guest draining in its `SIGTERM` listener may still need its host functions; disposal is the moment the guest can no longer use the result.
- **Waiting for cancelled host work during disposal.** Rejected. Host services outlive instances by design, and waiting would let a handler that ignores cancellation turn every shutdown into a timeout. `bento_prompt` waits because it owns its native work; a host owns its own.
- **Splitting cancellation and streams into a second proposal.** Rejected by Mark on 2026-11-27. Streams depend on both item schemas and cancellation, so a second proposal would only delay them.

## Open questions

- [NEEDS CLARIFICATION: Should `schema_function` and `schema_stream` validate what their handlers return against `outputSchema` and `itemSchema`? Recommended: yes, always, rejecting the call (or ending the stream) with `HostContractError`, so a manifest handed to a model cannot advertise output the host does not produce. The alternative trusts the host and skips the cost for large results.]
- [NEEDS CLARIFICATION: Should a host be able to opt in to exposing the manifest or declarations to guest code, for example as a `host:` module the host registers, so an agent can discover tools from inside the sandbox? Recommended: no, until a harness needs it; harnesses already put declarations in the model's context from Rust.]
- [NEEDS CLARIFICATION: Should stream functions return an `AsyncIterableIterator`, as proposed, or a `ReadableStream`, matching `LanguageModel.promptStreaming()`?]

## Acknowledgments

Cloudflare's [Code Mode](https://blog.cloudflare.com/code-mode/), Anthropic's [programmatic tool calling](https://platform.claude.com/docs/en/agents-and-tools/tool-use/programmatic-tool-calling), the [Model Context Protocol](https://modelcontextprotocol.io/specification/2025-11-25/schema)'s tool schemas, and `bento_prompt`'s cancellation, stream, and schema-subset design from [Proposal 0002](0002-web-prompt-api.md) shaped this proposal.
