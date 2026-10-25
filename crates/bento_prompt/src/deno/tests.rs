//! Behavior of the JavaScript Prompt API surface, exercised through a real V8 isolate.
//!
//! Each test builds a `JsRuntime` holding the same extension the runtime installs, over the
//! scripted engine from [`crate::tests::scripted`]. The assertions are what guest code can
//! observe: resolved values, error names and identities, event order, stream chunks, and the
//! transitions the engine recorded. No model, host, or CLI is involved.

use std::sync::Arc;
use std::time::Duration;

use ::deno_web::{BlobStore, InMemoryBroadcastChannel, deno_web};
use ::deno_webidl::deno_webidl;
use deno_core::{JsRuntime, PollEventLoopOptions, RuntimeOptions, serde_v8, v8};
use serde_json::{Value, json};

use crate::tests::scripted::{Event, Script, ScriptedEngine};
use crate::{ErrorKind, Role};

use super::{Access, Cleanup};

/// Installs the classes the test scripts assert against. The adapter itself loads what it
/// needs from the same extension scripts, so these globals are identical to the ones a Bento
/// guest sees after the runtime's bootstrap.
const GLOBALS: &str = r#"
import { core } from "ext:core/mod.js";
const event = core.loadExtScript("ext:deno_web/02_event.js");
const exception = core.loadExtScript("ext:deno_web/01_dom_exception.js");
const abort = core.loadExtScript("ext:deno_web/03_abort_signal.js");
const streams = core.loadExtScript("ext:deno_web/06_streams.js");
const timers = core.loadExtScript("ext:deno_web/02_timers.js");
Object.assign(globalThis, {
    Event: event.Event,
    EventTarget: event.EventTarget,
    ProgressEvent: event.ProgressEvent,
    DOMException: exception.DOMException,
    QuotaExceededError: exception.QuotaExceededError,
    AbortController: abort.AbortController,
    AbortSignal: abort.AbortSignal,
    ReadableStream: streams.ReadableStream,
    setTimeout: timers.setTimeout,
    clearTimeout: timers.clearTimeout,
});
globalThis.failure = error => ({
    name: error?.name ?? String(error),
    domException: error instanceof DOMException,
    quotaExceeded: error instanceof QuotaExceededError,
    requested: error?.requested ?? null,
    quota: error?.quota ?? null,
    typeError: error instanceof TypeError,
    rangeError: error instanceof RangeError,
    message: typeof error?.message === "string" ? error.message : "",
});
globalThis.caught = async promise => {
    try {
        return { settled: await promise };
    } catch (error) {
        globalThis.lastError = error;
        return { failed: failure(error) };
    }
};
// Resource identifiers are all JavaScript holds, and these ops are what anyone holding one
// would reach with it.
globalThis.ops = core.ops;
globalThis.collect = async stream => {
    const chunks = [];
    const reader = stream.getReader();
    for (;;) {
        const { value, done } = await reader.read();
        if (done) return { chunks };
        chunks.push(value);
    }
};
"#;

struct Harness {
    runtime: JsRuntime,
    engine: ScriptedEngine,
    cleanup: Cleanup,
    _installed: crate::tests::scripted::Guard,
}

impl Harness {
    fn new(access: Access) -> Self {
        Self::with_cleanup(access, Cleanup::new())
    }

    /// A harness whose sessions report their native cleanup to `cleanup`, which can outlive
    /// this isolate and the runtime it was built on.
    fn with_cleanup(access: Access, cleanup: Cleanup) -> Self {
        let engine = ScriptedEngine::new();
        let installed = engine.install();
        let mut runtime = JsRuntime::new(RuntimeOptions {
            extensions: vec![
                deno_webidl::init(),
                deno_web::init(
                    Arc::new(BlobStore::default()),
                    None,
                    false,
                    InMemoryBroadcastChannel::default(),
                ),
                super::extension(access, cleanup.clone()),
            ],
            ..Default::default()
        });
        runtime
            .lazy_load_es_module_with_code("ext:bento_prompt_deno/test_globals.mjs", GLOBALS)
            .expect("test globals load");
        Self { runtime, engine, cleanup, _installed: installed }
    }

    fn granted() -> Self {
        Self::new(Access::granted())
    }

    fn denied() -> Self {
        Self::new(Access::denied())
    }

    /// Runs `source` as the body of an async function and returns what it resolved with.
    async fn run(&mut self, source: &str) -> Value {
        let script = format!("(async () => {{\n{source}\n}})()");
        let promise = self.runtime.execute_script("file:///prompt-test.js", script).unwrap();
        let resolved = self.runtime.resolve(promise);
        let value = self
            .runtime
            .with_event_loop_promise(resolved, PollEventLoopOptions::default())
            .await
            .expect("test script resolves");
        self.json(value)
    }

    /// Runs `source` synchronously, as the body of a function, and returns what it returned.
    fn sync(&mut self, source: &str) -> Value {
        let script = format!("(() => {{\n{source}\n}})()");
        let value = self.runtime.execute_script("file:///prompt-test.js", script).unwrap();
        self.json(value)
    }

    fn json(&mut self, value: v8::Global<v8::Value>) -> Value {
        let runtime = &mut self.runtime;
        deno_core::scope!(scope, runtime);
        let local = v8::Local::new(scope, value);
        serde_v8::from_v8(scope, local).expect("the result is JSON")
    }

    fn generations(&self) -> usize {
        self.engine.log().iter().filter(|event| matches!(event, Event::Generated(_))).count()
    }

    /// Waits for an engine step the next phase of a test depends on, so that phase acts on
    /// work the session actually reached rather than on a hopeful delay.
    async fn reached<F>(&self, step: F)
    where
        F: Fn(&Event) -> bool,
    {
        let waited = tokio::time::timeout(
            Duration::from_secs(5),
            self.engine.wait_for(|log| log.iter().any(&step)),
        )
        .await;
        assert!(waited.is_ok(), "the engine never reached this step: {:?}", self.engine.log());
    }

    /// Drives this isolate's event loop until it has nothing left to do, and reports whether
    /// it finished within `within` rather than hanging the test when something holds it open.
    async fn drains_within(&mut self, within: Duration) -> bool {
        tokio::time::timeout(within, self.runtime.run_event_loop(PollEventLoopOptions::default()))
            .await
            .is_ok_and(|drained| drained.is_ok())
    }
}

fn name(result: &Value) -> &str {
    result["failed"]["name"].as_str().unwrap_or_else(|| panic!("expected a failure: {result}"))
}

// Authority, availability, and creation.

#[tokio::test]
async fn denied_access_is_reported_without_consulting_the_model() {
    let mut harness = Harness::denied();
    let result = harness
        .run(
            r#"
        return {
            availability: await LanguageModel.availability(),
            creation: await caught(LanguageModel.create()),
        };
        "#,
        )
        .await;
    assert_eq!(result["availability"], "unavailable");
    assert_eq!(name(&result["creation"]), "NotAllowedError");
    assert!(result["creation"]["failed"]["domException"].as_bool().unwrap());
    assert!(
        harness.engine.log().is_empty(),
        "denial must precede native readiness: {:?}",
        harness.engine.log()
    );
}

#[tokio::test]
async fn an_available_model_creates_a_session_with_its_context_accounting() {
    let mut harness = Harness::granted();
    harness.engine.set_context_window(40);
    let result = harness
        .run(
            r#"
        const availability = await LanguageModel.availability();
        const session = await LanguageModel.create();
        return {
            availability,
            usage: session.contextUsage,
            window: session.contextWindow,
            isEventTarget: session instanceof EventTarget,
            handler: session.oncontextoverflow,
        };
        "#,
        )
        .await;
    assert_eq!(result["availability"], "available");
    assert_eq!(result["usage"], 0, "an empty session starts at zero public usage");
    assert_eq!(result["window"], 39, "the empty-transcript overhead is reserved");
    assert_eq!(result["isEventTarget"], true);
    assert_eq!(result["handler"], Value::Null);
}

#[tokio::test]
async fn an_unavailable_model_rejects_creation_as_unsupported() {
    let mut harness = Harness::granted();
    harness.engine.set_available(false);
    let result = harness
        .run(
            r#"
        return {
            availability: await LanguageModel.availability(),
            creation: await caught(LanguageModel.create()),
        };
        "#,
        )
        .await;
    assert_eq!(result["availability"], "unavailable");
    assert_eq!(name(&result["creation"]), "NotSupportedError");
}

#[tokio::test]
async fn an_unsupported_language_is_unavailable_and_rejects_creation() {
    let mut harness = Harness::granted();
    // The scripted engine matches tags exactly, so a supported tag is the canonical one.
    harness.engine.set_languages(["en-US"]);
    let result = harness
        .run(
            r#"
        const expectedInputs = [{ type: 'text', languages: ['fr'] }];
        return {
            canonicalized: await LanguageModel.availability({
                expectedInputs: [{ type: 'text', languages: ['EN-us', 'en-US'] }],
            }),
            unsupported: await LanguageModel.availability({ expectedInputs }),
            creation: await caught(LanguageModel.create({ expectedInputs })),
        };
        "#,
        )
        .await;
    assert_eq!(
        result["canonicalized"], "available",
        "`EN-us` reaches the model canonicalized and deduplicated as `en-US`"
    );
    assert_eq!(result["unsupported"], "unavailable");
    assert_eq!(name(&result["creation"]), "NotSupportedError");
}

/// `LanguageModelExpected`'s `languages` is `sequence<DOMString>`, so a lone tag is not a
/// one-element list: it fails WebIDL conversion before the configuration means anything.
#[tokio::test]
async fn a_lone_language_tag_is_a_type_error_before_the_model_is_asked() {
    let mut harness = Harness::granted();
    let rejected = harness
        .run(
            r#"
        const scalar = [{ type: 'text', languages: 'en' }];
        let threw = false;
        let creation = null;
        try {
            creation = LanguageModel.create({ expectedInputs: scalar });
        } catch (error) {
            threw = true;
        }
        return {
            threw,
            inputs: await caught(LanguageModel.availability({ expectedInputs: scalar })),
            outputs: await caught(LanguageModel.availability({ expectedOutputs: scalar })),
            creation: await caught(creation),
        };
        "#,
        )
        .await;
    assert_eq!(rejected["threw"], false, "creation rejects its promise rather than throwing");
    for key in ["inputs", "outputs", "creation"] {
        let failure = &rejected[key]["failed"];
        assert_eq!(
            failure["typeError"], true,
            "a scalar `languages` is a conversion failure, not a tag: {}",
            rejected[key]
        );
        assert_eq!(failure["domException"], false);
        assert_eq!(failure["rangeError"], false, "nothing canonicalized a tag that never arrived");
    }
    assert!(
        harness.engine.log().is_empty(),
        "conversion refuses before availability or creation consults the model: {:?}",
        harness.engine.log()
    );
    let accepted = harness
        .run(
            r#"
        return {
            availability: await LanguageModel.availability({
                expectedInputs: [{ type: 'text', languages: ['en'] }],
            }),
            created: (await LanguageModel.create({
                expectedOutputs: [{ type: 'text', languages: ['en'] }],
            })) instanceof LanguageModel,
        };
        "#,
        )
        .await;
    assert_eq!(accepted["availability"], "available", "the sequence form still reaches the model");
    assert_eq!(accepted["created"], true);
}

#[tokio::test]
async fn a_monitor_observes_the_two_initialization_endpoints() {
    let mut harness = Harness::granted();
    let result = harness
        .run(
            r#"
        const events = [];
        let handled = 0;
        const session = await LanguageModel.create({
            monitor(monitor) {
                monitor.ondownloadprogress = () => { handled += 1; };
                monitor.addEventListener('downloadprogress', event => events.push({
                    type: event.type,
                    loaded: event.loaded,
                    total: event.total,
                    lengthComputable: event.lengthComputable,
                    isProgressEvent: event instanceof ProgressEvent,
                }));
            },
        });
        return { events, handled, created: session instanceof LanguageModel };
        "#,
        )
        .await;
    let events = result["events"].as_array().expect("the monitor received events");
    assert_eq!(
        events.len(),
        2,
        "an available model reports only the specified initialization endpoints"
    );
    for (index, event) in events.iter().enumerate() {
        assert_eq!(event["type"], "downloadprogress");
        assert_eq!(event["loaded"].as_f64(), Some(index as f64), "loaded reaches zero then one");
        assert_eq!(event["total"].as_f64(), Some(1.0));
        assert_eq!(event["lengthComputable"], true);
        assert_eq!(event["isProgressEvent"], true);
    }
    assert_eq!(result["handled"], 2, "the event handler attribute receives both events");
    assert_eq!(result["created"], true);
}

#[tokio::test]
async fn a_throwing_monitor_rejects_creation_and_leaves_no_session() {
    let mut harness = Harness::granted();
    let result = harness
        .run(
            r#"
        const refusal = new Error('no thanks');
        let identical = false;
        try {
            await LanguageModel.create({ monitor() { throw refusal; } });
        } catch (error) {
            identical = error === refusal;
        }
        return { identical };
        "#,
        )
        .await;
    assert_eq!(result["identical"], true);
    assert!(
        !harness.engine.log().iter().any(|event| matches!(event, Event::Opened(_))),
        "no session may be opened for a rejected creation"
    );
}

