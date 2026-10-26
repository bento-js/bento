// The `LanguageModel` global: session and stream lifetime over this isolate's resources.
// Argument conversion lives in `input.mjs`, constraint acceptance in `constraint.mjs`.

import { promptRequest } from "ext:bento_prompt_deno/constraint.mjs";
import {
    canonicalizeOptions,
    canonicalizePrompt,
    coreOptionsConverter,
    createOptionsConverter,
    domError,
    notSupported,
    promptConverter,
    promptOptionsConverter,
    signalOptionsConverter,
    unsupportedType,
    webError,
} from "ext:bento_prompt_deno/input.mjs";
import { core } from "ext:core/mod.js";

let {
    op_bento_prompt_allowed,
    op_bento_prompt_availability,
    op_bento_prompt_append,
    op_bento_prompt_cancel,
    op_bento_prompt_cancellation,
    op_bento_prompt_create,
    op_bento_prompt_destroy,
    op_bento_prompt_fork,
    op_bento_prompt_measure,
    op_bento_prompt_prompt,
    op_bento_prompt_release,
    op_bento_prompt_stream,
    op_bento_prompt_stream_close,
    op_bento_prompt_stream_next,
    op_bento_prompt_usage,
    op_bento_prompt_window,
} = core.ops;

let webidl = core.loadExtScript("ext:deno_webidl/00_webidl.js");
let { Event, EventTarget, ProgressEvent, defineEventHandler } = core.loadExtScript(
    "ext:deno_web/02_event.js",
);
// `add`/`remove` subscribe to the abort algorithm itself. An `abort` event is a
// notification anyone can dispatch, not this signal's lifetime changing.
let { AbortController, add, createDependentAbortSignal, remove } = core.loadExtScript(
    "ext:deno_web/03_abort_signal.js",
);
let { ReadableStream } = core.loadExtScript("ext:deno_web/06_streams.js");
// The event loop's own task boundary, not the guest-replaceable `setTimeout`.
let { defer } = core.loadExtScript("ext:deno_web/02_timers.js");

// Only this module may construct the interfaces the draft says are not constructible.
let construct = Symbol("bento.prompt.construct");
let state = Symbol("bento.prompt.state");

// Operations

function operationSignal(session, signal) {
    let signals = [session[state].destruction.signal];
    if (signal !== undefined) {
        signals.push(signal);
    }
    return createDependentAbortSignal(signals, "Failed to run a language model operation");
}

/**
 * The draft's aggregated result: one promise, rejected with the operation's own abort reason
 * the moment its signal aborts, whatever the native side is still finishing.
 *
 * `map` turns what the operation produced into what the caller receives, and `abandon`
 * disposes of what arrived for an operation whose signal had already aborted. Both run in
 * the handler that decides the operation's outcome: an abort that lands between the result
 * arriving and that decision must not leave anything behind.
 */
function aggregated(session, signal, start, { map = value => value, abandon = null } = {}) {
    if (signal.aborted) {
        return Promise.reject(signal.reason);
    }
    let cancellation = op_bento_prompt_cancellation();
    let aborted = false;
    return new Promise((resolve, reject) => {
        let onAbort = () => {
            aborted = true;
            op_bento_prompt_cancel(cancellation);
            reject(signal.reason);
        };
        signal[add](onAbort);
        // A settled operation stops listening: its signal may outlive it by any amount.
        let settle = () => {
            signal[remove](onAbort);
            op_bento_prompt_release(cancellation);
        };
        start(cancellation).then(
            result => {
                settle();
                if (result.status === "failed") {
                    if (!aborted) {
                        reject(failureOf(session, result));
                    }
                    return;
                }
                if (aborted) {
                    if (abandon !== null) {
                        abandon(result.value);
                    }
                    return;
                }
                let mapped;
                try {
                    mapped = map(result.value);
                } catch (error) {
                    // Guest code can make handing the result over fail. Nothing it produced
                    // may stay behind, and the caller hears why rather than waiting forever.
                    if (abandon !== null) {
                        abandon(result.value);
                    }
                    reject(error);
                    return;
                }
                resolve(mapped);
            },
            error => {
                settle();
                if (!aborted) {
                    reject(error);
                }
            },
        );
    });
}

