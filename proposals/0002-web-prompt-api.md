---
id: proposal.0002
title: Web Prompt API with macOS Foundation Models
authors: [Mark]
status: awaiting-implementation
pull-request: https://github.com/bento-js/bento/pull/249
issues: []
supersedes: []
---

# Web Prompt API with macOS Foundation Models

## Summary

Add the Web Prompt API's text-session interface to Bento, backed by Apple's on-device Foundation Models. Provide a reusable Rust crate, explicit host permission, genuine cancellable streaming, conversation management, and constrained JSON output. Keep the native dependency optional and leave other operating-system backends for later.

## Authorization

On 2026-10-24, Mark requested the Web Prompt API, macOS Foundation Models as the MVP, a reusable crate, and a pull request stacked on [PR #247](https://github.com/bento-js/bento/pull/247). When asked whether to prepare a proposal, he selected “Yes, prepare the proposal.” The authorization and initial research are recorded in [PR #248](https://github.com/bento-js/bento/pull/248#issuecomment-6386256803).

On 2026-10-24, Mark approved this design for implementation, with macOS 27.0 as the native-backend minimum. He approved the text-session/constrained-JSON scope, engine-independent crate with optional Deno adapter, default-enabled CLI backend, opt-in library backend, and backend-free builds. He also authorized creating and using a fork containing the narrowly scoped binding changes, with eventual upstreaming if the author is amenable. His instruction was “This design is approved to go to implementation”; the [approval record](https://github.com/bento-js/bento/pull/249#issuecomment-6388198021) preserves these decisions. This authorizes implementation and feature/dependency branch publication, not acceptance, merging, package release, or production deployment. The earlier runtime mandate is not used.

The feature branch was stacked on `sesh/rust-style-guide-alignment` at `791c5f76a354efc9f70a41f7b0593fe0522cb8e9`. During proposal preparation, PR #247 merged into `main` as `60ca248`, and deletion of its branch automatically closed PR #248. GitHub rejected reopening the closed PR or changing its base. [PR #249](https://github.com/bento-js/bento/pull/249) replaces it against `main`, which contains the prerequisite commits. The pushed feature history was not rewritten.

## Motivation

Bento applications and embedding hosts cannot currently use the operating system's local language model through a familiar JavaScript API. Each host would have to build a private native bridge, define its own conversation and cancellation semantics, and distribute application-specific declarations. That duplicates integration work and makes otherwise portable scripts depend on one host.

The requested outcome is local inference through the Web Prompt API shape, in both the CLI and embedded runtimes. It is not a cloud inference client, a model downloader, or a new application framework. Developers should be able to check support, create an isolated conversation, consume an answer or stream, and release resources without knowing Apple's Swift APIs.

## Domain grounding

### Established context

The [MDN Prompt API documentation](https://developer.mozilla.org/en-US/docs/Web/API/Prompt_API) describes `LanguageModel` sessions, not stateless completion requests. The normative baseline for this proposal is the [Prompt API Community Group draft](https://webmachinelearning.github.io/prompt-api/) at revision [`6dac677d5425d1175f4697e73dadd572a1ad6483`](https://github.com/webmachinelearning/prompt-api/tree/6dac677d5425d1175f4697e73dadd572a1ad6483), with its [shared lifecycle algorithms](https://webmachinelearning.github.io/writing-assistance-apis/#supporting) at revision [`38ce5471973b34c1bf5e1466febebf20254e5e85`](https://github.com/webmachinelearning/writing-assistance-apis/tree/38ce5471973b34c1bf5e1466febebf20254e5e85). This is an evolving community draft, not a ratified Web standard. Deprecated extension-only sampling controls and experimental tools are distinct from the ordinary web surface.

Apple's [Foundation Models framework](https://developer.apple.com/documentation/foundationmodels) supplies an OS-managed, stateful model. Its [availability](https://developer.apple.com/documentation/foundationmodels/generating-content-and-performing-tasks-with-foundation-models) depends on hardware, Apple Intelligence settings, model readiness, and supported locales. “Model not ready” does not establish that a download is running or provide a download progress API. The on-device model is distinct from macOS 27's Private Cloud Compute model; this proposal uses only the on-device model.

[`foundation-models` 0.12.0](https://docs.rs/foundation-models/0.12.0/foundation_models/) wraps the native framework through a Swift bridge. Its [published source revision](https://docs.rs/crate/foundation-models/0.12.0/source/.cargo_vcs_info.json) is `cf7ab5592b985b0766a3c0ef83e4625c76bef5eb`. Its build requires a modern macOS SDK and compiles and links Swift; its `macos_26_0` feature is not a mechanism for making that dependency build on other operating systems.

Apple exposes [context size](https://developer.apple.com/documentation/foundationmodels/systemlanguagemodel/contextsize) from macOS 26.0, with back deployment, but [native token counting](<https://developer.apple.com/documentation/foundationmodels/systemlanguagemodel/tokenCount(for:)>) requires macOS 26.4. Context includes instructions, conversation history, generated output, and applicable schema overhead. Apple's [context management guidance](https://developer.apple.com/documentation/foundationmodels/managing-the-context-window) describes overflow as an error, not automatic eviction.

### Relevant constraints and principles

- A model is an OS capability; a session is one conversation with its own history and lifetime. The OS may share model weights, but Bento must not share session history or authority between isolates or runtime instances.
- Permission, compiled backend support, and model availability are separate facts. Installing or enabling Apple Intelligence does not grant guest code permission to use it.
- Message roles have meaning. System instructions, user messages, and assistant history must remain distinct native transcript entries; concatenating role labels into one user string is not equivalent.
- A clone is an independent session initialized with the same context. The pinned specification copies context usage; it does not erase the token cost of that history. MDN's usage guide currently claims a reset, while its dedicated clone page and the draft describe preserving context. This proposal follows the pinned draft.
- Web text streams carry append-only chunks. Apple's stream produces snapshots, and the Rust binding distinguishes appended chunks from replacements. Appending a replacement snapshot would corrupt the response.
- Cancellation of a JavaScript promise is not cancellation of native inference. Native work, callback ownership, and session mutation must also terminate or settle safely.
- JSON Schema constraints require constrained generation and a defined translation. Asking the model to “return JSON” is not an implementation of `responseConstraint`.

### Quality bar

The common text path should retain the API's observable promise, stream, event, abort, history, and error behavior. Unsupported capabilities must be discoverable or rejected rather than ignored or simulated. The implementation must keep Bento's event loop responsive, preserve host authority at the Rust boundary, and allow Rust-only reuse without V8, the CLI, or project tooling.

### Remaining uncertainty

The released Rust dependency has two concrete integration gaps: its safe streaming API blocks and does not expose a caller-owned cancellation handle, and its [coverage report](https://github.com/doom-fish/foundation-models-rs/blob/cf7ab5592b985b0766a3c0ef83e4625c76bef5eb/COVERAGE.md) says macOS 27's newer `LanguageModelError` maps to `FMError::Unknown`. Use the authorized, narrowly patched Bento-owned fork at an immutable revision rather than weakening cancellation or error behavior. Keep the changes suitable for eventual upstream contribution; this work does not require an upstream release.

Rust-level transcript reconstruction, supported-schema translation, stream cancellation, and deployment-floor behavior still need implementation-time proof. Native Swift probes establish feasibility, not Rust binding correctness or Bento compatibility.

## Existing baseline

Bento currently supports macOS on Apple Silicon. It uses `deno_core` extensions with native operations and ESM bootstraps. `crates/bento_runtime/src/assembly.rs` registers them in the main isolate; `crates/bento_runtime/src/workers.rs` registers them in Web Workers and Node worker threads. There is no existing Prompt API or language-model backend.

`bento_host::Grants` and `GrantRequest::resolve` define deny-by-default authority and nested-launch ceilings. `bento_cli/src/grant_flags.rs` constructs requests; task subprocesses carry the serialized ceiling. These are the existing authority path, not something a new JavaScript wrapper may bypass. `bento_runtime/src/lifecycle.rs` owns shutdown deadlines and distinguishes completed disposal from `TimedOut`.

PR #247 establishes inherited workspace lints, documented public runtime and host APIs, one workspace version declaration per shared dependency, shared test support, and rustdoc with warnings denied. The new crate must follow those conventions. No existing Bento crate ships its own ambient global declarations or optional native backend, so those boundaries must be explicit here rather than attributed to an existing convention.

The native dependency offers typed transcripts, restoration, async non-streaming generation, schema builders, and token counting. It does not offer a direct append or clone method; those operations can be built from measured transcript reconstruction. `prewarm()` is not append. Its safe session pointer is private, so a consumer cannot simply attach a cancellable FFI stream to a high-level session.

Direct Swift probes on 2026-10-24, using Swift 6.4 and SDK 27.0, observed:

- `SystemLanguageModel.default.availability` returned `available`; a real local greeting request returned `Hello!`.
- Native context size was 8,192 tokens, demonstrating why the commonly documented 4,096 must not be hard-coded.
- A conversation storing the code word `bento` had a native transcript count of 77 tokens; a reconstructed session also counted 77 and recalled `bento`.
- An empty session had zero transcript entries but a native token count of one. Public accounting must consistently account for native serialization overhead.
- Decoding an ordinary object JSON Schema directly into Apple's `GenerationSchema` failed because Apple's representation required `x-order`. The binding's decoder also accepts its own dynamic-schema representation; arbitrary web JSON Schema cannot be passed through unchanged.

## Proposed solution

The approved MVP is the text-session Prompt API, including constrained JSON output, backed by Apple's default on-device model on macOS 27.0 or newer. Expose `globalThis.LanguageModel` in Bento's main and worker isolates. Put reusable session behavior and the native adapter in `bento_prompt`; make its Deno extension an optional feature. Do not introduce a public backend plugin registry or placeholder Windows/Linux implementations.

The intended usage is ordinary Prompt API JavaScript, with authority supplied outside the script:

```ts
if ((await LanguageModel.availability()) !== "available") {
    throw new Error("Local language-model access is unavailable");
}

const session = await LanguageModel.create({
    initialPrompts: [{ role: "system", content: "Answer concisely." }],
});
try {
    for await (const chunk of session.promptStreaming("Describe a bento lunch.")) {
        console.log(chunk);
    }
} finally {
    session.destroy();
}
```

The CLI grant is `--allow-language-model`; Rust hosts grant the same capability through `Grants`. Neither network nor native-addon grants are required for this fixed, on-device backend.

## Detailed design

### Reusable crate and build boundary

Add `crates/bento_prompt` as a normal workspace library with inherited package metadata and lints, documented public types, typed errors, and compiled Rust examples. Its default build has no Deno/V8 or Swift dependency. A `foundation-models` feature enables the target-specific Apple dependency; a separate `deno` feature exposes the extension adapter. Public Rust session APIs use Bento-owned messages, options, results, and errors, not engine handles or upstream framework types. A Rust caller can perform availability checks, create a session, append, generate, stream, measure, clone, and dispose without constructing Bento's runtime.

The core crate selects the compiled native implementation internally. No user-selectable model names, provider registry, cloud configuration, or public trait hierarchy for hypothetical backends is introduced. Unsupported-target and backend-disabled paths report genuine unavailability and cannot generate responses.

`bento_runtime` installs the Deno adapter independently of whether a native backend is compiled. Its `prompt-foundation-models` Cargo feature is opt-in for embedding hosts. The CLI build enables its forwarding feature by default on its currently supported macOS-arm64 target; `--no-default-features` provides a backend-free build without the new SDK dependency. The native dependency is target-gated as well as feature-gated. Enabling a build feature never grants runtime permission.

The enabled-backend runtime floor is macOS 27.0, with Xcode/SDK 27.0 or newer for builds. Native-enabled artifacts must declare their deployment requirement; they must not be advertised as binaries for earlier macOS releases. Backend-free artifacts retain the pre-feature platform requirements. CI must verify both feature configurations and record the actual SDK used rather than assuming a moving runner label establishes compatibility.

### Authority and isolation

Add `Grants.language_model: bool`, defaulting to false, and `GrantRequest.language_model: Option<bool>`. This is one capability, not an invented set of model identifiers. Unmentioned requests inherit an existing ceiling; without a ceiling they deny. An explicit false narrows authority, and requesting true below a false ceiling fails with the existing wider-grant diagnostic. Decode an older ceiling without the new field as false. Update affected Rust struct-literal callers and tests together.

`--allow-language-model` requests true; `--allow-language-model=false` explicitly narrows it. The grant propagates through ordinary tasks and nested Bento launches via the existing ceiling encoding. It does not change the subprocess trust boundary: an independently executed native program is not sandboxed by a Bento grant.

The adapter receives immutable instance authority and checks it in Rust before inspecting native model readiness, creating resources, or running operations. Denied `availability()` resolves to `"unavailable"`; denied `create()` rejects with `NotAllowedError`. Preserve that web-facing error name rather than replacing it with Bento's `NotCapable`; runtime diagnostics must still classify an uncaught denial as a permission failure.

Each isolate owns its sessions and resource identifiers. Main-isolate and worker registration use the same adapter and inherited grant, with separate session state. A worker or another runtime cannot use a guessed identifier to reach another isolate's session. Runtime disposal and worker termination cancel owned work and release owned sessions.

Bento has no browsing contexts, documents, HTTP Permissions Policy, or transient user activation. Its adaptation uses host grants, not fabricated browser activation or origin checks. Providing this global does not install it into a web browser visiting an application served by Bento.

### Exposed JavaScript contract

Expose nonconstructible `LanguageModel` instances extending the existing `EventTarget`. Provide `availability()`, `create()`, `prompt()`, `promptStreaming()`, `append()`, `measureContextUsage()`, `clone()`, `destroy()`, read-only `contextUsage` and `contextWindow`, and `oncontextoverflow`. Reuse the runtime's existing `Promise`, `ReadableStream`, `AbortSignal`, `DOMException`, and WebIDL conventions.

`availability()` and `create()` validate and canonicalize the specified text input/output languages. Invalid language tags reject according to the pinned specification; well-formed but unsupported types or languages yield unavailable or `NotSupportedError`, respectively. Language support comes from the native model's supported languages and locale matching, not a fixed English-only list. No model identifier or OS-specific selection parameter is added to the web surface.

Only `"available"` and `"unavailable"` are returned by this backend. Disabled Apple Intelligence, ineligible hardware, an unsupported OS, and model-not-ready are unavailable. Do not infer `"downloading"` from model-not-ready, initiate downloads, or change system settings. Availability is advisory; creation rechecks it and may fail if conditions changed. A `monitor` callback receives a `CreateMonitor` EventTarget. For an available model, follow the shared initialization algorithm's two completion notifications: `downloadprogress` with `loaded` zero and then one, `total` one, and `lengthComputable` true. These are the specified initialization endpoints, not evidence of an OS download; do not fabricate intermediate progress. A thrown monitor callback rejects creation and leaves no session behind.

Accept string prompts and arrays of role-bearing messages whose content is a string or text-content array. Canonicalize empty input and adjacent text content according to the pinned draft, including treating `prompt([])` as an empty user prompt. Preserve message boundaries and native roles. At most one system message is allowed, as the first message of a previously empty conversation; later system messages fail with `TypeError`. Initial user and assistant history is loaded without generating replies.

Image, audio, tool-call, and tool-response capabilities are not supported by this MVP. Well-formed configurations requesting them report unavailable; using unsupported content rejects with `NotSupportedError`. Invalid enum values and malformed dictionaries follow WebIDL validation. Experimental tools and `samplingMode`, and deprecated `params()`, `topK`, `temperature`, `inputUsage`, and `inputQuota`, are not implemented. Unknown dictionary members follow normal WebIDL handling, not a custom strict-options parser.

A syntactically valid assistant `prefix: true` rejects with `NotSupportedError`: the selected backend cannot faithfully continue a partial assistant turn. An invalid prefix position or role still produces the specified `SyntaxError`. Do not simulate continuation by inserting an instruction into a new user turn. This is an approved compatibility boundary, not a full-conformance claim.

Ship matching ambient declarations with `bento_prompt` and expose their text for Rust embedders to write into their guest projects. Include their use in the runnable TypeScript example. This requires no npm package publication or new runtime command. The guide must distinguish the declared web API shape from backend capability support.

### Conversation state, context, and cloning

Maintain ordered, typed transcript state. A native session may be replaced when restoring or appending a transcript without changing the identity of the JavaScript session. Native object identity is not the web session's identity.

`append()` validates and measures the candidate transcript, then commits it without model generation. `prompt()` and `promptStreaming()` prefill the supplied messages and then generate one assistant response. A message must not be applied twice when a transcript is reconstructed and a native response is requested. Transcript reconstruction must preserve assistant turns as assistant turns, not replay them as new requests.

Use native token counting for the same representation sent to the model. Reserve the measured empty-transcript serialization overhead consistently in the advertised usable context window, so a session without initial messages starts at zero public usage. Count initial instructions, subsequent messages, generated output, and included constraint overhead consistently against that usable capacity. Query the native context limit; do not hard-code a token limit, substitute character-count heuristics, or claim infinity for a bounded model.

`measureContextUsage()` measures what the supplied input and relevant constraint representation would add, without mutating session state or producing a response. Its result is finite and nonnegative. `contextUsage` is updated before the corresponding operation settles; a clone starts with the source's context usage and capacity, not a fresh quota.

Reject oversized initialization or prefill with `QuotaExceededError` before committing input. On an existing session, fire one `contextoverflow` event for the failing operation. If generation itself exhausts capacity, reject the promise or error the stream with `QuotaExceededError` and fire the same event; previously emitted chunks cannot be retracted. Never silently evict history, summarize it, or retry the prompt. A clone does not solve overflow unless it was taken before the overflowing operation.

Serialize state-affecting operations on one session in invocation order, including clone snapshots. A queued cancellation does not mutate history. Different sessions may progress independently. After successful prefill, generation failure or cancellation retains the accepted input but not a partial assistant response; restore that committed transcript before another operation runs. Failed validation or preflight changes nothing. A successfully completed clone owns its own lifetime and history; destroying its source does not destroy the clone.

### Streaming, aborts, and disposal

Text `promptStreaming()` returns a `ReadableStream` immediately and emits genuine incremental strings. Successful chunks concatenate to the generated response. Do not implement ordinary text streaming by waiting for `prompt()` and emitting the whole answer. If the native model rewrites a prefix already emitted, stop that operation with `UnknownError` rather than append a replacement as though it were a delta or silently return corrupted text.

Use a cancellation-capable native stream. The released dependency's blocking callback API is insufficient: putting it on a blocking thread alone does not make it cancellable. The selected dependency solution must supply explicit native cancellation and safe callback ownership. Model execution, synchronous native work, and waits must not block the JavaScript event loop. Transport buffering must be bounded, and cancellation must remain possible while a consumer is slow or stops reading.

Honor operation signals, including an already-aborted signal and arbitrary JavaScript abort reasons. Creation's signal remains associated with the created session, as specified by the shared destruction algorithm. A per-operation signal cancels that operation, not the whole session. Clone's signal cancels cloning; a completed clone does not inherit the source's destruction controller. Preserve the specified synchronous throw versus rejected-promise behavior, notably for an already-aborted `promptStreaming()`.

Stream cancellation stops native generation without turning cancellation into a fabricated stream error. `destroy()` is synchronous and idempotent, immediately makes the session unusable, cancels pending work with `AbortError`, and schedules safe native release. Later session operations fail with the destruction reason. Native callbacks must not access released state, and a subsequent operation must wait for cancelled native work to settle before reusing its session.

Isolate termination, runtime shutdown, and resource cleanup must use the same cancellation mechanism even when guest code forgets `destroy()`. An idle session must not keep the CLI alive indefinitely. Preserve Bento's bounded shutdown reporting; do not report completed disposal while owned inference or callbacks remain live. Test this independently of happy-path generation.

### Constrained JSON output and failures

Support a documented, closed JSON Schema subset for `responseConstraint`: object, array, string, integer, number, and boolean types; `properties`, `required`, `additionalProperties: false`, homogeneous `items`, `minItems`, `maxItems`, numeric `minimum` and `maximum`, string-valued `enum`, and `title`/`description` annotations. Nested combinations of these are supported. Omitted `required` makes properties optional. Omitted `additionalProperties` does not require the model to invent undeclared keys; the returned object may contain only declared properties. This is not a promise to support every possible result allowed by an open schema.

Validate schema shape and JSON serializability before mutating the session, translate supported keywords into native dynamic-schema constructs, and use native guided decoding. Unsupported keywords or types, including references, composition, arbitrary additional-property schemas, regex constraints, and nullable unions, reject with `NotSupportedError`; malformed supported keywords reject with `TypeError`. Unknown constraint keywords are not silently discarded. The guide and tests must enumerate this exact supported subset.

Return JSON text, not a Rust-shaped object or a JavaScript object. Verify the completed result against the accepted constraint before reporting success; do not retry failed generation or substitute a canned response. `omitResponseConstraintInput` suppresses the schema's instructional input but not constrained decoding, and its accounting must match the native request. Specified true without a constraint fails as the draft requires.

Constrained `promptStreaming()` is supported but buffers structured snapshots until one final valid JSON string can be emitted. Native partial JSON snapshots can rewrite earlier text and are not append-only chunks. This distinction must be documented; unconstrained text streaming remains incremental.

Map host denial to `NotAllowedError`, unsupported native capabilities/locales to `NotSupportedError`, guardrail filtering or refusal to `NotReadableError`, context exhaustion to `QuotaExceededError`, and cancellation to the appropriate abort reason. Creation and clone infrastructure failures use `OperationError`; otherwise-unclassified generation failures use `UnknownError`, following the pinned draft. Preserve typed native errors internally; do not infer categories by matching localized error messages. Keep Apple's default safety settings and never route to Private Cloud Compute or an external model as a fallback.

### Verification derived from this contract

Write behavioral tests first, then guides from this design, then implementation. Deterministic tests must exercise validation, grant denial before native inspection, nested-ceiling narrowing, main/worker isolation, ordered mutations, independent clones, quota failure without partial prefill, input retention after generation cancellation, stream chunk integrity, abort reason identity, disposal races, and constraint translation. Use a controllable test backend only to drive these observable state transitions and error boundaries; an echo mock is not evidence of native inference.

Run the existing `vp run check` and `vp run rust` gates, including rustdoc and inherited lints. Cover native-enabled and backend-free builds; verify the core crate without the Deno feature. No model-dependent test may silently return early and be reported as a live-model pass. Ordinary CI must run deterministic behavior and unavailable-model paths without requiring Apple Intelligence. Record live-model checks separately on an eligible Mac.

The live smoke must exercise both the actual Bento CLI and a separate Rust consumer: readiness, initial instructions/history, multi-turn generation, incremental text streaming, constrained JSON, append without inference, context measurement, clone independence, abort during inference and streaming, stream-reader cancellation, worker termination, and runtime shutdown. Check structural behavior and resource settlement rather than exact model wording. Also exercise denied access without calling the backend. The dependency's macOS 27 error translation and native stream cancellation require direct proof, not just wrapper-unit coverage.

## Compatibility

This is a documented server-side adaptation of the pinned Prompt API text surface, not a browser implementation or a claim of complete draft conformance. Browser activation and Permissions Policy are replaced by explicit host authority; worker exposure is a Bento extension. Unsupported modalities, assistant prefixes, experimental tools/sampling, and deprecated APIs are stated explicitly. Windows and Linux availability does not imply those platforms are newly supported by the rest of Bento.

Existing guests receive no new model authority. Existing Rust callers constructing `Grants` or `GrantRequest` exhaustively need the new field; migrate all repository callers without a compatibility shim. Old serialized ceilings deny the new capability. No session history is persisted, logged, or exported by default.

The default CLI's native feature raises the requirements of that build to macOS 27.0 and a suitable SDK. Backend-free builds and default library embeddings do not acquire those requirements.

## Implications on adoption

A native-enabled consumer needs Apple Silicon, macOS 27.0 or newer, Apple Intelligence enabled, and the OS model ready. Developers building it need Xcode/SDK 27.0 or newer and the Swift toolchain. Enabling the Rust feature and granting guest access are separate steps. Unavailable users must receive an actionable explanation in the guide; Bento does not install model assets or change settings for them.

The new library remains a workspace/path-or-Git dependency under the repository's existing `publish = false` policy. Reuse does not require public crates.io or npm publication. Removing the native feature removes the Swift/framework dependency; removing the grant denies guest access without removing the API surface.

## Scope

This change includes `bento_prompt`, its Apple backend and optional Deno adapter, the necessary corrected dependency integration, runtime and worker installation, the new grant and CLI flag, ceiling propagation, declarations, behavioral tests, guides, and runnable CLI/Rust previews. It includes updating existing grant/API documentation and affected callers. The Detailed design is the behavior specification; there is no duplicate acceptance checklist.

### Out of scope

Windows AI APIs, Linux or portable model engines, Gemma weights, model selection, downloadable models, cloud inference, Private Cloud Compute, adapters/fine-tuning, persistent conversations, tool execution, image/audio inputs, summarizer/writer/rewriter APIs, browser UI integration, general backend plugins, public package publication, and unrelated runtime portability work are excluded. No exclusion permits replacing required cancellation, streaming, or isolation with a placeholder.

## Preview

Provide an installable local macOS-arm64 Bento executable with the native feature, a small runnable TypeScript Prompt API example, and a separate Rust embedding/reuse example built against the same crate revision. Include the backend-free build for exercising genuine unavailability and a denial invocation without the grant. Give exact build and run commands in the guide and record paths, source revision, OS/SDK, and observed behavior in the PR readiness report.

This is the cheapest real preview for a library-backed CLI capability that depends on the reviewer's local Apple Intelligence setup. The existing documentation deployment alone is insufficient. Keep binaries, logs, native prompts/responses used for diagnostics, and temporary artifacts out of Git. Retain the runnable preview for review; clean up only owned temporary processes and smoke data. No public release, production deployment, or external inference service is required.

## Policies and decisions checked

- [VISION.md](../VISION.md): embedding first, explicit host authority, observable compatibility, upstream reuse, and paying only for compiled capabilities. The optional backend and engine-free crate satisfy those constraints; capability limitations remain visible.
- [ARCHITECTURE.md](../ARCHITECTURE.md): native-boundary permission enforcement, per-instance lifecycle, worker consistency, and typed host interfaces. No browser-origin machinery or general service abstraction is introduced.
- [policy.0001](../policies/0001-prefer-voidzero-crates.md): the model adapter is not a JavaScript tooling implementation. Existing Oxc resolution/transformation remains unchanged; no SWC/Deno parser dependency is added.
- Numbered decisions: none exist. This proposal records the feature-specific choices; no separate policy or decision is needed merely to repeat them.
- [PR #247](https://github.com/bento-js/bento/pull/247): inherit its Rust quality gates, public API documentation requirements, shared dependency declarations, and test-support conventions.

## Future directions

A Windows backend or a portable quantized-model backend could implement the same session behavior after its actual capabilities and resource lifecycle are studied. Additional modalities, assistant continuation, and experimental tools would require an approved compatibility extension, not silent changes to availability. None requires adding speculative implementations or a public plugin protocol now.

## Alternatives considered

1. **Selected: text sessions and constrained JSON in an engine-independent crate with an optional Deno adapter.** This supports both Rust reuse and the requested web global, keeps the native dependency selectable, and makes the known Apple capability boundaries explicit. It requires schema translation and a dependency correction for cancellable streaming and modern errors.
2. **A reusable Deno-extension-only crate.** This also implements the requested JavaScript surface and can be embedded into other Deno-based runtimes, with less public Rust API. It makes every consumer depend on the engine substrate and does not provide standalone native-session reuse. It is viable if reuse is intended only for Deno/V8 hosts.
3. **A private host module or a minimal `prompt()` wrapper.** This is smaller but does not deliver the requested global, conversation-management surface, or stream/lifecycle contract. It is not recommended as an implementation of the Web Prompt API.

Supporting macOS 26.0–26.3 would require a different, explicitly approximate context-accounting contract or omitting context operations. Although 26.4 supplies token counting, Mark selected macOS 27.0 as the minimum. Using a separate Swift bridge would give Bento control over missing bindings but duplicate the dependency's work; the approved route is a narrowly patched, pinned fork suitable for eventual upstream contribution.

## Open questions

None. The human resolved the three design and dependency-authority questions and authorized implementation on 2026-10-24.