/// `monitor` is a `CreateMonitorCallback`, a callback function. A value that is not callable
/// fails argument conversion, which the draft performs before the permission check, so the
/// host's answer never decides what an invalid argument means.
#[tokio::test]
async fn a_non_callable_monitor_is_a_type_error_before_the_grant_is_consulted() {
    let mut harness = Harness::denied();
    let result = harness
        .run(
            r#"
        let threw = false;
        let creation = null;
        try {
            creation = LanguageModel.create({ monitor: 7 });
        } catch (error) {
            threw = true;
        }
        let invoked = false;
        return {
            threw,
            number: await caught(creation),
            null: await caught(LanguageModel.create({ monitor: null })),
            // A callback function, unlike a callback interface, is not satisfied by an
            // object carrying a method.
            object: await caught(LanguageModel.create({ monitor: { handleEvent() {} } })),
            absent: await caught(LanguageModel.create()),
            callable: await caught(LanguageModel.create({ monitor() { invoked = true; } })),
            invoked,
        };
        "#,
        )
        .await;
    assert_eq!(result["threw"], false, "creation rejects its promise rather than throwing");
    for key in ["number", "null", "object"] {
        let failure = &result[key]["failed"];
        assert_eq!(
            failure["typeError"], true,
            "an uncallable monitor fails conversion, before denial: {}",
            result[key]
        );
        assert_eq!(failure["domException"], false);
    }
    assert_eq!(name(&result["absent"]), "NotAllowedError", "an absent monitor is valid");
    assert_eq!(name(&result["callable"]), "NotAllowedError");
    assert_eq!(result["invoked"], false, "a denied creation never runs the monitor callback");
    assert!(
        harness.engine.log().is_empty(),
        "neither refusal consults the model: {:?}",
        harness.engine.log()
    );
}

/// A callback function is invoked with an undefined receiver. Guest code that reads `this`
/// must not be handed the call's own converted arguments: the dictionary the callback was
/// found in is an internal detail of the call, not an object the guest ever named.
#[tokio::test]
async fn a_monitor_callback_is_invoked_without_a_receiver() {
    let mut harness = Harness::granted();
    let result = harness
        .run(
            r#"
        const received = [];
        await LanguageModel.create({
            monitor: function (monitor) {
                'use strict';
                received.push({
                    undefinedThis: this === undefined,
                    receiver: Object.prototype.toString.call(this),
                    isMonitor: monitor instanceof EventTarget,
                });
            },
        });
        return { received };
        "#,
        )
        .await;
    let received = result["received"].as_array().expect("the monitor callback ran").clone();
    assert_eq!(received.len(), 1, "the callback runs once");
    let call = &received[0];
    assert_eq!(call["isMonitor"], true, "the callback still receives its monitor");
    assert_eq!(
        call["undefinedThis"], true,
        "a callback function is invoked with no receiver, but got {}: {call}",
        call["receiver"]
    );
}

/// The creation signal governs the creation from the moment it begins, which includes the
/// monitor callback creation runs itself. A callback that returns after aborting decides the
/// outcome: the rejection is the abort's own reason, not whatever the configuration would
/// have been refused for afterwards, and nothing is asked of the model for a creation
/// already given up. A callback that throws still rejects with its own exception, which the
/// pinned algorithm returns as a fresh rejection rather than the signal's reason.
#[tokio::test]
async fn a_monitor_callback_that_aborts_decides_the_creation() {
    let mut harness = Harness::granted();
    let result = harness
        .run(
            r#"
        const settle = async (options, reason, refusal) => {
            try {
                await LanguageModel.create(options);
                return 'resolved';
            } catch (error) {
                if (error === reason) return 'the abort reason';
                if (error === refusal) return 'the thrown exception';
                return error?.name ?? String(error);
            }
        };
        const unserviceable = new AbortController();
        const unserviceableReason = { stop: 'an unserviceable configuration' };
        const ready = new AbortController();
        const readyReason = { stop: 'a ready model' };
        const both = new AbortController();
        const bothReason = { stop: 'aborted first' };
        const refusal = new Error('no thanks');
        return {
            unserviceable: await settle(
                {
                    signal: unserviceable.signal,
                    expectedInputs: [{ type: 'image' }],
                    monitor() {
                        unserviceable.abort(unserviceableReason);
                    },
                },
                unserviceableReason,
            ),
            ready: await settle(
                {
                    signal: ready.signal,
                    monitor() {
                        ready.abort(readyReason);
                    },
                },
                readyReason,
            ),
            thrown: await settle(
                {
                    signal: both.signal,
                    monitor() {
                        both.abort(bothReason);
                        throw refusal;
                    },
                },
                bothReason,
                refusal,
            ),
        };
        "#,
        )
        .await;
    assert_eq!(
        result["unserviceable"], "the abort reason",
        "an abort from the monitor callback precedes the configuration's own refusal"
    );
    assert_eq!(
        result["ready"], "the abort reason",
        "a creation given up during its callback does not wait to find out it was possible"
    );
    assert_eq!(
        result["thrown"], "the thrown exception",
        "a callback that throws rejects with its own exception, aborted signal or not"
    );
    assert!(
        harness.engine.log().is_empty(),
        "a creation abandoned before it began asks the model nothing: {:?}",
        harness.engine.log()
    );
    tokio::time::timeout(Duration::from_secs(5), harness.cleanup.settle())
        .await
        .expect("an abandoned creation leaves no unsettled native work");
    assert!(harness.cleanup.is_idle());
}

/// A `downloadprogress` listener runs on the event loop while the creation that reported the
/// endpoint is still unfinished, and it may abort the creation signal. The shared algorithm
/// queues each endpoint as its own task that returns early once the signal aborted, so the
/// endpoints after the abort are not reported, the promise rejects with the abort reason
/// itself, and the creation never goes on to initialize a session nobody can receive.
#[tokio::test]
async fn aborting_at_the_first_endpoint_rejects_creation_without_initializing() {
    let mut harness = Harness::granted();
    let result = harness.run(ABORT_AT_ENDPOINT.replace("$ENDPOINT", "0").as_str()).await;
    assert_eq!(result["settled"], "rejected", "an aborted creation hands out no session");
    assert_eq!(result["identical"], true, "the arbitrary abort reason is preserved");
    assert_eq!(
        endpoints(&result),
        vec![0.0],
        "an abort during the first endpoint stops the second from being reported"
    );
    tokio::time::timeout(Duration::from_secs(5), harness.cleanup.settle())
        .await
        .expect("an abandoned creation leaves no unsettled native work");
    assert!(harness.cleanup.is_idle());
}

/// The same abort, one endpoint later: the creation is just as abandoned when its signal
/// aborts inside the final notification as inside the first.
#[tokio::test]
async fn aborting_at_the_final_endpoint_rejects_creation_without_initializing() {
    let mut harness = Harness::granted();
    let result = harness.run(ABORT_AT_ENDPOINT.replace("$ENDPOINT", "1").as_str()).await;
    assert_eq!(result["settled"], "rejected", "an aborted creation hands out no session");
    assert_eq!(result["identical"], true, "the arbitrary abort reason is preserved");
    assert_eq!(endpoints(&result), vec![0.0, 1.0], "both endpoints were reported before the abort");
    tokio::time::timeout(Duration::from_secs(5), harness.cleanup.settle())
        .await
        .expect("an abandoned creation leaves no unsettled native work");
    assert!(harness.cleanup.is_idle());
}

/// Each endpoint is reported by its own task, so a listener handling the first one can
/// schedule work that runs before the second: an abort queued as a microtask reaches the
/// signal first, and the endpoint that follows it is not reported.
#[tokio::test]
async fn an_abort_scheduled_while_the_first_endpoint_is_handled_suppresses_the_second() {
    let mut harness = Harness::granted();
    let result = harness
        .run(
            r#"
        const controller = new AbortController();
        const reason = { stop: 'scheduled from the first endpoint' };
        const loaded = [];
        let settled = 'pending';
        let identical = false;
        try {
            await LanguageModel.create({
                signal: controller.signal,
                monitor(monitor) {
                    monitor.addEventListener('downloadprogress', event => {
                        loaded.push(event.loaded);
                        if (event.loaded === 0) {
                            queueMicrotask(() => controller.abort(reason));
                        }
                    });
                },
            });
            settled = 'resolved';
        } catch (error) {
            settled = 'rejected';
            identical = error === reason;
        }
        return { loaded, settled, identical };
        "#,
        )
        .await;
    assert_eq!(result["settled"], "rejected", "an aborted creation hands out no session");
    assert_eq!(result["identical"], true, "the arbitrary abort reason is preserved");
    assert_eq!(
        endpoints(&result),
        vec![0.0],
        "an abort scheduled while the first endpoint was handled precedes the second"
    );
    tokio::time::timeout(Duration::from_secs(5), harness.cleanup.settle())
        .await
        .expect("an abandoned creation leaves no unsettled native work");
    assert!(harness.cleanup.is_idle());
}

/// The endpoints describe the initialization that is about to be attempted, so a ready model
/// reports both of them before initialization runs and before anyone knows whether it will
/// succeed. A creation that then fails has still told its monitor what it attempted.
#[tokio::test]
async fn a_failing_initialization_reports_its_endpoints_before_rejecting() {
    let mut harness = Harness::granted();
    harness.engine.fail_open(ErrorKind::Unknown);
    let result = harness
        .run(
            r#"
        const order = [];
        let name = null;
        try {
            await LanguageModel.create({
                monitor(monitor) {
                    monitor.addEventListener('downloadprogress', event => {
                        order.push(`loaded ${event.loaded}`);
                    });
                },
            });
            order.push('resolved');
        } catch (error) {
            order.push('rejected');
            name = error.name;
        }
        return { order, name };
        "#,
        )
        .await;
    assert_eq!(
        result["order"],
        json!(["loaded 0", "loaded 1", "rejected"]),
        "a ready model reports both endpoints before its initialization fails"
    );
    assert_eq!(
        result["name"], "OperationError",
        "an otherwise-unknown initialization failure is infrastructure, not a model answer"
    );
    assert!(
        !harness.engine.log().iter().any(|event| matches!(event, Event::Opened(_))),
        "initialization failed, so no session exists: {:?}",
        harness.engine.log()
    );
    tokio::time::timeout(Duration::from_secs(5), harness.cleanup.settle())
        .await
        .expect("a failed creation leaves no unsettled native work");
    assert!(harness.cleanup.is_idle());
}

/// Readiness that cannot be determined is a failure about infrastructure, not an answer
/// about the model: creation rejects with it rather than deciding for itself. The failure is
/// consumed once, so a creation that quietly probed again and succeeded would resolve a
/// session here instead of failing, and a monitored creation reports no endpoints for an
/// initialization whose readiness it never learned.
#[tokio::test]
async fn a_failed_readiness_probe_rejects_creation_as_unknown() {
    let mut harness = Harness::granted();
    harness.engine.fail_next_availability(ErrorKind::Unknown);
    let monitored = harness
        .run(
            r#"
        const loaded = [];
        const failure = await caught(LanguageModel.create({
            monitor(monitor) {
                monitor.addEventListener('downloadprogress', event => loaded.push(event.loaded));
            },
        }));
        return { failure, loaded };
        "#,
        )
        .await;
    assert_eq!(
        name(&monitored["failure"]),
        "UnknownError",
        "a readiness failure is reported, not swallowed by a second probe"
    );
    assert!(
        endpoints(&monitored).is_empty(),
        "nothing is reported for an initialization that was never known to be possible: {monitored}"
    );
    harness.engine.fail_next_availability(ErrorKind::Unknown);
    let unmonitored = harness.run("return await caught(LanguageModel.create());").await;
    assert_eq!(
        name(&unmonitored),
        "UnknownError",
        "a creation without a monitor reports the same readiness failure"
    );
    let recovered =
        harness.run("return (await LanguageModel.create()) instanceof LanguageModel;").await;
    assert_eq!(recovered, true, "the spent failure leaves a working engine behind");
}

/// Readiness decides whether anything is reported at all: a model that will not initialize
/// reports no endpoints, and refuses exactly as it does without a monitor.
#[tokio::test]
async fn a_model_that_will_not_initialize_reports_no_endpoints() {
    let mut harness = Harness::granted();
    harness.engine.set_available(false);
    let result = harness
        .run(
            r#"
        const loaded = [];
        const watch = monitor =>
            monitor.addEventListener('downloadprogress', event => loaded.push(event.loaded));
        return {
            unready: await caught(LanguageModel.create({ monitor: watch })),
            unsupported: await caught(LanguageModel.create({
                expectedInputs: [{ type: 'image' }],
                monitor: watch,
            })),
            loaded,
        };
        "#,
        )
        .await;
    assert_eq!(name(&result["unready"]), "NotSupportedError");
    assert_eq!(name(&result["unsupported"]), "NotSupportedError");
    assert!(
        endpoints(&result).is_empty(),
        "an initialization that will not be attempted is not reported: {result}"
    );
}

/// Initialization is native work that takes time, and the endpoints precede it: while the
/// open is still in flight the monitor has already been told everything. A signal aborted in
/// that window rejects the creation there and then, with its own reason and no session,
/// while the native work it started stays owned until it actually settles.
#[tokio::test]
async fn aborting_during_initialization_releases_the_session_it_opened() {
    let mut harness = Harness::granted();
    let hold = harness.engine.hold_open();
    let reported = harness
        .run(
            r#"
        globalThis.controller = new AbortController();
        globalThis.reason = { stop: 'mid-initialization' };
        globalThis.loaded = [];
        globalThis.creating = LanguageModel.create({
            signal: controller.signal,
            monitor(monitor) {
                monitor.addEventListener('downloadprogress', event => loaded.push(event.loaded));
            },
        });
        // The open is held, so this waits for what the monitor is told beforehand, and gives
        // up rather than hanging if a creation reports nothing until it has finished.
        const deadline = Date.now() + 2000;
        while (loaded.length < 2 && Date.now() < deadline) {
            await new Promise(resolve => setTimeout(resolve, 1));
        }
        return { loaded };
        "#,
        )
        .await;
    assert_eq!(
        endpoints(&reported),
        vec![0.0, 1.0],
        "the endpoints are reported before the initialization they describe"
    );
    tokio::time::timeout(
        Duration::from_secs(5),
        harness.engine.wait_for(|log| log.iter().any(|event| matches!(event, Event::Opening))),
    )
    .await
    .expect("the native open starts, or this proves nothing");
    // The abort lands while the native open is still held, so the rejection has to be the
    // abort's own doing rather than the native side eventually finishing.
    let result = harness
        .run(
            r#"
        controller.abort(reason);
        const outcome = await Promise.race([
            creating.then(
                () => 'resolved',
                error => error === reason
                    ? 'rejected with the reason'
                    : 'rejected with something else',
            ),
            new Promise(resolve => setTimeout(() => resolve('still pending'), 2000)),
        ]);
        return { outcome };
        "#,
        )
        .await;
    assert_eq!(
        result["outcome"], "rejected with the reason",
        "the creation rejects with its own abort reason while its open is still in flight"
    );
    assert!(
        !harness.cleanup.is_idle(),
        "an early rejection must not pretend the native open settled: {:?}",
        harness.engine.log()
    );
    hold.release();
    tokio::time::timeout(Duration::from_secs(5), harness.cleanup.settle())
        .await
        .expect("the abandoned session's native release settles");
    let log = harness.engine.log();
    assert!(
        log.iter().any(|event| matches!(event, Event::Opened(_))),
        "the open produced a session, or this proves nothing: {log:?}"
    );
    assert!(
        log.iter().any(|event| matches!(event, Event::Released)),
        "the session the abort abandoned is released: {log:?}"
    );
    assert!(harness.cleanup.is_idle());
}