// A session that cannot fit an operation reports it once, for that operation.
function failureOf(session, result) {
    if (session !== null && result.error === "QuotaExceededError") {
        session.dispatchEvent(new Event("contextoverflow"));
    }
    return webError(result);
}

function messagesFor(session, input) {
    return canonicalizePrompt(input, session[state].usage() > 0);
}

class LanguageModel extends EventTarget {
    constructor(key, rid, cancellation, destruction) {
        if (key !== construct) {
            webidl.illegalConstructor();
        }
        super();
        this[webidl.brand] = webidl.brand;
        this[state] = {
            rid,
            cancellation,
            destruction,
            // Removes this session's hook on the creation signal, once it has one.
            detachLifetime: null,
            destroyed: false,
            lastUsage: 0,
            lastWindow: 0,
            usage: () => {
                if (!this[state].destroyed) {
                    this[state].lastUsage = op_bento_prompt_usage(this[state].rid);
                }
                return this[state].lastUsage;
            },
            window: () => {
                if (!this[state].destroyed) {
                    this[state].lastWindow = op_bento_prompt_window(this[state].rid);
                }
                return this[state].lastWindow;
            },
        };
    }

    static availability(options = {}) {
        let prefix = "Failed to execute 'availability' on 'LanguageModel'";
        let converted;
        try {
            converted = coreOptionsConverter(options, prefix, "Argument 1");
            canonicalizeOptions(converted);
        } catch (error) {
            return Promise.reject(error);
        }
        if (unsupportedType(converted)) {
            return Promise.resolve("unavailable");
        }
        return op_bento_prompt_availability(converted).then(result => {
            if (result.status === "failed") {
                throw webError(result);
            }
            return result.value;
        });
    }

    static create(options = {}) {
        let prefix = "Failed to execute 'create' on 'LanguageModel'";
        let converted;
        let initialPrompts;
        try {
            converted = createOptionsConverter(options, prefix, "Argument 1");
            canonicalizeOptions(converted);
            initialPrompts =
                converted.initialPrompts.length === 0
                    ? []
                    : canonicalizePrompt(converted.initialPrompts, false);
        } catch (error) {
            return Promise.reject(error);
        }
        let { signal } = converted;
        if (signal !== undefined && signal.aborted) {
            return Promise.reject(signal.reason);
        }
        if (!op_bento_prompt_allowed()) {
            return Promise.reject(
                domError("NotAllowedError", "The host has not granted language-model access"),
            );
        }
        let monitor = null;
        if (converted.monitor !== undefined) {
            monitor = new CreateMonitor(construct);
            // A callback function is invoked with no receiver: the dictionary it arrived in
            // is this call's own business, not something the guest named.
            let invokeMonitor = converted.monitor;
            try {
                invokeMonitor(monitor);
            } catch (error) {
                return Promise.reject(error);
            }
        }
        let request = {
            initialPrompts,
            expectedInputs: converted.expectedInputs ?? [],
            expectedOutputs: converted.expectedOutputs ?? [],
        };
        return startSession(request, monitor, signal);
    }

    prompt(input, options = {}) {
        let prefix = "Failed to execute 'prompt' on 'LanguageModel'";
        let signal;
        try {
            webidl.assertBranded(this, LanguageModel.prototype);
            webidl.requiredArguments(arguments.length, 1, prefix);
            input = promptConverter(input, prefix, "Argument 1");
            options = promptOptionsConverter(options, prefix, "Argument 2");
            signal = operationSignal(this, options.signal);
        } catch (error) {
            return Promise.reject(error);
        }
        // The draft's aggregated result checks the abort before the operation prefills, and
        // prefilling is what validates the input: an abandoned operation answers with its
        // own reason rather than with what its input would have been refused for.
        if (signal.aborted) {
            return Promise.reject(signal.reason);
        }
        let messages;
        let request;
        try {
            messages = messagesFor(this, input);
            request = promptRequest(options);
        } catch (error) {
            return Promise.reject(error);
        }
        return aggregated(this, signal, cancellation =>
            op_bento_prompt_prompt(this[state].rid, messages, request, cancellation),
        );
    }

