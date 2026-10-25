---
id: guide.prompt-api
title: Using the Prompt API
describes: [proposal.0002]
---

# Using the Prompt API

## What it does

Bento exposes `LanguageModel` in its main isolate and workers. It provides text conversations, incremental text responses, context measurement, independent session clones, and JSON constrained by a supported schema. Apple's default on-device Foundation Models model runs the inference. Bento never falls back to a cloud service or Private Cloud Compute.

This is a server-side adaptation of the [Prompt API draft](https://webmachinelearning.github.io/prompt-api/), with host grants instead of browser permissions or user activation. A browser visiting a server written in Bento does not acquire this API. The supported surface excludes image and audio input, tools, assistant-prefix continuation, experimental sampling controls, and deprecated quota and sampling properties.

## Build and grant access

The native backend requires Apple silicon, macOS 27.0 or newer, Apple Intelligence enabled, and the system model ready. Building requires Xcode with the macOS 27 SDK or newer, its Swift toolchain, and Bento's pinned Rust toolchain. The CLI enables the native backend by default:

```sh
cargo install --locked --path crates/bento_cli
bento run --allow-read=. --allow-language-model prompt.ts
```

`--allow-language-model` grants access to the model. It grants no file, network, environment, or native-addon access. Without it, `LanguageModel.availability()` resolves to `"unavailable"` and `LanguageModel.create()` rejects with a `NotAllowedError` DOMException before probing the native model. No interactive permission prompt appears.

Nested Bento task launches inherit this capability as a ceiling. An omitted flag inherits the ceiling, `--allow-language-model=false` denies it explicitly, and requesting it under a denying ceiling fails before running the guest. An older serialized ceiling without the capability denies it. Native programs started by a trusted task retain the task's operating-system authority; Bento grants are not an OS sandbox.

To build without the native dependency or new SDK requirement:

```sh
cargo install --locked --path crates/bento_cli --no-default-features
```

A backend-free build still exposes the JavaScript API and validates its arguments, but reports `"unavailable"` even when the host grants model access. Windows and Linux model backends are not implemented.

For a Rust host embedding `bento_runtime`, enable its `prompt-foundation-models` Cargo feature and set `RuntimeConfig.grants.language_model = true`. Library embeddings enable neither by default. Workers inherit the immutable grant, while each isolate owns its own sessions.

## Start a conversation

Save this as `prompt.ts` in a project with a `package.json`, then run it with the grant shown above.

```typescript
const availability = await LanguageModel.availability({
    expectedInputs: [{ type: "text", languages: ["en"] }],
    expectedOutputs: [{ type: "text", languages: ["en"] }],
});
if (availability !== "available") {
    throw new Error("The local language model is unavailable");
}

const session = await LanguageModel.create({
    initialPrompts: [{ role: "system", content: "Answer briefly and plainly." }],
});
try {
    console.log(await session.prompt("Explain what a tide pool is."));
    console.log(await session.prompt("Name one animal that lives there."));
} finally {
    session.destroy();
}
```

Availability is advisory. Creation checks readiness again. Bento reports only `"available"` or `"unavailable"`; a model that is not ready is unavailable, without implying that a download is running. Check the grant, build features, macOS version, and Apple Intelligence settings when it is unavailable. Bento does not download model assets or change system settings.

Expected text languages use canonical BCP 47 tags and the native model's locale support. An invalid tag rejects; a valid unsupported language makes availability unavailable and creation reject with `NotSupportedError`. Omitting language expectations leaves language selection to the model. Requesting an unsupported expected content type also reports unavailable.

`LanguageModel` cannot be constructed with `new`. Sessions are `EventTarget` instances. A `monitor` callback passed to `create()` receives an `EventTarget` on which `downloadprogress` reports the specified initialization endpoints: loaded zero, then one, with total one and `lengthComputable` true. Those events do not describe an OS download. Throwing from the callback rejects creation without retaining a session.

For an available model, the notifications run in separate tasks before native initialization. Creation can still fail after both notifications. Unavailable or failed readiness produces neither. Aborting creation rejects with the signal's reason and prevents later progress notifications, including when the abort is queued as a microtask by a progress listener. Native initialization already under way stays owned until it settles; the immediate rejection does not abandon it.

## Messages and context

A prompt can be a string or an array of messages. Message roles are `system`, `user`, and `assistant`; content is a string or an array of `{ type: "text", value: "..." }` items. Initial user and assistant messages restore conversation history without generating responses. A system message may occur only once, first in a previously empty conversation. Later system messages reject with `TypeError`.

Inputs follow WebIDL conversion. For example, a text-content `value` of `7` becomes `"7"`; `ArrayBuffer`, typed-array, `Blob`, and image values cannot be relabelled as text. Message and content sequences accept iterable objects, including callable objects. An iterator property of `null` selects the string branch instead.

```typescript
await session.append([
    { role: "user", content: "Remember the code word: kelp." },
    { role: "assistant", content: "The code word is kelp." },
]);
const addedUsage = await session.measureContextUsage("What is the code word?");
console.log({ addedUsage, used: session.contextUsage, capacity: session.contextWindow });
console.log(await session.prompt("What is the code word?"));
```

`append()` commits messages without generating a response. `measureContextUsage()` measures the supplied input and relevant constraint overhead without changing history. An empty prompt, including `prompt([])`, is an empty user message. Adjacent text content is canonicalized before processing.

Usage and capacity are measured with the native tokenizer for the representation sent to the model. The empty transcript's serialization overhead is reserved in the advertised capacity, so an empty session starts at zero public usage. Capacity is queried from the model; it is not a fixed token count. Usage updates before an operation settles and includes successful generated output. An included response constraint remains part of its prompt's context cost on later turns and in clones; it is not a one-time surcharge.

An oversized initial conversation or prompt rejects with `QuotaExceededError` before committing its input. An existing session also dispatches one `contextoverflow` event for the failing operation; `oncontextoverflow` is available as an event-handler property. Generation can itself exhaust capacity. In that case the promise rejects or the stream errors, and chunks already delivered remain delivered. Bento never evicts history, summarizes it, or retries automatically.

State-changing operations and clone snapshots on one session run in invocation order. Different sessions can progress independently. After input passes preflight, failed or cancelled generation retains that input but discards any partial assistant response. A queued cancellation and failed validation or preflight leave history unchanged.

## Stream and cancel

```typescript
const controller = new AbortController();
const stream = session.promptStreaming("Describe three tide-pool habitats.", {
    signal: controller.signal,
});
for await (const chunk of stream) {
    console.log(chunk);
}
```

`promptStreaming()` returns a `ReadableStream` immediately. Ordinary text streams deliver incremental strings whose concatenation is the response. Bento errors the stream if the native model rewrites a prefix that was already delivered. It does not append a replacement as if it were a new suffix. A non-streaming `prompt()` has delivered nothing to its caller, so the same rewrite is not a failure there: the promise resolves with the model's final text.

An operation's `signal` cancels only that operation. JavaScript abort reasons are preserved, including non-Error values. An already-aborted `promptStreaming()` call throws synchronously; promise-returning operations reject. Cancelling a reader stops native generation without inventing an error for the cancelled stream. Backpressure keeps transport buffering bounded, and cancellation still works when the consumer stops reading.

The signal passed to `create()` controls the session's lifetime. Aborting it destroys that session and cancels pending work. `destroy()` is synchronous and idempotent: the session becomes unusable immediately, pending work is cancelled, and native cleanup settles safely in the background. Later operations fail with the destruction reason. Runtime shutdown and worker termination cancel owned sessions even when guest code forgets to destroy them; an idle session does not keep the CLI running.

## Clone a conversation

```typescript
const branch = await session.clone();
try {
    console.log(await branch.prompt("Suggest a different explanation."));
} finally {
    branch.destroy();
}
```

A clone starts with the source's committed history, context usage, and capacity at its ordered snapshot point. Later changes and destruction are independent. A clone's signal cancels the cloning operation; it does not control a successfully completed clone. Cloning an already full conversation does not create extra capacity.

## Constrain JSON output

```typescript
const responseConstraint = {
    type: "object",
    properties: {
        habitat: { type: "string" },
        animal: { type: "string", enum: ["crab", "anemone", "snail"] },
    },
    required: ["habitat", "animal"],
    additionalProperties: false,
};
const text = await session.prompt("Choose a tide-pool animal and its habitat.", {
    responseConstraint,
});
const answer = JSON.parse(text);
```

The response is JSON text. Bento translates the accepted schema into native guided decoding and validates the completed result before reporting success. The supported subset is closed:

| Schema element | Supported behavior |
| --- | --- |
| `type` | `object`, `array`, `string`, `integer`, `number`, or `boolean` |
| `properties`, `required` | Nested object properties; omitted `required` makes them optional |
| `additionalProperties` | Only `false`; when omitted, output may still contain only declared properties |
| `items` | One schema for every array item |
| `minItems`, `maxItems` | Nonnegative array length bounds in the signed 64-bit range |
| `minimum`, `maximum` | Numeric bounds; integer bounds must fit the native signed 64-bit range, and number bounds use binary64 |
| `enum` | String-valued choices |
| `title`, `description` | Schema annotations |

Each schema node requires an explicit supported `type`; an array also requires its homogeneous `items` schema. Names in `required` must be unique and declared in `properties`. An integer is a number with no fractional part, so `1.0` is valid both as an integer result and as an array length bound. A fractional `minimum` or `maximum` on an integer schema narrows inward to the integers it permits: `minimum: 1.2` permits 2 and above, and `maximum: 4.7` permits 4 and below. Bounds that leave no permitted integer, such as `minimum: 1.2` with `maximum: 1.8`, reject with `TypeError`. Empty enums, repeated required names, negative or fractional array bounds, inverted bounds, and incorrectly typed annotations are malformed. Tuple `items`, non-string enum values, and `additionalProperties: true` are outside the supported subset.

The native backend uses signed 64-bit integers and binary64 floating-point numbers. Integer bounds and array length bounds outside the signed 64-bit range reject with `NotSupportedError` rather than being rounded or dropped. For `type: "number"`, a Rust caller's exact integer bound that binary64 cannot represent also rejects instead of silently changing the constraint; use `type: "integer"` for exact signed 64-bit integer bounds. Supported keywords used outside their listed type, such as `enum` on an integer schema, are also outside the supported subset.

Malformed supported keywords reject with `TypeError` before changing history. Unsupported keywords or types reject with `NotSupportedError`, including references, composition, regex constraints, nullable unions, and arbitrary additional-property schemas. Unknown keywords are never silently discarded. A constraint must be a JSON-serializable object: `null` rejects with `TypeError`, while omitting the option or passing `undefined` leaves generation unconstrained.

`omitResponseConstraintInput: true` suppresses the schema's instructional input while retaining constrained decoding. Measurement uses the same choice. Setting it without a constraint rejects with `TypeError` in `measureContextUsage()`; it has no effect in `prompt()` or `promptStreaming()` without a constraint.

Constrained `promptStreaming()` buffers native structured snapshots and emits one final validated JSON string. Partial JSON snapshots can rewrite earlier text, so constrained streaming has different latency and chunk behavior from ordinary text streaming. Invalid model output fails rather than being retried or replaced.

## Reuse from Rust

The `bento_prompt` crate works independently of Bento's runtime. Its default feature set depends on neither Deno/V8 nor Swift. Enable `foundation-models` for native inference; enable the separate `deno` feature only when embedding the web adapter. The crate is a workspace, path, or Git dependency; no registry publication is required.

For a native-enabled embedding executable, set `MACOSX_DEPLOYMENT_TARGET=27.0` when building so the final Mach-O declares its requirement. A library cannot set its consumer's final deployment target. Bento's CLI sets that target in its own linker configuration; backend-free consumers retain their existing deployment requirements. `bento_prompt::MACOS_DEPLOYMENT_FLOOR` is the lowest macOS release the native backend supports, `"27.0"`. `bento_prompt::NATIVE_SDK_VERSION` is `Some` with the macOS SDK version this build's backend was compiled against, and `None` when no backend is compiled in, so a host can report both instead of repeating them.

The public Rust API uses `Session`, typed text `Message` values, `CreateOptions`, `PromptOptions`, and `Cancellation`. It provides availability, creation, append, generation, streaming, measurement, independent forks, destruction, and awaited disposal. Direct Rust callers own model authority; the JavaScript adapter is where host grants are enforced. Await disposal when the caller needs proof that native inference and callbacks have settled.

`bento_prompt::TYPESCRIPT_DECLARATIONS` contains the ambient declarations from `crates/bento_prompt/types/prompt.d.ts`. Embedders can write that text into a guest project's type inputs. The declarations describe the implemented surface rather than the whole draft: message and expected content types are narrowed to `"text"`, and members Bento does not implement are absent instead of declared. Those absent members are image, audio, tool-call and tool-response content, tool declarations, `samplingMode`, `topK`, `temperature`, `params()`, `inputUsage`, and `inputQuota`.

## Exercise the preview

From this repository, run the TypeScript preview with a native-enabled Bento:

```sh
bento run --allow-read=. --allow-language-model examples/prompt-api/main.ts
bento run --allow-read=. examples/prompt-api/main.ts --denied
bento run --allow-read=. --allow-language-model examples/prompt-api/shutdown.ts
bento run --allow-read=. --allow-language-model examples/prompt-api/idle.ts
```

The main preview exercises initial history, multiple prompts, measurement, cloning, text and constrained streams, cancellation, and worker termination. The denied run exercises the permission boundary. The shutdown preview exits after receiving a native chunk without calling `destroy()`, so runtime disposal must cancel and settle the remaining work. The idle preview creates a session and lets the event loop finish without explicit destruction; the CLI must exit normally. Model work still unsettled at the shutdown deadline is reported as incomplete disposal instead, and `bento` exits with status 1.

On a backend-free build, the same preview exercises genuine unavailability:

```sh
bento run --allow-read=. --allow-language-model examples/prompt-api/main.ts
```

It prints `Availability unavailable`, then fails with an uncaught `Error`: `LanguageModel is unavailable. Check the --allow-language-model grant, that Bento was built with the native backend, macOS 27 or newer, and that Apple Intelligence is enabled and ready.` The process exits with status 1.

The Rust example uses the reusable crate directly:

```sh
MACOSX_DEPLOYMENT_TARGET=27.0 cargo run --locked -p bento_prompt --features foundation-models --example prompt_session
```

These native previews fail explicitly if the model is unavailable. They are separate from deterministic CI tests and must be run on an eligible Mac with Apple Intelligence ready. CI provides native `bento-macos27-arm64-prompt` and backend-free `bento-macos26-arm64-backend-free` archive artifacts; extracting the archive preserves the executable's permissions.

## Errors and limits

| Failure | JavaScript result |
| --- | --- |
| Missing model grant | `NotAllowedError`; an uncaught denial is a Bento permission diagnostic |
| Unsupported capability, schema feature, or locale | `NotSupportedError` |
| Invalid message or supported schema shape | `TypeError`; invalid prefix role or position uses `SyntaxError` |
| Valid assistant-prefix continuation request | `NotSupportedError` |
| Native refusal or safety filtering | `NotReadableError` |
| Context exhaustion | `QuotaExceededError`, plus an existing session's `contextoverflow` event |
| Cancellation | The applicable abort reason; explicit destruction uses `AbortError` |
| Failure to determine model readiness | `UnknownError` |
| Other native initialization failure during creation or cloning | `OperationError` |
| Other generation failure, including a rewritten prefix already delivered by a stream | `UnknownError` |

Apple's default safety settings remain enabled. Model output is probabilistic and can be incorrect even when it satisfies a schema. The typed error categories come from the native framework; Bento does not infer them from localized messages. Session history stays in memory and is not automatically persisted, exported, or logged.