/// Creation watched by a monitor whose listener aborts the creation signal while handling the
/// `$ENDPOINT` endpoint.
const ABORT_AT_ENDPOINT: &str = r#"
const controller = new AbortController();
const reason = { stop: 'during initialization' };
const loaded = [];
let settled = 'pending';
let identical = false;
try {
    await LanguageModel.create({
        signal: controller.signal,
        monitor(monitor) {
            monitor.addEventListener('downloadprogress', event => {
                loaded.push(event.loaded);
                if (event.loaded === $ENDPOINT) {
                    controller.abort(reason);
                }
            });
        },
    });
    settled = 'resolved';
} catch (error) {
    settled = 'rejected';
    identical = error === reason;
}
return { loaded, settled, identical };
"#;

fn endpoints(result: &Value) -> Vec<f64> {
    result["loaded"]
        .as_array()
        .unwrap_or_else(|| panic!("the monitor recorded its endpoints: {result}"))
        .iter()
        .map(|loaded| loaded.as_f64().expect("a numeric `loaded`"))
        .collect()
}

#[tokio::test]
async fn the_creation_signal_controls_the_session_lifetime() {
    let mut harness = Harness::granted();
    let result = harness
        .run(
            r#"
        const controller = new AbortController();
        const reason = { stop: 'now' };
        const session = await LanguageModel.create({ signal: controller.signal });
        controller.abort(reason);
        let identical = false;
        try {
            await session.prompt('hello');
        } catch (error) {
            identical = error === reason;
        }
        return { identical, usable: session.contextWindow > 0 };
        "#,
        )
        .await;
    assert_eq!(result["identical"], true, "the session adopts the creation signal's reason");
    // Release happens on the driver after the abort settles the promise.
    harness.reached(|event| matches!(event, Event::Released)).await;
}

// Input validation and canonicalization.

#[tokio::test]
async fn text_operations_require_an_input_argument() {
    let mut harness = Harness::granted();
    for _ in 0..3 {
        harness.engine.script(Script::text(["noted"]));
    }
    let result = harness
        .run(
            r#"
        const session = await LanguageModel.create();
        const before = session.contextUsage;
        const missing = {};
        for (const method of ['prompt', 'append', 'measureContextUsage']) {
            missing[method] = await caught(session[method]());
        }
        let stream;
        let streaming;
        try {
            stream = session.promptStreaming();
            streaming = 'returned a stream';
        } catch (error) {
            streaming = error.name;
        }
        if (stream !== undefined) await collect(stream);
        const unchanged = session.contextUsage === before;
        await session.prompt(undefined);
        session.destroy();
        return { missing, streaming, unchanged };
        "#,
        )
        .await;

    for method in ["prompt", "append", "measureContextUsage"] {
        assert_eq!(name(&result["missing"][method]), "TypeError", "{method} requires input");
    }
    assert_eq!(result["streaming"], "TypeError", "streaming conversion throws synchronously");
    assert_eq!(result["unchanged"], true, "missing arguments change no conversation state");
    let requests = harness.engine.requests();
    assert_eq!(requests.len(), 1, "only explicit undefined starts generation");
    assert_eq!(requests[0].prompt.content, "undefined");
}

#[tokio::test]
async fn webidl_rejects_a_null_constraint_without_starting_unconstrained_generation() {
    let mut harness = Harness::granted();
    for _ in 0..3 {
        harness.engine.script(Script::text(["noted"]));
    }
    let result = harness
        .run(
            r#"
        const session = await LanguageModel.create();
        const before = session.contextUsage;
        const prompt = await caught(session.prompt('x', { responseConstraint: null }));
        const measurement = await caught(session.measureContextUsage('x', {
            responseConstraint: null,
        }));
        let streaming;
        let stream;
        try {
            stream = session.promptStreaming('x', { responseConstraint: null });
            streaming = 'returned a stream';
        } catch (error) {
            streaming = error.name;
        }
        if (stream !== undefined) await collect(stream);
        const unchanged = session.contextUsage === before;
        await session.prompt('unconstrained', { responseConstraint: undefined });
        session.destroy();
        return { prompt, measurement, streaming, unchanged };
        "#,
        )
        .await;
    assert_eq!(name(&result["prompt"]), "TypeError");
    assert_eq!(name(&result["measurement"]), "TypeError");
    assert_eq!(result["streaming"], "TypeError");
    assert_eq!(result["unchanged"], true);
    let requests = harness.engine.requests();
    assert_eq!(requests.len(), 1, "only the absent constraint permits generation");
    assert_eq!(requests[0].prompt.content, "unconstrained");
}

#[tokio::test]
async fn webidl_converts_text_content_values_through_the_string_union_branch() {
    let mut harness = Harness::granted();
    harness.engine.script(Script::text(["noted"]));
    let result = harness
        .run(
            r#"
        const session = await LanguageModel.create();
        const values = [
            7, true, null, 9n, { toString() { return 'kelp'; } }, new SharedArrayBuffer(1),
        ];
        const converted = await caught(session.prompt([{
            role: 'user',
            content: values.flatMap(value => [
                { type: 'text', value },
                { type: 'text', value: '|' },
            ]),
        }]));
        const beforeInvalid = session.contextUsage;
        const invalid = [];
        const nonText = [
            Symbol('text'),
            new Uint8Array([7]),
            new ArrayBuffer(1),
            new Uint8Array(new SharedArrayBuffer(1)),
        ];
        for (const value of nonText) {
            invalid.push(await caught(session.append([{
                role: 'user', content: [{ type: 'text', value }],
            }])));
        }
        const unchanged = session.contextUsage === beforeInvalid;
        const sharedImage = await caught(session.append([{
            role: 'user', content: [{ type: 'image', value: new SharedArrayBuffer(1) }],
        }]));
        session.destroy();
        return { converted, invalid, unchanged, sharedImage };
        "#,
        )
        .await;
    assert_eq!(result["converted"]["settled"], "noted", "{result}");
    for failure in result["invalid"].as_array().unwrap() {
        assert_eq!(name(failure), "TypeError");
    }
    assert_eq!(result["unchanged"], true);
    assert_eq!(
        name(&result["sharedImage"]),
        "NotSupportedError",
        "a bare shared buffer is not a BufferSource, so it reaches the type check as a string"
    );
    let requests = harness.engine.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].prompt.content, "7|true|null|9|kelp|[object SharedArrayBuffer]|");
}

#[tokio::test]
async fn webidl_selects_callable_iterables_and_reads_the_iterator_once() {
    let mut harness = Harness::granted();
    for _ in 0..4 {
        harness.engine.script(Script::text(["noted"]));
    }
    let result = harness
        .run(
            r#"
        const session = await LanguageModel.create();
        function messages() {}
        messages[Symbol.iterator] = function* () {
            if (this !== messages) throw new Error('wrong iterator receiver');
            yield { role: 'user', content: 'callable prompt' };
        };
        function content() {}
        content[Symbol.iterator] = function* () {
            yield { type: 'text', value: 'callable content' };
        };
        const fallback = { [Symbol.iterator]: null, toString() { return 'string fallback'; } };
        let reads = 0;
        const once = {
            get [Symbol.iterator]() {
                if (++reads !== 1) throw new Error('iterator read twice');
                return function* () { yield { role: 'user', content: 'single read' }; };
            },
        };
        const outcomes = [];
        for (const input of [messages, [{ role: 'user', content }], fallback, once]) {
            outcomes.push(await caught(session.prompt(input)));
        }
        const beforeInvalid = session.contextUsage;
        const invalid = await caught(session.prompt({
            [Symbol.iterator]: 7,
            toString() { return 'must not fall back'; },
        }));
        const unchanged = session.contextUsage === beforeInvalid;
        session.destroy();
        return { outcomes, reads, invalid, unchanged };
        "#,
        )
        .await;
    for outcome in result["outcomes"].as_array().unwrap() {
        assert_eq!(outcome["settled"], "noted", "{outcome}");
    }
    assert_eq!(result["reads"], 1);
    assert_eq!(name(&result["invalid"]), "TypeError");
    assert_eq!(result["unchanged"], true);
    let requests = harness.engine.requests();
    let prompts: Vec<_> = requests.iter().map(|request| request.prompt.content.as_str()).collect();
    assert_eq!(prompts, ["callable prompt", "callable content", "string fallback", "single read"]);
}

#[tokio::test]
async fn prompts_are_canonicalized_before_reaching_the_model() {
    let mut harness = Harness::granted();
    for _ in 0..3 {
        harness.engine.script(Script::text(["noted"]));
    }
    let result = harness
        .run(
            r#"
        const session = await LanguageModel.create();
        await session.prompt([]);
        await session.prompt([
            { role: 'user', content: [{ type: 'text', value: 'one' }, { type: 'text', value: ' two' }] },
        ]);
        await session.prompt('plain string');
        return { usage: session.contextUsage };
        "#,
        )
        .await;
    let requests = harness.engine.requests();
    assert_eq!(requests.len(), 3);
    assert_eq!(requests[0].prompt.role, Role::User, "an empty prompt is one empty user message");
    assert_eq!(requests[0].prompt.content, "");
    assert_eq!(requests[1].prompt.content, "one two", "adjacent text content is joined");
    assert_eq!(requests[2].prompt.content, "plain string");
    assert!(result["usage"].as_u64().unwrap() > 0);
}