    promptStreaming(input, options = {}) {
        let prefix = "Failed to execute 'promptStreaming' on 'LanguageModel'";
        webidl.assertBranded(this, LanguageModel.prototype);
        webidl.requiredArguments(arguments.length, 1, prefix);
        input = promptConverter(input, prefix, "Argument 1");
        options = promptOptionsConverter(options, prefix, "Argument 2");
        let signal = operationSignal(this, options.signal);
        if (signal.aborted) {
            throw signal.reason;
        }
        // Validation failures error the stream rather than throwing, as the draft prefills
        // in parallel; nothing is enqueued on the session for them.
        let messages = null;
        let request = null;
        let invalid = null;
        try {
            messages = messagesFor(this, input);
            request = promptRequest(options);
        } catch (error) {
            invalid = error;
        }
        // Accepting input reads guest code, such as getters and iterables, which may abort
        // this operation or destroy the session. An operation abandoned there starts
        // nothing, and says so the way an already-aborted signal does.
        if (signal.aborted) {
            throw signal.reason;
        }
        return streamOf(this, signal, messages, request, invalid);
    }

    append(input, options = {}) {
        let prefix = "Failed to execute 'append' on 'LanguageModel'";
        let signal;
        try {
            webidl.assertBranded(this, LanguageModel.prototype);
            webidl.requiredArguments(arguments.length, 1, prefix);
            input = promptConverter(input, prefix, "Argument 1");
            options = signalOptionsConverter(options, prefix, "Argument 2");
            signal = operationSignal(this, options.signal);
        } catch (error) {
            return Promise.reject(error);
        }
        if (signal.aborted) {
            return Promise.reject(signal.reason);
        }
        let messages;
        try {
            messages = messagesFor(this, input);
        } catch (error) {
            return Promise.reject(error);
        }
        return aggregated(this, signal, cancellation =>
            op_bento_prompt_append(this[state].rid, messages, cancellation),
        );
    }

    measureContextUsage(input, options = {}) {
        let prefix = "Failed to execute 'measureContextUsage' on 'LanguageModel'";
        let messages;
        let request;
        try {
            webidl.assertBranded(this, LanguageModel.prototype);
            webidl.requiredArguments(arguments.length, 1, prefix);
            input = promptConverter(input, prefix, "Argument 1");
            options = promptOptionsConverter(options, prefix, "Argument 2");
            if (options.omitResponseConstraintInput && options.responseConstraint === undefined) {
                throw new TypeError(
                    "omitResponseConstraintInput requires a responseConstraint to omit",
                );
            }
            // The draft's measurement steps canonicalize with no appended input, so what a
            // conversation could still accept first is measurable whatever it already holds.
            messages = canonicalizePrompt(input, false);
            request = promptRequest(options);
        } catch (error) {
            return Promise.reject(error);
        }
        let signal = operationSignal(this, options.signal);
        return aggregated(this, signal, cancellation =>
            op_bento_prompt_measure(this[state].rid, messages, request, cancellation),
        );
    }

    clone(options = {}) {
        let prefix = "Failed to execute 'clone' on 'LanguageModel'";
        try {
            webidl.assertBranded(this, LanguageModel.prototype);
            options = signalOptionsConverter(options, prefix, "Argument 1");
        } catch (error) {
            return Promise.reject(error);
        }
        let signal = operationSignal(this, options.signal);
        return aggregated(
            this,
            signal,
            cancellation => op_bento_prompt_fork(this[state].rid, cancellation),
            {
                map: rid => adopt(rid),
                // A clone that arrives after its signal aborted is nobody's session.
                abandon: rid => op_bento_prompt_destroy(rid),
            },
        );
    }

    destroy() {
        webidl.assertBranded(this, LanguageModel.prototype);
        destroy(this, domError("AbortError", "The session was destroyed"));
    }

    get contextUsage() {
        webidl.assertBranded(this, LanguageModel.prototype);
        return this[state].usage();
    }

    get contextWindow() {
        webidl.assertBranded(this, LanguageModel.prototype);
        return this[state].window();
    }
}

class CreateMonitor extends EventTarget {
    constructor(key) {
        if (key !== construct) {
            webidl.illegalConstructor();
        }
        super();
        this[webidl.brand] = webidl.brand;
    }
}

defineEventHandler(LanguageModel.prototype, "contextoverflow");
defineEventHandler(CreateMonitor.prototype, "downloadprogress");
webidl.configureInterface(LanguageModel);
webidl.configureInterface(CreateMonitor);

function adopt(rid) {
    let destruction = new AbortController();
    return new LanguageModel(construct, rid, null, destruction);
}

function destroy(session, reason) {
    let owned = session[state];
    if (owned.destroyed) {
        return;
    }
    // Read the accounting while the session still exists; it stays readable afterwards.
    owned.usage();
    owned.window();
    owned.destroyed = true;
    // A destroyed session has no lifetime left for its creation signal to control.
    if (owned.detachLifetime !== null) {
        owned.detachLifetime();
        owned.detachLifetime = null;
    }
    owned.destruction.abort(reason);
    op_bento_prompt_destroy(owned.rid);
    if (owned.cancellation !== null) {
        op_bento_prompt_release(owned.cancellation);
    }
}

/**
 * The draft's creation: one promise, rejected with the creation signal's own reason the
 * moment it aborts, whatever the native side is still finishing. What the creation started
 * stays owned until it settles, so an abort leaves behind neither a session nobody received
 * nor native work nobody is waiting for.
 */
function startSession(request, monitor, signal) {
    // The monitor callback ran just before this, and may have aborted the signal: a
    // creation given up there asks nothing of the model and waits for nothing.
    if (signal !== undefined && signal.aborted) {
        return Promise.reject(signal.reason);
    }
    if (signal === undefined) {
        return createSession(request, monitor, undefined);
    }
    return new Promise((resolve, reject) => {
        let aborted = false;
        let onAbort = () => {
            aborted = true;
            reject(signal.reason);
        };
        signal[add](onAbort);
        // A settled creation stops listening: it has nothing left for the signal to reject.
        let settle = () => signal[remove](onAbort);
        createSession(request, monitor, signal).then(
            session => {
                settle();
                // A session whose signal aborted before anyone could receive it is nobody's
                // session: the signal that governs it destroys it instead.
                if (aborted) {
                    destroy(session, signal.reason);
                    return;
                }
                resolve(session);
            },
            error => {
                settle();
                if (!aborted) {
                    reject(error);
                }
            },
        );
    });
}

async function createSession(request, monitor, signal) {
    if (unsupportedType(request)) {
        // The model is unavailable for this configuration, which creation reports as an
        // unsupported capability. No initialization endpoints are reported for it.
        throw notSupported("The model cannot serve the expected content types");
    }
    let aborted = () => signal !== undefined && signal.aborted;
    if (monitor !== null) {
        await reportInitialization(monitor, request, signal);
        if (aborted()) {
            throw signal.reason;
        }
    }
    let cancellation = op_bento_prompt_cancellation();
    let onAbort = () => op_bento_prompt_cancel(cancellation);
    if (signal !== undefined) {
        signal[add](onAbort);
    }
    // Once creation has settled one way or another, the signal no longer cancels it: a
    // signal that outlives this call must not keep driving a handle nobody holds.
    let detach = () => {
        if (signal !== undefined) {
            signal[remove](onAbort);
        }
    };
    let release = () => {
        detach();
        op_bento_prompt_release(cancellation);
    };
    let result;
    // Creation that its own signal aborted owns whatever the native side already produced:
    // the session it would have handed out is destroyed instead, and the abort reason is
    // what the caller sees, identity and all.
    let abandoned = () => {
        if (!aborted()) {
            return false;
        }
        if (result.status === "done") {
            op_bento_prompt_destroy(result.value);
        }
        release();
        return true;
    };
    try {
        result = await op_bento_prompt_create(request, cancellation);
    } catch (error) {
        release();
        throw error;
    }
    if (abandoned()) {
        throw signal.reason;
    }
    if (result.status === "failed") {
        release();
        throw webError(result);
    }
    let destruction = new AbortController();
    let session = new LanguageModel(construct, result.value, cancellation, destruction);
    detach();
    if (signal !== undefined) {
        // The creation signal keeps controlling the session it created, until the session
        // is destroyed and has no lifetime left to control.
        let onCreationAbort = () => destroy(session, signal.reason);
        signal[add](onCreationAbort);
        session[state].detachLifetime = () => signal[remove](onCreationAbort);
    }
    return session;
}