#[tokio::test]
async fn message_roles_and_prefixes_are_validated() {
    let mut harness = Harness::granted();
    let result = harness
        .run(r#"
        const session = await LanguageModel.create({
            initialPrompts: [{ role: 'system', content: 'Be brief.' }],
        });
        return {
            lateSystem: await caught(session.prompt([{ role: 'system', content: 'More.' }])),
            userPrefix: await caught(session.prompt([{ role: 'user', content: 'x', prefix: true }])),
            misplaced: await caught(session.prompt([
                { role: 'assistant', content: 'x', prefix: true },
                { role: 'user', content: 'y' },
            ])),
            assistantPrefix: await caught(session.prompt([
                { role: 'user', content: 'Finish this.' },
                { role: 'assistant', content: 'Once upon', prefix: true },
            ])),
            image: await caught(session.prompt([
                { role: 'user', content: [{ type: 'image', value: 'x' }] },
            ])),
            usage: session.contextUsage,
        };
        "#)
        .await;
    assert_eq!(name(&result["lateSystem"]), "TypeError");
    assert_eq!(name(&result["userPrefix"]), "SyntaxError");
    assert_eq!(name(&result["misplaced"]), "SyntaxError");
    assert_eq!(
        name(&result["assistantPrefix"]),
        "NotSupportedError",
        "a valid assistant continuation is an unsupported capability"
    );
    assert_eq!(name(&result["image"]), "NotSupportedError");
    assert_eq!(harness.generations(), 0, "failed validation never reaches generation");
    let initial = result["usage"].as_u64().unwrap();
    assert_eq!(initial, 3, "the system message costs one message plus its two words");
}

// Conversation state.

#[tokio::test]
async fn append_commits_history_without_generating() {
    let mut harness = Harness::granted();
    harness.engine.script(Script::text(["remembered"]));
    let result = harness
        .run(
            r#"
        const session = await LanguageModel.create();
        await session.append([
            { role: 'user', content: 'Remember kelp.' },
            { role: 'assistant', content: 'Noted.' },
        ]);
        const afterAppend = session.contextUsage;
        const answer = await session.prompt('What did I say?');
        return { afterAppend, answer, afterPrompt: session.contextUsage };
        "#,
        )
        .await;
    assert_eq!(harness.generations(), 1, "append never generates");
    assert_eq!(result["answer"], "remembered");
    assert_eq!(result["afterAppend"], 5, "two messages carrying three words cost 2 + 3");
    let requests = harness.engine.requests();
    // Appending records no request of its own: it commits history and generates nothing.
    let [request] = requests.as_slice() else {
        panic!("only the prompt reaches the model: {} requests", requests.len());
    };
    assert_eq!(request.committed.len(), 2, "the appended turns are committed history");
    assert_eq!(request.committed[1].role, Role::Assistant);
    assert!(result["afterPrompt"].as_u64().unwrap() > result["afterAppend"].as_u64().unwrap());
}

#[tokio::test]
async fn measuring_context_usage_changes_nothing() {
    let mut harness = Harness::granted();
    let result = harness
        .run(
            r#"
        const session = await LanguageModel.create();
        const before = session.contextUsage;
        const measured = await session.measureContextUsage('three little words');
        return { before, measured, after: session.contextUsage };
        "#,
        )
        .await;
    assert_eq!(result["measured"], 4, "one message plus three words");
    assert_eq!(result["before"], result["after"]);
    assert_eq!(harness.engine.requests().len(), 0, "measurement commits nothing");
}

/// Measuring validates its input as a conversation of its own: the draft's measurement
/// steps pass `hasAppendedInput` as false, so a system message is measurable even once the
/// session has history. Committing one still follows the conversation rule.
#[tokio::test]
async fn measuring_a_system_message_is_allowed_on_a_session_with_history() {
    let mut harness = Harness::granted();
    let result = harness
        .run(
            r#"
        const session = await LanguageModel.create();
        await session.append('remember this');
        const usage = session.contextUsage;
        const measured = await session.measureContextUsage([
            { role: 'system', content: 'be brief' },
        ]);
        return {
            measured,
            unchanged: session.contextUsage === usage,
            appended: await caught(session.append([{ role: 'system', content: 'be brief' }])),
        };
        "#,
        )
        .await;
    assert!(result["measured"].as_u64().expect("a measurement") > 0);
    assert_eq!(result["unchanged"], true, "measuring changes nothing");
    assert_eq!(
        name(&result["appended"]),
        "TypeError",
        "committing a system message after history is still refused"
    );
}

#[tokio::test]
async fn operations_run_in_invocation_order() {
    let mut harness = Harness::granted();
    harness.engine.script(Script::text(["first"]));
    harness.engine.script(Script::text(["second"]));
    let result = harness
        .run(
            r#"
        const session = await LanguageModel.create();
        const order = [];
        const operations = [
            session.append('alpha one').then(() => order.push('append-alpha')),
            session.prompt('beta two').then(() => order.push('prompt-beta')),
            session.append('gamma three').then(() => order.push('append-gamma')),
            session.prompt('delta four').then(() => order.push('prompt-delta')),
        ];
        await Promise.all(operations);
        return { order };
        "#,
        )
        .await;
    assert_eq!(
        result["order"],
        json!(["append-alpha", "prompt-beta", "append-gamma", "prompt-delta"])
    );
    let requests = harness.engine.requests();
    let prompts: Vec<&str> =
        requests.iter().map(|request| request.prompt.content.as_str()).collect();
    assert_eq!(prompts, ["beta two", "delta four"], "only the generations reach the model");
    // The appends between them are history by the time the second generation runs, in the
    // order they were invoked.
    let committed: Vec<&str> = requests
        .last()
        .expect("the second generation ran")
        .committed
        .iter()
        .map(|entry| entry.content.as_str())
        .collect();
    assert_eq!(committed, ["alpha one", "beta two", "first", "gamma three"]);
}

/// Every kind of session operation, dozens of them invoked back to back, hands its result
/// over in the order it was invoked, whatever order the isolate's op driver finishes them
/// in. Without the order the session keeps, this test fails only some of the time; the gate
/// it relies on is pinned directly below.
#[tokio::test]
async fn many_interleaved_operations_settle_in_invocation_order() {
    let mut harness = Harness::granted();
    for answer in ["a", "b", "c", "d", "e", "f", "g", "h"] {
        harness.engine.script(Script::text([answer]));
    }
    let result = harness
        .run(
            r#"
        const session = await LanguageModel.create();
        const order = [];
        const operations = [];
        const record = label => () => order.push(label);
        for (let round = 0; round < 8; round += 1) {
            operations.push(session.append(`append ${round}`).then(record(`append-${round}`)));
            operations.push(
                session.measureContextUsage(`measure ${round}`).then(record(`measure-${round}`)),
            );
            operations.push(session.prompt(`prompt ${round}`).then(record(`prompt-${round}`)));
            operations.push(
                session.clone().then(clone => {
                    clone.destroy();
                    order.push(`clone-${round}`);
                }),
            );
        }
        await Promise.all(operations);
        session.destroy();
        return { order };
        "#,
        )
        .await;

    let expected: Vec<String> = (0..8)
        .flat_map(|round| {
            ["append", "measure", "prompt", "clone"].map(|operation| format!("{operation}-{round}"))
        })
        .collect();
    assert_eq!(result["order"], json!(expected));
}

/// What keeps that order: an operation's turn to hand its result over does not arrive until
/// the operation invoked before it has settled, or been abandoned with its isolate.
#[test]
fn an_operations_turn_waits_for_the_one_invoked_before_it() {
    use std::future::Future;

    let order = super::state::Order::default();
    let first = order.ticket();
    let second = order.ticket();
    let third = order.ticket();
    let mut context = std::task::Context::from_waker(std::task::Waker::noop());

    {
        let mut waiting = std::pin::pin!(second.turn());
        assert!(
            waiting.as_mut().poll(&mut context).is_pending(),
            "the second operation waits for the first"
        );
        drop(first);
        assert!(waiting.as_mut().poll(&mut context).is_ready(), "and goes once the first has");
    }

    let mut last = std::pin::pin!(third.turn());
    assert!(last.as_mut().poll(&mut context).is_pending(), "the third waits for the second");
    drop(second);
    assert!(last.as_mut().poll(&mut context).is_ready(), "and goes once the second has");
}

#[tokio::test]
async fn different_sessions_progress_independently() {
    let mut harness = Harness::granted();
    harness.engine.script(Script::hang());
    harness.engine.script(Script::text(["unblocked"]));
    let result = harness
        .run(
            r#"
        const blocked = await LanguageModel.create();
        const free = await LanguageModel.create();
        const stuck = caught(blocked.prompt('hangs'));
        const answer = await free.prompt('independent');
        blocked.destroy();
        return { answer, stuck: (await stuck).failed.name };
        "#,
        )
        .await;
    assert_eq!(
        result["answer"], "unblocked",
        "one session's queued work never blocks another session"
    );
    assert_eq!(result["stuck"], "AbortError");
}

#[tokio::test]
async fn a_clone_owns_its_history_and_lifetime() {
    let mut harness = Harness::granted();
    harness.engine.script(Script::text(["clone answer"]));
    let result = harness
        .run(
            r#"
        const session = await LanguageModel.create();
        await session.append('shared history');
        const branch = await session.clone();
        const usages = { source: session.contextUsage, branch: branch.contextUsage };
        session.destroy();
        const answer = await branch.prompt('still alive');
        return { usages, answer, sourceFails: await caught(session.prompt('gone')) };
        "#,
        )
        .await;
    assert_eq!(result["usages"]["branch"], result["usages"]["source"], "a clone copies usage");
    assert_eq!(result["answer"], "clone answer");
    assert_eq!(name(&result["sourceFails"]), "AbortError");
    let request = harness.engine.requests().last().unwrap().clone();
    assert_eq!(request.committed[0].content, "shared history", "the clone kept the history");
}

/// The pinned draft gives `clone()`'s signal the cloning operation and nothing else: not the
/// source, and not the destruction of a clone that completes.
#[tokio::test]
async fn a_clone_signal_cancels_only_the_cloning_operation() {
    let mut harness = Harness::granted();
    harness.engine.script(Script::text(["source still works"]));
    let result = harness
        .run(
            r#"
        const session = await LanguageModel.create();
        const reason = new Error('no clone');
        const cloning = caught(session.clone({ signal: AbortSignal.abort(reason) }));
        return { cloning: await cloning, answer: await session.prompt('after') };
        "#,
        )
        .await;
    assert_eq!(name(&result["cloning"]), "Error");
    assert_eq!(result["answer"], "source still works", "the source is unaffected");
}

/// Cloning that its signal abandons leaves no session behind, however late the abort lands.
///
/// Each operation is followed by another command on the same session, and a session answers
/// its commands in order, so the follower's native count proves the operation before it had
/// already replied and is only waiting to be delivered. With the cloning's reply waiting
/// first and the other session's bare operation second, the isolate usually delivers them
/// together and the abort runs from the second reply's reaction one step after the clone
/// arrives: the moment between a clone arriving and this operation deciding what to do with
/// it. Whether the isolate delivers them together is not ours to decide, so the scenario is
/// repeated: however the deliveries fall, no attempt may leave a session behind.
#[tokio::test]
async fn a_clone_abandoned_as_it_arrives_leaves_no_session_behind() {
    const ATTEMPTS: usize = 20;

    let mut harness = Harness::granted();
    let counted =
        |log: &[Event]| log.iter().filter(|event| matches!(event, Event::Counted(_))).count();
    for attempt in 0..ATTEMPTS {
        harness
            .run(
                r#"
        globalThis.source = await LanguageModel.create();
        globalThis.cancellation = ops.op_bento_prompt_cancellation();
        globalThis.other = (await ops.op_bento_prompt_create({}, cancellation)).value;
        return null;
        "#,
            )
            .await;
        let before_cloning = counted(&harness.engine.log());
        // Started without running the event loop, so nothing is delivered until every reply
        // is waiting.
        harness.sync(
            r#"
        globalThis.controller = new AbortController();
        globalThis.reason = { stop: 'cloning' };
        globalThis.cloning = caught(source.clone({ signal: controller.signal }));
        globalThis.afterCloning = caught(source.append('after the cloning'));
        return null;
        "#,
        );
        tokio::time::timeout(
            Duration::from_secs(5),
            harness.engine.wait_for(move |log| counted(log) > before_cloning),
        )
        .await
        .expect("the command after the cloning counted, so the cloning had already replied");
        let before_noise = counted(&harness.engine.log());
        harness.sync(
            r#"
        globalThis.noise = ops
            .op_bento_prompt_append(other, [{ role: 'user', content: 'noise' }], cancellation)
            .then(() => controller.abort(reason));
        globalThis.afterNoise = ops.op_bento_prompt_append(
            other,
            [{ role: 'user', content: 'after the noise' }],
            cancellation,
        );
        return null;
        "#,
        );
        tokio::time::timeout(
            Duration::from_secs(5),
            harness.engine.wait_for(move |log| counted(log) > before_noise + 1),
        )
        .await
        .expect("the command after the other session's operation counted, so that one replied");
        let result = harness
            .run(
                r#"
        await Promise.all([cloning, afterCloning, noise, afterNoise]);
        const arrived = (await cloning).settled;
        // A clone this operation did receive is this test's to dispose of.
        if (arrived !== undefined) {
            arrived.destroy();
        }
        source.destroy();
        ops.op_bento_prompt_destroy(other);
        return {
            abandoned: arrived === undefined,
            rejected: (await cloning).failed !== undefined,
            identical: globalThis.lastError === reason,
        };
        "#,
            )
            .await;
        // What the clone's arrival still owes the isolate is settled before the next attempt.
        assert!(
            harness.drains_within(Duration::from_secs(5)).await,
            "attempt {attempt}: the event loop finishes"
        );
        tokio::time::timeout(Duration::from_secs(5), harness.cleanup.settle())
            .await
            .unwrap_or_else(|_| {
                panic!("attempt {attempt}: every session this isolate opened is disposable")
            });

        assert_eq!(
            result["abandoned"], result["rejected"],
            "attempt {attempt}: a cloning either hands out its clone or rejects, and rejecting hands out nothing: {result}"
        );
        if result["rejected"] == true {
            assert_eq!(
                result["identical"], true,
                "attempt {attempt}: it rejects with its own abort reason"
            );
        }
        let log = harness.engine.log();
        let opened = log.iter().filter(|event| matches!(event, Event::Opened(_))).count();
        let released = log.iter().filter(|event| matches!(event, Event::Released)).count();
        assert_eq!(opened, 3 * (attempt + 1), "attempt {attempt}: two sessions and a clone");
        assert_eq!(
            released, opened,
            "attempt {attempt}: a clone nobody received is released, not left running"
        );
    }
}

/// The same abandonment, with the abort landing while the clone's result waits to be
/// delivered: the cloning hands out nothing and the clone it was given is destroyed.
#[tokio::test]
async fn a_clone_whose_signal_aborted_before_delivery_is_destroyed() {
    let mut harness = Harness::granted();
    harness.run("globalThis.source = await LanguageModel.create(); return null;").await;
    let counted =
        |log: &[Event]| log.iter().filter(|event| matches!(event, Event::Counted(_))).count();
    let before_cloning = counted(&harness.engine.log());
    harness.sync(
        r#"
        globalThis.controller = new AbortController();
        globalThis.reason = { stop: 'cloning' };
        globalThis.cloning = caught(source.clone({ signal: controller.signal }));
        globalThis.afterCloning = caught(source.append('after the cloning'));
        return null;
        "#,
    );
    tokio::time::timeout(
        Duration::from_secs(5),
        harness.engine.wait_for(move |log| counted(log) > before_cloning),
    )
    .await
    .expect("the command after the cloning counted, so the cloning had already replied");
    // The event loop has not run since, so the clone's result is still waiting.
    harness.sync("controller.abort(reason); return null;");

    let result = harness
        .run(
            r#"
        await Promise.all([cloning, afterCloning]);
        source.destroy();
        return {
            handedOut: (await cloning).settled !== undefined,
            identical: globalThis.lastError === reason,
        };
        "#,
        )
        .await;

    assert!(harness.drains_within(Duration::from_secs(5)).await, "the event loop finishes");
    tokio::time::timeout(Duration::from_secs(5), harness.cleanup.settle())
        .await
        .expect("every session this isolate opened is disposable");
    assert_eq!(result["handedOut"], false, "an abandoned cloning hands out nothing");
    assert_eq!(result["identical"], true, "it rejects with its own abort reason");
    let log = harness.engine.log();
    let opened = log.iter().filter(|event| matches!(event, Event::Opened(_))).count();
    let released = log.iter().filter(|event| matches!(event, Event::Released)).count();
    assert_eq!(opened, 2, "the session and the clone it made both reached the model");
    assert_eq!(released, opened, "the clone nobody received is released");
}

/// Guest code can make handing a result over fail, by tampering with what the interfaces it
/// is given are built from. The clone that arrived is destroyed and the caller hears what
/// went wrong, rather than waiting on a promise nothing will ever settle.
#[tokio::test]
async fn a_clone_that_cannot_be_handed_over_is_destroyed_and_reported() {
    let mut harness = Harness::granted();
    let result = harness
        .run(
            r#"
        const session = await LanguageModel.create();
        const brand = Object.getOwnPropertySymbols(session).find(
            symbol => session[symbol] === symbol,
        );
        globalThis.explosion = new Error('adoption exploded');
        Object.defineProperty(LanguageModel.prototype, brand, {
            set() { throw explosion; },
            configurable: true,
        });
        const cloning = await caught(session.clone());
        session.destroy();
        return { cloning, identical: globalThis.lastError === explosion };
        "#,
        )
        .await;
    assert!(result["cloning"]["failed"].is_object(), "the cloning rejects: {result}");
    assert_eq!(result["identical"], true, "with what the guest's own code threw");
    assert!(harness.drains_within(Duration::from_secs(5)).await, "the event loop finishes");
    tokio::time::timeout(Duration::from_secs(5), harness.cleanup.settle())
        .await
        .expect("every session this isolate opened is disposable");
    let log = harness.engine.log();
    let opened = log.iter().filter(|event| matches!(event, Event::Opened(_))).count();
    let released = log.iter().filter(|event| matches!(event, Event::Released)).count();
    assert_eq!(opened, 2, "the session and the clone it made both reached the model");
    assert_eq!(released, opened, "a clone nobody could receive is released");
}

/// A clone is one of its session's ordered operations, so it hands its result over before
/// the operations invoked after it.
#[tokio::test]
async fn a_clone_settles_before_the_operations_invoked_after_it() {
    let mut harness = Harness::granted();
    harness.run("globalThis.session = await LanguageModel.create(); return null;").await;
    harness.sync(
        r#"
        globalThis.order = [];
        globalThis.operations = [
            session.clone().then(clone => { order.push('clone'); clone.destroy(); }),
            session.append('later').then(() => order.push('append')),
            session.measureContextUsage('x').then(() => order.push('measure')),
        ];
        return null;
        "#,
    );
    let result =
        harness.run("await Promise.all(operations); session.destroy(); return { order };").await;

    assert_eq!(result["order"], json!(["clone", "append", "measure"]));
}

// Streaming.

#[tokio::test]
async fn text_streaming_returns_immediately_and_emits_deltas() {
    let mut harness = Harness::granted();
    harness.engine.script(Script::text(["He", "Hell", "Hello"]));
    let result = harness
        .run(
            r#"
        const session = await LanguageModel.create();
        const stream = session.promptStreaming('greet');
        const synchronous = { isStream: stream instanceof ReadableStream, generated: 0 };
        const { chunks } = await collect(stream);
        return { ...synchronous, chunks, joined: chunks.join('') };
        "#,
        )
        .await;
    assert_eq!(result["isStream"], true);
    assert_eq!(result["chunks"], json!(["He", "ll", "o"]), "chunks are append-only deltas");
    assert_eq!(result["joined"], "Hello");
}

#[tokio::test]
async fn a_rewritten_prefix_errors_the_stream() {
    let mut harness = Harness::granted();
    harness.engine.script(Script::text(["Hello", "Goodbye"]));
    let result = harness
        .run(
            r#"
        const session = await LanguageModel.create();
        const stream = session.promptStreaming('greet');
        return await caught(collect(stream));
        "#,
        )
        .await;
    assert_eq!(name(&result), "UnknownError", "a replacement is never appended as a delta");
}

#[tokio::test]
async fn stream_transport_stays_bounded_when_the_consumer_stops_reading() {
    let mut harness = Harness::granted();
    // Far more steps than any buffer: a consumer that reads once and stops must leave most
    // of them unproduced, however long the answer would have been.
    let snapshots: Vec<String> = (1..=24).map(|length| "x".repeat(length)).collect();
    let steps: Vec<&str> = snapshots.iter().map(String::as_str).collect();
    harness.engine.script(Script::text(steps));
    let result = harness
        .run(
            r#"
        const session = await LanguageModel.create();
        const stream = session.promptStreaming('long answer');
        const reader = stream.getReader();
        const first = await reader.read();
        await new Promise(resolve => setTimeout(resolve, 10));
        await reader.cancel();
        return { first: first.value };
        "#,
        )
        .await;
    assert_eq!(result["first"], "x");
    let consumed = harness.engine.consumed_steps();
    assert!(
        consumed <= 8,
        "a stopped consumer must not drain the generation: {consumed} of {} steps",
        snapshots.len()
    );
}

#[tokio::test]
async fn cancelling_a_reader_stops_generation_without_an_error() {
    let mut harness = Harness::granted();
    harness.engine.script(Script::text_then_hang(["partial"]));
    harness.engine.script(Script::text(["next answer"]));
    let result = harness
        .run(
            r#"
        const session = await LanguageModel.create();
        const reader = session.promptStreaming('start').getReader();
        const first = await reader.read();
        const cancelled = await caught(reader.cancel());
        const answer = await session.prompt('afterwards');
        return { first: first.value, cancelled, answer };
        "#,
        )
        .await;
    assert_eq!(result["first"], "partial");
    assert!(result["cancelled"]["settled"].is_null(), "cancellation is not an error");
    assert_eq!(result["answer"], "next answer", "the session is reusable after settlement");
    assert!(
        harness.engine.log().iter().any(|event| matches!(event, Event::Cancelled(_))),
        "native generation must be cancelled"
    );
}

#[tokio::test]
async fn destroying_a_session_errors_a_stream_nobody_is_draining() {
    let mut harness = Harness::granted();
    harness.engine.script(Script::text(["a", "ab", "abc", "abcd", "abcde", "abcdef"]));
    let result = harness
        .run(
            r#"
        const session = await LanguageModel.create();
        const reader = session.promptStreaming('long answer').getReader();
        const first = await reader.read();
        // Stop reading, so the transport is holding what it can.
        await new Promise(resolve => setTimeout(resolve, 10));
        session.destroy();
        return { first: first.value, failed: await caught(reader.read()) };
        "#,
        )
        .await;
    assert_eq!(result["first"], "a");
    assert_eq!(
        name(&result["failed"]),
        "AbortError",
        "destruction is an abort even when the transport could only close the stream"
    );
}

// Aborts and destruction.

/// Aborting is an algorithm, not a notification. Dispatching an `abort` event at a signal
/// leaves it unaborted, so the session that signal created is untouched and still usable.
#[tokio::test]
async fn a_dispatched_abort_event_does_not_destroy_the_session_it_never_aborted() {
    let mut harness = Harness::granted();
    harness.engine.script(Script::text(["still here"]));
    let result = harness
        .run(
            r#"
        const controller = new AbortController();
        const session = await LanguageModel.create({ signal: controller.signal });
        const delivered = controller.signal.dispatchEvent(new Event('abort'));
        return {
            delivered,
            aborted: controller.signal.aborted,
            answer: await session.prompt('hello'),
            window: session.contextWindow,
        };
        "#,
        )
        .await;
    assert_eq!(result["delivered"], true, "the event is an ordinary event and is dispatched");
    assert_eq!(result["aborted"], false, "dispatching an event does not abort a signal");
    assert_eq!(result["answer"], "still here", "the session its signal never aborted still works");
    assert!(result["window"].as_f64().unwrap_or_default() > 0.0, "the session is still live");
    assert!(
        !harness.engine.log().iter().any(|event| matches!(event, Event::Released)),
        "nothing released a session whose signal never aborted: {:?}",
        harness.engine.log()
    );
}

/// The same boundary for one operation: a dispatched `abort` event cancels nothing, and the
/// operation goes on until something actually aborts it.
#[tokio::test]
async fn a_dispatched_abort_event_does_not_cancel_an_operation() {
    let mut harness = Harness::granted();
    harness.engine.script(Script::hang());
    let result = harness
        .run(
            r#"
        const session = await LanguageModel.create();
        const controller = new AbortController();
        const generating = session.prompt('wait', { signal: controller.signal });
        let state = 'pending';
        generating.then(
            () => { state = 'resolved'; },
            error => { state = `rejected with ${error?.name ?? String(error)}`; },
        );
        controller.signal.dispatchEvent(new Event('abort'));
        await Promise.resolve();
        await Promise.resolve();
        const afterDispatch = state;
        const aborted = controller.signal.aborted;
        session.destroy();
        await Promise.resolve();
        await Promise.resolve();
        return { afterDispatch, aborted, afterDestroy: state };
        "#,
        )
        .await;
    assert_eq!(result["aborted"], false, "dispatching an event does not abort a signal");
    assert_eq!(
        result["afterDispatch"], "pending",
        "an operation nobody aborted keeps running, whatever was dispatched at its signal"
    );
    assert_eq!(
        result["afterDestroy"], "rejected with AbortError",
        "destruction, which does abort, is what settles it"
    );
}

#[tokio::test]
async fn an_already_aborted_signal_throws_synchronously_only_for_streaming() {
    let mut harness = Harness::granted();
    harness.run("globalThis.session = await LanguageModel.create();").await;
    let thrown = harness.sync(
        r#"
        globalThis.reason = { already: 'aborted' };
        try {
            session.promptStreaming('x', { signal: AbortSignal.abort(reason) });
            return { threw: false };
        } catch (error) {
            return { threw: true, identical: error === reason };
        }
        "#,
    );
    assert_eq!(thrown, json!({ "threw": true, "identical": true }));
    let result = harness
        .run(
            r#"
        const rejected = await caught(session.prompt('x', { signal: AbortSignal.abort(reason) }));
        return { rejected: rejected.failed !== undefined, identical: lastError === reason };
        "#,
        )
        .await;
    assert_eq!(result["rejected"], true, "promise-returning operations reject instead");
    assert_eq!(result["identical"], true);
    assert_eq!(harness.generations(), 0);
}

/// Accepting a response constraint reads guest code, which can abort the operation or
/// destroy the session before anything has been asked of the model. Either way the stream
/// never starts, exactly as for a signal that was already aborted.
#[tokio::test]
async fn a_stream_answers_an_abort_that_lands_while_its_constraint_is_read() {
    let mut harness = Harness::granted();
    harness.engine.script(Script::structured([r#""kelp""#]));
    harness.engine.script(Script::structured([r#""kelp""#]));
    let result = harness
        .run(
            r#"
        const aborting = await LanguageModel.create();
        const destroying = await LanguageModel.create();
        const controller = new AbortController();
        const reason = { stopped: 'inside the getter' };
        const run = (session, constraint, options = {}) => {
            try {
                session.promptStreaming('hi', { ...options, responseConstraint: constraint });
                return { threw: null };
            } catch (error) {
                return { threw: failure(error), identical: error === reason };
            }
        };
        return {
            aborted: run(
                aborting,
                { get type() { controller.abort(reason); return 'string'; } },
                { signal: controller.signal },
            ),
            destroyed: run(destroying, {
                get type() { destroying.destroy(); return 'string'; },
            }),
            usage: aborting.contextUsage,
        };
        "#,
        )
        .await;
    assert_eq!(result["aborted"]["identical"], true, "the abort reason itself is thrown");
    assert_eq!(
        result["destroyed"]["threw"]["name"], "AbortError",
        "a session destroyed in the getter answers with its destruction reason"
    );
    assert_eq!(result["usage"], 0, "an abandoned operation commits nothing");
    assert_eq!(harness.generations(), 0, "and asks the model for nothing");
}

/// The draft's aggregated result checks the operation's abort before the operation prefills,
/// and prefilling is what validates and canonicalizes the prompt. A destroyed session and an
/// already-aborted signal therefore answer with their own reason, whatever the input is.
#[tokio::test]
async fn an_aborted_operation_answers_with_its_reason_before_validating_input() {
    let mut harness = Harness::granted();
    let result = harness
        .run(
            r#"
        const session = await LanguageModel.create();
        globalThis.reason = { stopped: 'before validation' };
        const unsupported = [{ role: 'user', content: [{ type: 'image', value: 'x' }] }];
        const aborted = () => ({ signal: AbortSignal.abort(reason) });
        const answers = {};
        answers.prompt = await caught(session.prompt(unsupported, aborted()));
        answers.promptIdentical = lastError === reason;
        answers.constraint = await caught(
            session.prompt('ok', { ...aborted(), responseConstraint: { type: 'unknown' } }),
        );
        answers.constraintIdentical = lastError === reason;
        answers.append = await caught(session.append(unsupported, aborted()));
        answers.appendIdentical = lastError === reason;
        session.destroy();
        answers.destroyedPrompt = await caught(session.prompt(unsupported));
        answers.destroyedAppend = await caught(session.append(unsupported));
        return answers;
        "#,
        )
        .await;
    for operation in ["prompt", "constraint", "append"] {
        assert!(
            result[operation]["failed"].is_object(),
            "an aborted operation rejects: {}",
            result[operation]
        );
        assert_eq!(
            result[format!("{operation}Identical").as_str()],
            true,
            "{operation} rejects with the abort reason itself, not with an input failure: {}",
            result[operation]
        );
    }
    assert_eq!(name(&result["destroyedPrompt"]), "AbortError");
    assert_eq!(name(&result["destroyedAppend"]), "AbortError");
    assert_eq!(harness.generations(), 0);
}

#[tokio::test]
async fn an_operation_signal_cancels_only_that_operation() {
    let mut harness = Harness::granted();
    harness.engine.script(Script::hang());
    harness.engine.script(Script::text(["later"]));
    harness
        .run(
            r#"
        globalThis.session = await LanguageModel.create();
        globalThis.controller = new AbortController();
        globalThis.reason = Symbol('stop');
        globalThis.pending = caught(session.prompt('hangs', { signal: controller.signal }));
        "#,
        )
        .await;
    // The generation is under way, so the abort below is what ends it.
    harness.reached(|event| matches!(event, Event::Generated(_))).await;
    let result = harness
        .run(
            r#"
        controller.abort(reason);
        await pending;
        return {
            identical: lastError === reason,
            answer: await session.prompt('later'),
        };
        "#,
        )
        .await;
    assert_eq!(result["identical"], true, "an arbitrary abort reason keeps its identity");
    assert_eq!(result["answer"], "later", "the session survives a cancelled operation");
}

#[tokio::test]
async fn cancelled_generation_retains_its_accepted_input() {
    let mut harness = Harness::granted();
    harness.engine.script(Script::text_then_hang(["partial"]));
    harness.engine.script(Script::text(["after"]));
    harness
        .run(
            r#"
        globalThis.session = await LanguageModel.create();
        globalThis.controller = new AbortController();
        globalThis.pending = caught(
            session.prompt('remember this', { signal: controller.signal }),
        );
        "#,
        )
        .await;
    harness.reached(|event| matches!(event, Event::Generated(_))).await;
    let failed = harness
        .run(
            r#"
        controller.abort();
        return (await pending).failed.name;
        "#,
        )
        .await;
    assert_eq!(failed, "AbortError");
    // An aborted operation rejects at once, while the model is still being put back on the
    // conversation it accepted; the accounting below is what that turn left behind.
    harness.reached(|event| matches!(event, Event::Restored(_))).await;
    let result = harness
        .run(
            r#"
        const usage = session.contextUsage;
        await session.prompt('next');
        return { usage };
        "#,
        )
        .await;
    assert_eq!(result["usage"], 3, "the prefilled input stays committed");
    let last = harness.engine.requests().last().unwrap().clone();
    assert_eq!(last.committed.len(), 1, "no partial assistant response is committed");
    assert_eq!(last.committed[0].content, "remember this");
}

#[tokio::test]
async fn destroy_is_idempotent_and_cancels_pending_work() {
    let mut harness = Harness::granted();
    harness.engine.script(Script::hang());
    let result = harness
        .run(r#"
        const session = await LanguageModel.create();
        const pending = caught(session.prompt('hangs'));
        await new Promise(resolve => setTimeout(resolve, 5));
        session.destroy();
        session.destroy();
        return {
            pending: await pending,
            later: await caught(session.prompt('too late')),
            stream: (() => { try { session.promptStreaming('x'); return null; } catch (error) { return failure(error); } })(),
            usage: session.contextUsage,
        };
        "#)
        .await;
    assert_eq!(name(&result["pending"]), "AbortError");
    assert_eq!(name(&result["later"]), "AbortError");
    assert_eq!(result["stream"]["name"], "AbortError");
    assert!(result["usage"].is_number(), "a destroyed session still reports its accounting");
}

#[tokio::test]
async fn disposal_waits_for_native_settlement_after_destruction() {
    let mut harness = Harness::granted();
    harness.engine.script(Script::hang());
    let settlement = harness.engine.hold_settlement();
    harness
        .run(
            r#"
        globalThis.session = await LanguageModel.create();
        globalThis.pending = caught(session.prompt('hangs'));
        "#,
        )
        .await;
    // Destruction only has native work to settle once the model is generating.
    harness.reached(|event| matches!(event, Event::Generated(_))).await;
    harness
        .run(
            r#"
        session.destroy();
        await pending;
        "#,
        )
        .await;
    assert!(!harness.cleanup.is_idle(), "destruction alone does not prove native settlement");
    settlement.release();
    harness.cleanup.settle().await;
    assert!(harness.cleanup.is_idle());
    assert!(harness.engine.log().iter().any(|event| matches!(event, Event::Settled(_))));
    assert!(harness.engine.log().iter().any(|event| matches!(event, Event::Released)));
}

#[tokio::test]
async fn dropped_sessions_release_their_resources() {
    let mut harness = Harness::granted();
    harness
        .run(
            r#"
        for (let index = 0; index < 3; index += 1) {
            const session = await LanguageModel.create();
            session.destroy();
        }
        "#,
        )
        .await;
    harness.cleanup.settle().await;
    let released =
        harness.engine.log().iter().filter(|event| matches!(event, Event::Released)).count();
    assert_eq!(released, 3, "every session releases its native state");
}

/// A session that does not exist yet still owns native work. Losing the isolate while
/// creation is counting or opening must not let the instance report that it disposed of
/// everything it owned.
#[tokio::test]
async fn a_creation_still_in_native_work_is_owed_cleanup() {
    let mut harness = Harness::granted();
    let hold = harness.engine.hold_open();
    harness.run("globalThis.creating = LanguageModel.create(); return null;").await;
    // The owning task runs on the crate's own runtime, so wait for the step rather than
    // assuming it has been scheduled by now.
    tokio::time::timeout(
        Duration::from_secs(5),
        harness.engine.wait_for(|log| log.iter().any(|event| matches!(event, Event::Opening))),
    )
    .await
    .expect("the native open starts, or this proves nothing");
    assert!(!harness.cleanup.is_idle(), "an unfinished creation is owed cleanup");
    let cleanup = harness.cleanup.clone();
    // The isolate goes away with the creation still inside native work.
    drop(harness);
    {
        let mut waiting = std::pin::pin!(cleanup.settle());
        let mut concurrent = std::pin::pin!(cleanup.settle());
        let mut context = std::task::Context::from_waker(std::task::Waker::noop());
        assert!(waiting.as_mut().poll(&mut context).is_pending());
        assert!(!cleanup.is_idle(), "a waiter must not hide outstanding native work");
        assert!(
            concurrent.as_mut().poll(&mut context).is_pending(),
            "every waiter must observe the same outstanding work"
        );
    }
    assert!(!cleanup.is_idle(), "dropping waiters must not forget native work");
    hold.release();
    tokio::time::timeout(Duration::from_secs(5), cleanup.settle())
        .await
        .expect("disposal completes once the native work settles");
    assert!(cleanup.is_idle());
}

/// The isolate's Tokio runtime dies with native work still live, which is what worker
/// termination does. The instance's cleanup is awaited elsewhere and must still be honest:
/// it waits, then completes — it never resolves empty.
#[test]
fn disposal_outlives_the_runtime_that_started_the_creation() {
    let cleanup = Cleanup::new();
    let isolate = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    let hold = isolate.block_on(async {
        let mut harness = Harness::with_cleanup(Access::granted(), cleanup.clone());
        let hold = harness.engine.hold_open();
        harness.run("globalThis.creating = LanguageModel.create(); return null;").await;
        tokio::time::timeout(
            Duration::from_secs(5),
            harness.engine.wait_for(|log| log.iter().any(|event| matches!(event, Event::Opening))),
        )
        .await
        .expect("the native open starts before the runtime dies");
        hold
    });
    isolate.shutdown_background();

    let instance = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    instance.block_on(async {
        assert!(
            tokio::time::timeout(Duration::from_millis(50), cleanup.settle()).await.is_err(),
            "native work outlives the runtime that started it"
        );
        assert!(!cleanup.is_idle(), "a disposal timeout must retain the outstanding work");
        hold.release();
        tokio::time::timeout(Duration::from_secs(5), cleanup.settle())
            .await
            .expect("a disposal whose runtime is gone still settles");
    });
}

// Context overflow.

#[tokio::test]
async fn an_oversized_prompt_overflows_once_without_committing() {
    let mut harness = Harness::granted();
    harness.engine.set_context_window(10);
    let result = harness
        .run(r#"
        const session = await LanguageModel.create();
        const events = [];
        session.oncontextoverflow = event => events.push(event.type);
        session.addEventListener('contextoverflow', () => events.push('listener'));
        const failed = await caught(session.prompt('one two three four five six seven eight nine ten'));
        return { failed, events, usage: session.contextUsage };
        "#)
        .await;
    assert_eq!(name(&result["failed"]), "QuotaExceededError");
    assert_eq!(
        result["events"],
        json!(["contextoverflow", "listener"]),
        "one event reaches both the handler attribute and the listener"
    );
    assert_eq!(result["usage"], 0, "an overflowing operation commits nothing");
    assert_eq!(harness.generations(), 0);
}

#[tokio::test]
async fn oversized_initial_prompts_reject_creation_without_an_event() {
    let mut harness = Harness::granted();
    harness.engine.set_context_window(4);
    let result = harness
        .run(
            r#"
        return await caught(LanguageModel.create({
            initialPrompts: [{ role: 'system', content: 'far too many words for this window' }],
        }));
        "#,
        )
        .await;
    assert_eq!(name(&result), "QuotaExceededError");
}

/// Context exhaustion is the WebIDL `QuotaExceededError` the draft names, and it says what
/// the operation needed and what the session can hold, for creation, prefilling and
/// generation alike.
#[tokio::test]
async fn context_exhaustion_reports_what_it_needed_and_what_the_window_holds() {
    let mut harness = Harness::granted();
    harness.engine.set_context_window(10);
    // Six words of answer on top of a two-word turn do not fit in ten tokens.
    harness.engine.script(Script::text(["one two three four five six"]));
    let result = harness
        .run(
            r#"
        const creation = await caught(LanguageModel.create({
            initialPrompts: [
                { role: 'user', content: 'one two three four five six seven eight nine' },
            ],
        }));
        const session = await LanguageModel.create();
        const window = session.contextWindow;
        const prefill = await caught(
            session.prompt('one two three four five six seven eight nine'),
        );
        const generation = await caught(session.prompt('say plenty'));
        return { creation, prefill, generation, window, usage: session.contextUsage };
        "#,
        )
        .await;
    let window = result["window"].as_u64().expect("a usable window");
    for operation in ["creation", "prefill", "generation"] {
        let failed = &result[operation]["failed"];
        assert_eq!(failed["name"], "QuotaExceededError", "{operation}: {failed}");
        assert_eq!(
            failed["quotaExceeded"], true,
            "{operation} reports the quota interface, not a bare DOMException: {failed}"
        );
        assert_eq!(failed["quota"].as_u64(), Some(window), "{operation} reports the window");
        let requested = failed["requested"].as_u64().expect("the operation reports what it needed");
        assert!(
            requested > window,
            "{operation} needed more than the window holds: {requested} of {window}"
        );
    }
    // The generation's own turn was committed before the response overflowed.
    assert_eq!(
        result["generation"]["failed"]["requested"].as_u64(),
        Some(window + 1),
        "the response needed one token more than the window holds"
    );
}

#[tokio::test]
async fn generation_that_exhausts_the_window_errors_the_stream_after_its_chunks() {
    let mut harness = Harness::granted();
    harness.engine.script(Script::text_then_fail(["He"], ErrorKind::QuotaExceeded));
    let result = harness
        .run(
            r#"
        const session = await LanguageModel.create();
        const events = [];
        session.oncontextoverflow = event => events.push(event.type);
        const chunks = [];
        const reader = session.promptStreaming('greet').getReader();
        let failed = null;
        try {
            for (;;) {
                const { value, done } = await reader.read();
                if (done) break;
                chunks.push(value);
            }
        } catch (error) {
            failed = failure(error);
        }
        return { chunks, failed, events };
        "#,
        )
        .await;
    assert_eq!(result["chunks"], json!(["He"]), "delivered chunks are not retracted");
    assert_eq!(result["failed"]["name"], "QuotaExceededError");
    assert_eq!(result["events"], json!(["contextoverflow"]));
}

/// A model that declines to answer — a refusal or a safety guardrail — is the draft's
/// NotReadableError, and the conversation survives it.
#[tokio::test]
async fn a_model_that_refuses_to_answer_is_a_not_readable_error() {
    let mut harness = Harness::granted();
    harness.engine.script(Script::failing(ErrorKind::NotReadable));
    harness.engine.script(Script::failing(ErrorKind::NotReadable));
    harness.engine.script(Script::text(["allowed"]));
    let result = harness
        .run(
            r#"
        const session = await LanguageModel.create();
        const prompted = await caught(session.prompt('describe the fight'));
        const streamed = await caught(collect(session.promptStreaming('describe the fight')));
        return { prompted, streamed, recovered: await session.prompt('describe the kelp') };
        "#,
        )
        .await;
    assert_eq!(name(&result["prompted"]), "NotReadableError");
    assert!(result["prompted"]["failed"]["domException"].as_bool().unwrap());
    assert_eq!(name(&result["streamed"]), "NotReadableError");
    assert_eq!(result["recovered"], "allowed", "a refusal leaves the session usable");
}

// Constrained output.

#[tokio::test]
async fn a_supported_schema_constrains_generation_and_returns_json_text() {
    let mut harness = Harness::granted();
    harness.engine.script(Script::structured(["{\"habitat\":", "{\"habitat\":\"pool\"}"]));
    let result = harness
        .run(
            r#"
        const session = await LanguageModel.create();
        const responseConstraint = {
            type: 'object',
            title: 'Answer',
            description: 'One habitat',
            properties: { habitat: { type: 'string', enum: ['pool', 'reef'] } },
            required: ['habitat'],
            additionalProperties: false,
        };
        const text = await session.prompt('pick one', { responseConstraint });
        return { text, parsed: JSON.parse(text).habitat, type: typeof text };
        "#,
        )
        .await;
    assert_eq!(result["type"], "string", "the result is JSON text, not an object");
    assert_eq!(result["parsed"], "pool");
    let request = harness.engine.requests().last().unwrap().clone();
    assert!(request.schema.is_some(), "the constraint reached the model as a schema");
    assert!(request.include_schema_in_prompt, "the schema's input is included by default");
}

/// Reading a response constraint runs guest code, and guest code may use the runtime while
/// it runs. The constraint is snapshotted in JavaScript before any native call sees it, so a
/// getter that calls into the runtime is ordinary guest code, not a call re-entering an
/// operation that is still converting its arguments.
#[tokio::test]
async fn a_constraint_getter_may_use_the_runtime_while_it_is_read() {
    let mut harness = Harness::granted();
    harness.engine.script(Script::structured([r#"{"animal":"crab"}"#]));
    let result = harness
        .run(
            r#"
        const session = await LanguageModel.create();
        let reads = 0;
        const responseConstraint = {
            type: 'object',
            properties: { animal: { type: 'string' } },
            required: ['animal'],
            get additionalProperties() {
                reads += 1;
                ops.op_bento_prompt_release(ops.op_bento_prompt_cancellation());
                return false;
            },
        };
        const answer = await session.prompt('choose an animal', { responseConstraint });
        return { answer, reads };
        "#,
        )
        .await;
    assert_eq!(result["answer"], r#"{"animal":"crab"}"#);
    assert_eq!(result["reads"], 1, "the constraint is read once, outside every native call");
}

/// A constraint that answers differently each time it is read is read once: what the call
/// accepted is what the model is asked for.
#[tokio::test]
async fn a_constraint_that_changes_between_reads_is_read_once() {
    let mut harness = Harness::granted();
    harness.engine.script(Script::structured([r#""kelp""#]));
    let result = harness
        .run(
            r#"
        const session = await LanguageModel.create();
        let reads = 0;
        const responseConstraint = {
            get type() {
                reads += 1;
                return reads === 1 ? 'string' : 'integer';
            },
        };
        const answer = await session.prompt('name the algae', { responseConstraint });
        return { answer, reads };
        "#,
        )
        .await;
    assert_eq!(result["reads"], 1);
    assert_eq!(result["answer"], r#""kelp""#);
    let request = harness.engine.requests().last().expect("one generation").clone();
    assert_eq!(
        request.schema,
        Some(json!({ "type": "string" })),
        "the model is asked for the schema the call accepted"
    );
}

/// Every keyword of the subset decides whether an answer is acceptable, rather than being
/// recorded and forgotten: each scripted response below breaks exactly one of them, and none
/// of them may reach the caller.
#[tokio::test]
async fn every_supported_schema_keyword_constrains_the_answer() {
    fn replacing(conforming: &Value, member: &str, value: Value) -> Value {
        let mut answer = conforming.clone();
        answer[member] = value;
        answer
    }

    fn without(conforming: &Value, member: &str) -> Value {
        let mut answer = conforming.clone();
        answer.as_object_mut().expect("an object answer").remove(member);
        answer
    }

    let conforming = json!({
        "name": "reef",
        "depth": 2,
        "ratio": 0.5,
        "tidal": true,
        "animals": ["crab"],
        "nested": { "kind": "rocky" },
    });
    let violations = [
        ("enum", replacing(&conforming, "name", json!("lagoon"))),
        ("string type", replacing(&conforming, "name", json!(7))),
        ("integer type", replacing(&conforming, "depth", json!(2.5))),
        ("integer minimum", replacing(&conforming, "depth", json!(-1))),
        ("integer maximum", replacing(&conforming, "depth", json!(11))),
        ("number minimum", replacing(&conforming, "ratio", json!(-0.5))),
        ("number maximum", replacing(&conforming, "ratio", json!(1.5))),
        ("boolean type", replacing(&conforming, "tidal", json!("yes"))),
        ("items", replacing(&conforming, "animals", json!([7]))),
        ("minItems", replacing(&conforming, "animals", json!([]))),
        ("maxItems", replacing(&conforming, "animals", json!(["crab", "eel", "cod", "ray"]))),
        ("required", without(&conforming, "depth")),
        ("closed object", replacing(&conforming, "salinity", json!(35))),
        ("nested required", replacing(&conforming, "nested", json!({}))),
    ];

    let mut harness = Harness::granted();
    // Every turn is charged for the constraint it carried, and the conversation keeps all of
    // them, so the window has to hold the subset once per attempt.
    harness.engine.set_context_window(4096);
    harness.engine.script(Script::structured([conforming.to_string()]));
    for (_, violation) in &violations {
        harness.engine.script(Script::structured([violation.to_string()]));
    }
    let result = harness
        .run(&format!(
            r#"
        const session = await LanguageModel.create();
        const responseConstraint = {{
            type: 'object',
            title: 'Habitat',
            description: 'Every supported keyword at once',
            properties: {{
                name: {{ type: 'string', enum: ['reef', 'pool'] }},
                depth: {{ type: 'integer', minimum: 0, maximum: 10 }},
                ratio: {{ type: 'number', minimum: 0, maximum: 1 }},
                tidal: {{ type: 'boolean' }},
                animals: {{
                    type: 'array',
                    items: {{ type: 'string' }},
                    minItems: 1,
                    maxItems: 3,
                }},
                nested: {{
                    type: 'object',
                    properties: {{ kind: {{ type: 'string', title: 'Kind' }} }},
                    required: ['kind'],
                    additionalProperties: false,
                }},
            }},
            required: ['name', 'depth'],
            additionalProperties: false,
        }};
        const attempts = [];
        for (let attempt = 0; attempt <= {attempts}; attempt += 1) {{
            attempts.push(await caught(session.prompt('describe', {{ responseConstraint }})));
        }}
        return {{ attempts }};
        "#,
            attempts = violations.len()
        ))
        .await;
    let answered = result["attempts"][0]["settled"]
        .as_str()
        .unwrap_or_else(|| panic!("the conforming answer is the caller's result: {result}"));
    assert_eq!(
        serde_json::from_str::<Value>(answered).expect("the answer is JSON text"),
        conforming
    );
    for (index, (keyword, violation)) in violations.iter().enumerate() {
        let attempt = &result["attempts"][index + 1];
        assert_eq!(attempt["failed"]["name"], "UnknownError", "{keyword}: {violation}");
    }
}

#[tokio::test]
async fn constrained_streaming_emits_one_validated_json_chunk() {
    let mut harness = Harness::granted();
    harness.engine.script(Script::structured(["{\"n\":", "{\"n\":1}"]));
    let result = harness
        .run(
            r#"
        const session = await LanguageModel.create();
        const stream = session.promptStreaming('count', {
            responseConstraint: { type: 'object', properties: { n: { type: 'integer' } } },
        });
        return await collect(stream);
        "#,
        )
        .await;
    assert_eq!(
        result["chunks"],
        json!(["{\"n\":1}"]),
        "structured snapshots are buffered into one final chunk"
    );
}

#[tokio::test]
async fn omitting_the_constraint_input_keeps_constrained_decoding() {
    let mut harness = Harness::granted();
    harness.engine.script(Script::structured(["{\"n\":1}"]));
    let result = harness
        .run(
            r#"
        const session = await LanguageModel.create();
        const responseConstraint = { type: 'object', properties: { n: { type: 'integer' } } };
        const measured = await session.measureContextUsage('count', {
            responseConstraint,
            omitResponseConstraintInput: true,
        });
        const text = await session.prompt('count', {
            responseConstraint,
            omitResponseConstraintInput: true,
        });
        return { measured, text };
        "#,
        )
        .await;
    assert_eq!(result["measured"], 2, "the omitted schema adds nothing to measurement");
    let request = harness.engine.requests().last().unwrap().clone();
    assert!(request.schema.is_some(), "decoding stays constrained");
    assert!(!request.include_schema_in_prompt);
}

/// An included schema is part of the turn that carried it, so its cost stays in the
/// conversation afterwards and a clone inherits it. Suppressing the schema's input charges
/// nothing for it.
#[tokio::test]
async fn an_included_constraint_is_charged_to_the_turn_that_carried_it() {
    let mut harness = Harness::granted();
    harness.engine.script(Script::structured(["{\"n\":1}"]));
    harness.engine.script(Script::structured(["{\"n\":1}"]));
    let result = harness
        .run(
            r#"
        const responseConstraint = { type: 'object', properties: { n: { type: 'integer' } } };
        const included = await LanguageModel.create();
        await included.prompt('count', { responseConstraint });
        const suppressed = await LanguageModel.create();
        await suppressed.prompt('count', {
            responseConstraint,
            omitResponseConstraintInput: true,
        });
        const branch = await included.clone();
        return {
            included: included.contextUsage,
            suppressed: suppressed.contextUsage,
            branch: branch.contextUsage,
        };
        "#,
        )
        .await;
    let included = result["included"].as_u64().unwrap();
    let suppressed = result["suppressed"].as_u64().unwrap();
    assert!(
        included > suppressed,
        "the schema's input is charged once the turn is committed: {included} vs {suppressed}"
    );
    assert_eq!(result["branch"], result["included"], "a clone inherits that cost");
}

#[tokio::test]
async fn only_measurement_rejects_the_constraint_flag_without_a_constraint() {
    let mut harness = Harness::granted();
    harness.engine.script(Script::text(["unconstrained"]));
    let result = harness
        .run(r#"
        const session = await LanguageModel.create();
        return {
            prompting: await caught(session.prompt('x', { omitResponseConstraintInput: true })),
            measuring: await caught(session.measureContextUsage('x', { omitResponseConstraintInput: true })),
        };
        "#)
        .await;
    assert_eq!(
        result["prompting"]["settled"], "unconstrained",
        "the pinned draft passes the flag to prefill and generates normally"
    );
    assert_eq!(name(&result["measuring"]), "TypeError");
    let request = harness.engine.requests().last().unwrap().clone();
    assert!(request.schema.is_none(), "there is no constraint to decode against");
}

#[tokio::test]
async fn unsupported_schema_features_reject_as_unsupported() {
    let mut harness = Harness::granted();
    let result = harness
        .run(r#"
        const session = await LanguageModel.create();
        const reject = async responseConstraint =>
            (await caught(session.prompt('x', { responseConstraint }))).failed.name;
        return {
            reference: await reject({ $ref: '#/definitions/thing' }),
            composition: await reject({ anyOf: [{ type: 'string' }, { type: 'integer' }] }),
            pattern: await reject({ type: 'string', pattern: '^a' }),
            nullable: await reject({ type: ['string', 'null'] }),
            openProperties: await reject({ type: 'object', additionalProperties: { type: 'string' } }),
            openByDefault: await reject({ type: 'object', properties: {}, additionalProperties: true }),
            tupleItems: await reject({ type: 'array', items: [{ type: 'string' }, { type: 'integer' }] }),
            nonStringEnum: await reject({ type: 'string', enum: [1, 2] }),
            numericEnum: await reject({ type: 'integer', enum: [1, 2] }),
            unknownKeyword: await reject({ type: 'string', format: 'email' }),
            unknownType: await reject({ type: 'struct' }),
        };
        "#)
        .await;
    for feature in [
        "reference",
        "composition",
        "pattern",
        "nullable",
        "openProperties",
        "openByDefault",
        "tupleItems",
        "nonStringEnum",
        "numericEnum",
        "unknownKeyword",
        "unknownType",
    ] {
        assert_eq!(result[feature], "NotSupportedError", "{feature}");
    }
    assert_eq!(harness.generations(), 0, "an unsupported constraint never generates");
}

#[tokio::test]
async fn malformed_supported_keywords_reject_as_type_errors() {
    let mut harness = Harness::granted();
    let result = harness
        .run(r#"
        const session = await LanguageModel.create();
        const reject = async responseConstraint =>
            (await caught(session.prompt('x', { responseConstraint }))).failed;
        const circular = { type: 'object', properties: {} };
        circular.properties.self = circular;
        return {
            minItems: await reject({ type: 'array', items: { type: 'string' }, minItems: 'three' }),
            required: await reject({ type: 'object', properties: {}, required: 'habitat' }),
            properties: await reject({ type: 'object', properties: 5 }),
            type: await reject({ type: 5 }),
            unserializable: await reject({ type: 'object', properties: { n: { type: 'integer', minimum: 1n } } }),
            circular: await reject(circular),
        };
        "#)
        .await;
    for keyword in ["minItems", "required", "properties", "type", "unserializable", "circular"] {
        assert_eq!(result[keyword]["typeError"], true, "{keyword}");
    }
}

/// One subset, one authority: what a constraint means, and which failure an unacceptable one
/// is, is decided by the same Rust translation a generation would use. The JavaScript surface
/// settles only that the schema can travel as JSON. Every refusal below is answered as the
/// call is made: a generation that never finishes holds the session's queue, so nothing that
/// settles here could have waited its turn.
#[tokio::test]
async fn constraint_refusals_follow_the_rust_subset_as_the_call_is_made() {
    let mut harness = Harness::granted();
    harness.engine.script(Script::hang());
    let result = harness
        .run(
            r#"
        const session = await LanguageModel.create();
        const held = caught(session.prompt('hangs'));
        const refuse = responseConstraint =>
            Promise.race([
                session
                    .prompt('x', { responseConstraint })
                    .then(() => 'accepted', error => failure(error).name),
                new Promise(resolve => setTimeout(() => resolve('still queued'), 2000)),
            ]);
        return {
            openedBeforeNestedShape: await refuse({
                type: 'object',
                properties: { a: { type: 5 } },
                additionalProperties: true,
            }),
            invertedItemCounts: await refuse({
                type: 'array',
                items: { type: 'string' },
                minItems: 3,
                maxItems: 1,
            }),
            invertedRange: await refuse({ type: 'integer', minimum: 5, maximum: 1 }),
            undeclaredRequired: await refuse({
                type: 'object',
                properties: { depth: { type: 'integer' } },
                required: ['habitat'],
                additionalProperties: false,
            }),
            beyondIntegerRange: await refuse({ type: 'integer', minimum: 1e19 }),
        };
        "#,
        )
        .await;
    // `additionalProperties: true` is a capability this backend lacks, whatever is wrong
    // inside the properties it opens; the rest are malformed uses of supported keywords,
    // except a bound the model's generation guides cannot carry.
    for (shape, refusal) in [
        ("openedBeforeNestedShape", "NotSupportedError"),
        ("invertedItemCounts", "TypeError"),
        ("invertedRange", "TypeError"),
        ("undeclaredRequired", "TypeError"),
        ("beyondIntegerRange", "NotSupportedError"),
    ] {
        assert_eq!(result[shape], refusal, "{shape}: {result}");
    }
    // The held prompt reaches the engine asynchronously; wait for it before listing requests.
    harness.reached(|event| matches!(event, Event::Generated(_))).await;
    let prompts: Vec<String> =
        harness.engine.requests().iter().map(|request| request.prompt.content.clone()).collect();
    assert_eq!(prompts, ["hangs"], "a refused constraint never reaches the model");
}

/// The call-time check and the generation must judge the same value. A number bound such as
/// 2^60 is exact in binary64, but its shortest JSON spelling is a different integer.
#[tokio::test]
async fn the_call_time_check_judges_the_value_generation_receives() {
    let mut harness = Harness::granted();
    harness.engine.script(Script::structured(["2e18"]));
    let result = harness
        .run(
            r#"
        const session = await LanguageModel.create();
        const responseConstraint = { type: 'number', minimum: 2 ** 60 };
        return {
            measured: await caught(session.measureContextUsage('x', { responseConstraint })),
            answered: await caught(session.prompt('x', { responseConstraint })),
        };
        "#,
        )
        .await;
    assert!(result["measured"]["settled"].is_number(), "{result}");
    assert_eq!(result["answered"]["settled"], "2e18", "{result}");
}

/// The subset's own shape rules are argument errors rather than missing capabilities: a
/// schema that declares no type, an array that declares no `items`, an
/// `additionalProperties` that is neither a boolean nor a schema, and a `required` list that
/// names a property twice are all malformed uses of supported keywords. Every
/// promise-returning entry point answers so before the conversation moves.
#[tokio::test]
async fn a_malformed_supported_schema_shape_rejects_as_a_type_error() {
    let mut harness = Harness::granted();
    let result = harness
        .run(
            r#"
        const session = await LanguageModel.create();
        const before = session.contextUsage;
        const shapes = {
            missingType: { properties: { n: { type: 'integer' } } },
            emptySchema: {},
            nestedMissingType: { type: 'object', properties: { n: {} } },
            missingItems: { type: 'array', minItems: 1 },
            nestedMissingItems: { type: 'object', properties: { xs: { type: 'array' } } },
            numericAdditionalProperties: {
                type: 'object',
                properties: { n: { type: 'integer' } },
                additionalProperties: 1,
            },
            duplicateRequired: {
                type: 'object',
                properties: { habitat: { type: 'string' }, depth: { type: 'integer' } },
                required: ['habitat', 'habitat'],
                additionalProperties: false,
            },
        };
        const prompting = {};
        for (const [shape, responseConstraint] of Object.entries(shapes)) {
            prompting[shape] = await caught(session.prompt('x', { responseConstraint }));
        }
        return {
            prompting,
            measuring: await caught(
                session.measureContextUsage('x', { responseConstraint: shapes.missingItems }),
            ),
            before,
            after: session.contextUsage,
        };
        "#,
        )
        .await;
    for shape in [
        "missingType",
        "emptySchema",
        "nestedMissingType",
        "missingItems",
        "nestedMissingItems",
        "numericAdditionalProperties",
        "duplicateRequired",
    ] {
        let outcome = &result["prompting"][shape];
        assert_eq!(outcome["failed"]["typeError"], true, "{shape}: {outcome}");
        assert_eq!(outcome["failed"]["name"], "TypeError", "{shape}");
        assert_eq!(
            outcome["failed"]["domException"], false,
            "{shape} is a bad argument, not a capability this backend lacks"
        );
    }
    let measuring = &result["measuring"];
    assert_eq!(measuring["failed"]["typeError"], true, "measurement agrees: {measuring}");
    assert_eq!(result["after"], result["before"], "a refused constraint commits nothing");
    assert_eq!(harness.generations(), 0, "a malformed constraint never generates");
    assert!(harness.engine.requests().is_empty(), "the model is never asked to generate");
}

/// `promptStreaming` judges its arguments as it is called and hands back a stream either
/// way, reporting what it refused through that stream instead of throwing. The two refusals
/// stay distinguishable there, and neither opens a generation.
#[tokio::test]
async fn streaming_separates_a_malformed_shape_from_an_unsupported_feature() {
    let mut harness = Harness::granted();
    harness.run("globalThis.session = await LanguageModel.create();").await;
    let returned = harness.sync(
        r#"
        globalThis.before = session.contextUsage;
        globalThis.streams = {};
        const open = (label, responseConstraint) => {
            try {
                streams[label] = session.promptStreaming('x', { responseConstraint });
                return { threw: null, isStream: streams[label] instanceof ReadableStream };
            } catch (error) {
                return { threw: failure(error), isStream: false };
            }
        };
        return {
            missingItems: open('missingItems', { type: 'array', minItems: 1 }),
            duplicateRequired: open('duplicateRequired', {
                type: 'object',
                properties: { habitat: { type: 'string' } },
                required: ['habitat', 'habitat'],
                additionalProperties: false,
            }),
            openByDefault: open('openByDefault', {
                type: 'object',
                properties: {},
                additionalProperties: true,
            }),
        };
        "#,
    );
    for stream in ["missingItems", "duplicateRequired", "openByDefault"] {
        assert!(returned[stream]["threw"].is_null(), "{stream} throws nothing: {returned}");
        assert_eq!(returned[stream]["isStream"], true, "{stream} still answers with a stream");
    }
    let result = harness
        .run(
            r#"
        const drain = async stream => (await caught(collect(stream))).failed;
        return {
            missingItems: await drain(streams.missingItems),
            duplicateRequired: await drain(streams.duplicateRequired),
            openByDefault: await drain(streams.openByDefault),
            before,
            after: session.contextUsage,
        };
        "#,
        )
        .await;
    for stream in ["missingItems", "duplicateRequired"] {
        let failed = &result[stream];
        assert_eq!(failed["typeError"], true, "{stream}: {failed}");
        assert_eq!(failed["name"], "TypeError", "{stream}");
    }
    assert_eq!(
        result["openByDefault"]["name"], "NotSupportedError",
        "an unsupported feature is still a capability failure in a stream"
    );
    assert_eq!(result["after"], result["before"], "a refused stream commits nothing");
    assert_eq!(harness.generations(), 0, "a refused constraint opens no generation");
    assert!(harness.engine.requests().is_empty(), "the model is never asked to generate");
}

// Isolation.

/// Two isolates, each with its own sessions. They run on their own threads, as a worker and
/// its parent do, so neither the resource identifiers nor the conversations can meet.
#[test]
fn sessions_belong_to_the_isolate_that_created_them() {
    fn isolate(name: &'static str) -> (Value, Vec<String>) {
        let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        runtime.block_on(async move {
            let mut harness = Harness::granted();
            harness.engine.script(Script::text([name]));
            let answer = harness
                .run(&format!(
                    r#"
                const session = await LanguageModel.create();
                await session.append('{name} history');
                return await session.prompt('who am i');
                "#
                ))
                .await;
            let committed = harness
                .engine
                .requests()
                .last()
                .expect("the isolate prompted")
                .committed
                .iter()
                .map(|entry| entry.content.clone())
                .collect();
            (answer, committed)
        })
    }

    let other = std::thread::spawn(|| isolate("first"));
    let (second_answer, second_committed) = isolate("second");
    let (first_answer, first_committed) = other.join().expect("the other isolate finished");

    assert_eq!(first_answer, "first");
    assert_eq!(second_answer, "second");
    assert!(
        first_committed.iter().all(|content| content != "second history"),
        "one isolate's history never reaches another: {first_committed:?}"
    );
    assert!(
        second_committed.iter().all(|content| content != "first history"),
        "one isolate's history never reaches another: {second_committed:?}"
    );
}

/// Resource identifiers are issued by, and only mean anything to, the isolate that holds
/// them. One isolate's live session identifier, handed to another isolate's ops, reaches no
/// session there and leaves the conversation it names untouched.
#[test]
fn a_session_identifier_from_another_isolate_reaches_nothing() {
    let (announce, announced) = std::sync::mpsc::channel();
    let (release, released) = std::sync::mpsc::channel();
    let owner = std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        runtime.block_on(async move {
            let mut harness = Harness::granted();
            harness.engine.script(Script::text(["owner answer"]));
            let session = harness
                .run(
                    r#"
                globalThis.cancellation = ops.op_bento_prompt_cancellation();
                const created = await ops.op_bento_prompt_create({}, cancellation);
                globalThis.rid = created.value;
                await ops.op_bento_prompt_append(
                    rid,
                    [{ role: 'user', content: 'owner history' }],
                    cancellation,
                );
                return { rid, usage: ops.op_bento_prompt_usage(rid) };
                "#,
                )
                .await;
            announce.send(session["rid"].as_u64().expect("the owner holds a session")).unwrap();
            released.recv().expect("the other isolate finished with the identifier");

            harness
                .run(
                    r#"
                return await ops.op_bento_prompt_prompt(
                    rid, [{ role: 'user', content: 'who am i' }], {}, cancellation);
                "#,
                )
                .await;
            harness
                .engine
                .requests()
                .last()
                .expect("the owner prompted")
                .committed
                .iter()
                .map(|message| message.content.clone())
                .collect::<Vec<String>>()
        })
    });

    let rid = announced.recv().expect("the owner announced its session");
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    let (reached, opened) = runtime.block_on(async move {
        let mut harness = Harness::granted();
        let reached = harness
            .run(&format!(
                r#"
            const cancellation = ops.op_bento_prompt_cancellation();
            const appended = await ops.op_bento_prompt_append(
                {rid}, [{{ role: 'user', content: 'intruder history' }}], cancellation);
            const prompted = await ops.op_bento_prompt_prompt(
                {rid}, [{{ role: 'user', content: 'who am i' }}], {{}}, cancellation);
            ops.op_bento_prompt_destroy({rid});
            return {{
                usage: String(ops.op_bento_prompt_usage({rid})),
                appended: appended.status,
                prompted: prompted.status,
            }};
            "#
            ))
            .await;
        (reached, harness.engine.requests().len())
    });
    release.send(()).expect("the owner isolate is waiting for the identifier");
    let committed = owner.join().expect("the owner isolate finished");

    assert_eq!(reached["usage"], "NaN", "another isolate's identifier names no session here");
    assert_eq!(reached["appended"], "failed");
    assert_eq!(reached["prompted"], "failed");
    assert_eq!(opened, 0, "the model is never asked anything through a foreign identifier");
    assert_eq!(
        committed,
        vec!["owner history".to_owned()],
        "the owner's conversation is only what the owner put in it"
    );
}

/// A session nobody destroyed is not work: it holds no operation, so the instance's event
/// loop drains and the guest ends. The control is the same isolate with a generation under
/// way, which does keep it running.
#[tokio::test]
async fn an_idle_session_lets_the_event_loop_finish() {
    let mut harness = Harness::granted();
    harness.engine.script(Script::text(["still here"]));
    harness.run("globalThis.session = await LanguageModel.create(); return null;").await;

    assert!(
        harness.drains_within(Duration::from_secs(5)).await,
        "an idle session kept the event loop running"
    );
    assert_eq!(
        harness.run("return await session.prompt('anyone there?');").await,
        "still here",
        "the session was neither destroyed nor released by draining the loop"
    );

    let mut busy = Harness::granted();
    busy.engine.script(Script::hang());
    busy.run(
        r#"
        globalThis.session = await LanguageModel.create();
        globalThis.pending = caught(session.prompt('hangs'));
        return null;
        "#,
    )
    .await;
    busy.reached(|event| matches!(event, Event::Generated(_))).await;
    assert!(
        !busy.drains_within(Duration::from_millis(250)).await,
        "a generation in flight is work, and work keeps the event loop running"
    );
}

/// Members the draft deprecates or Bento does not implement are absent, not present and
/// unsupported, and WebIDL ignores dictionary members it does not know.
#[tokio::test]
async fn deprecated_members_are_absent_and_unknown_dictionary_members_are_ignored() {
    let mut harness = Harness::granted();
    harness.engine.script(Script::text(["answered"]));
    let result = harness
        .run(
            r#"
        const session = await LanguageModel.create({
            topK: 3,
            temperature: 0.5,
            tools: [{ name: 'lookup' }],
            thisMemberDoesNotExist: true,
        });
        return {
            params: typeof LanguageModel.params,
            inputUsage: 'inputUsage' in session,
            inputQuota: 'inputQuota' in session,
            measureInputUsage: 'measureInputUsage' in session,
            contextUsage: 'contextUsage' in session,
            answer: await session.prompt('anything', { thisOptionDoesNotExist: true }),
        };
        "#,
        )
        .await;
    assert_eq!(result["params"], "undefined", "`params()` is not implemented, so it is absent");
    assert_eq!(result["inputUsage"], false, "the deprecated accounting name is absent");
    assert_eq!(result["inputQuota"], false, "the deprecated accounting name is absent");
    assert_eq!(result["measureInputUsage"], false, "the deprecated measurement name is absent");
    assert_eq!(result["contextUsage"], true, "the accounting the crate does implement is there");
    assert_eq!(result["answer"], "answered", "unknown dictionary members change nothing");
}

/// Tool calls and tool responses are content types this backend cannot serve: a
/// configuration expecting them is unavailable, and creating it is an unsupported capability.
#[tokio::test]
async fn tool_content_types_are_unavailable_and_reject_creation() {
    let mut harness = Harness::granted();
    let result = harness
        .run(
            r#"
        const results = {};
        for (const type of ['tool-call', 'tool-response']) {
            results[type] = {
                input: await LanguageModel.availability({ expectedInputs: [{ type }] }),
                output: await LanguageModel.availability({ expectedOutputs: [{ type }] }),
                creation: await caught(LanguageModel.create({ expectedInputs: [{ type }] })),
            };
        }
        return results;
        "#,
        )
        .await;
    for kind in ["tool-call", "tool-response"] {
        assert_eq!(result[kind]["input"], "unavailable", "{kind} input");
        assert_eq!(result[kind]["output"], "unavailable", "{kind} output");
        assert_eq!(name(&result[kind]["creation"]), "NotSupportedError", "{kind} creation");
    }
    assert!(
        harness.engine.log().is_empty(),
        "a content type this backend cannot serve is decided before the model: {:?}",
        harness.engine.log()
    );
}