/**
 * The shared algorithm's two initialization endpoints, reported before the initialization
 * they describe and only for a model that is ready to be initialized. They are not evidence
 * of a download: Bento never downloads model assets.
 */
async function reportInitialization(monitor, request, signal) {
    let readiness = await op_bento_prompt_availability(request);
    if (signal !== undefined && signal.aborted) {
        return;
    }
    if (readiness.status === "failed") {
        // Readiness that could not be determined is this creation's answer. It is neither
        // decided here nor asked for again.
        throw webError(readiness);
    }
    if (readiness.value !== "available") {
        throw notSupported("The model is unavailable");
    }
    // Separate tasks let a listener's microtasks abort before the next endpoint.
    await queueProgress(monitor, 0, signal);
    await queueProgress(monitor, 1, signal);
}

async function queueProgress(monitor, loaded, signal) {
    await new Promise(resolve => defer(resolve));
    // The queued task reports nothing once the creation it describes has been abandoned.
    if (signal !== undefined && signal.aborted) {
        return;
    }
    monitor.dispatchEvent(
        new ProgressEvent("downloadprogress", { loaded, total: 1, lengthComputable: true }),
    );
}

function streamOf(session, signal, messages, request, invalid) {
    let cancellation = op_bento_prompt_cancellation();
    let generation = null;
    let started = null;
    let finished = false;
    // Set once the stream subscribes to its signal's abort algorithm.
    let detach = () => {};
    let release = () => {
        if (finished) {
            return;
        }
        finished = true;
        detach();
        if (generation !== null) {
            op_bento_prompt_stream_close(generation);
        } else if (started !== null) {
            // A generation that opens after the consumer left belongs to nobody; close it
            // so the session is not left waiting on it.
            started.then(
                opened => {
                    if (opened.status === "done") {
                        op_bento_prompt_stream_close(opened.value);
                    }
                },
                () => {},
            );
        }
        op_bento_prompt_release(cancellation);
    };
    return new ReadableStream(
        {
            start(controller) {
                if (invalid !== null) {
                    op_bento_prompt_release(cancellation);
                    finished = true;
                    controller.error(invalid);
                    return;
                }
                let onAbort = () => {
                    if (finished) {
                        return;
                    }
                    op_bento_prompt_cancel(cancellation);
                    release();
                    // The reason wins over whatever the transport did with the
                    // operation: a stream a consumer stopped reading can be closed
                    // rather than errored natively, and this is still an abort.
                    controller.error(signal.reason);
                };
                signal[add](onAbort);
                detach = () => signal[remove](onAbort);
                // Starting here keeps this operation's place in the session's order, even
                // though nothing is pulled until a consumer reads.
                started = op_bento_prompt_stream(
                    session[state].rid,
                    messages,
                    request,
                    cancellation,
                );
            },
            async pull(controller) {
                if (finished) {
                    return;
                }
                if (generation === null) {
                    let opened = await started;
                    if (finished) {
                        return;
                    }
                    if (opened.status === "failed") {
                        release();
                        controller.error(failureOf(session, opened));
                        return;
                    }
                    generation = opened.value;
                }
                let next = await op_bento_prompt_stream_next(generation);
                if (finished) {
                    return;
                }
                if (next.status === "failed") {
                    release();
                    controller.error(failureOf(session, next));
                    return;
                }
                if (next.value === null) {
                    release();
                    controller.close();
                    return;
                }
                controller.enqueue(next.value);
            },
            cancel() {
                // A consumer stopping is not an error; it stops the model's work.
                op_bento_prompt_cancel(cancellation);
                release();
            },
        },
        { highWaterMark: 0 },
    );
}

Object.defineProperty(globalThis, "LanguageModel", {
    value: LanguageModel,
    writable: true,
    enumerable: false,
    configurable: true,
});
