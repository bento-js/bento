/**
 * Ambient declarations for the Prompt API that Bento exposes as `globalThis.LanguageModel`.
 *
 * The shape follows the Prompt API Community Group draft at revision
 * `6dac677d5425d1175f4697e73dadd572a1ad6483`, narrowed to what Bento implements: text
 * conversations and constrained JSON output. Capabilities the draft describes but Bento does
 * not implement are absent here rather than declared and unsupported: image, audio, tool-call
 * and tool-response content, tool declarations, `samplingMode`, `topK`, `temperature`,
 * `params()`, `inputUsage`, and `inputQuota`. Declaring this API does not mean the running
 * host grants access to a model, or that a model backend is compiled in; see the guide.
 */

/** Whether a model can serve a configuration, as reported by this backend. */
type Availability = "available" | "unavailable";

/** Who a message came from. */
type LanguageModelMessageRole = "system" | "user" | "assistant";

/** The kind of a content item. Bento supports text only. */
type LanguageModelMessageType = "text";

/** One content item of a message. */
interface LanguageModelMessageContent {
    type: LanguageModelMessageType;
    value: string;
}

/** One conversation turn. */
interface LanguageModelMessage {
    role: LanguageModelMessageRole;
    content: string | LanguageModelMessageContent[];
    /**
     * Whether an assistant message is an unfinished turn the model should continue. Bento's
     * backend cannot continue a partial assistant turn, so `true` is refused with a
     * `NotSupportedError` `DOMException`: `prompt()`, `append()` and
     * `measureContextUsage()` reject with it, and `promptStreaming()` errors its stream
     * with it.
     */
    prefix?: boolean;
}

/** A prompt: one string, or an ordered list of messages. */
type LanguageModelPrompt = string | LanguageModelMessage[];

/** A content type a session expects to receive or produce. */
interface LanguageModelExpected {
    type: LanguageModelMessageType;
    /** BCP 47 language tags. A tag that is not one rejects with a `RangeError`. */
    languages?: string[];
}

/**
 * The `downloadprogress` event. Bento reports only the two initialization endpoints, so
 * `loaded` is 0 and then 1, `total` is 1, and `lengthComputable` is true.
 */
interface CreateMonitorProgressEvent extends Event {
    readonly loaded: number;
    readonly total: number;
    readonly lengthComputable: boolean;
}

/** Events a `CreateMonitor` dispatches. */
interface CreateMonitorEventMap {
    downloadprogress: CreateMonitorProgressEvent;
}

/** Reports the endpoints of model initialization. */
interface CreateMonitor extends EventTarget {
    /**
     * Fires twice for an available model, with `total` 1, `lengthComputable` true, and
     * `loaded` 0 then 1. These are the specified initialization endpoints; Bento never
     * downloads model assets.
     */
    ondownloadprogress:
        | ((this: CreateMonitor, event: CreateMonitorProgressEvent) => unknown)
        | null;
    addEventListener<K extends keyof CreateMonitorEventMap>(
        type: K,
        listener: (this: CreateMonitor, event: CreateMonitorEventMap[K]) => unknown,
        options?: boolean | AddEventListenerOptions,
    ): void;
    addEventListener(
        type: string,
        listener: EventListenerOrEventListenerObject,
        options?: boolean | AddEventListenerOptions,
    ): void;
    removeEventListener<K extends keyof CreateMonitorEventMap>(
        type: K,
        listener: (this: CreateMonitor, event: CreateMonitorEventMap[K]) => unknown,
        options?: boolean | EventListenerOptions,
    ): void;
    removeEventListener(
        type: string,
        listener: EventListenerOrEventListenerObject,
        options?: boolean | EventListenerOptions,
    ): void;
}

/** Configuration shared by `LanguageModel.availability()` and `LanguageModel.create()`. */
interface LanguageModelCreateCoreOptions {
    /** Content types and languages the session expects to receive. */
    expectedInputs?: LanguageModelExpected[];
    /** Content types and languages the session expects to produce. */
    expectedOutputs?: LanguageModelExpected[];
}

/** Configuration for `LanguageModel.create()`. */
interface LanguageModelCreateOptions extends LanguageModelCreateCoreOptions {
    /** Cancels creation, and destroys the created session when aborted later. */
    signal?: AbortSignal;
    /** Called with a `CreateMonitor` before initialization. Throwing rejects creation. */
    monitor?: (monitor: CreateMonitor) => void;
    /** History loaded into the session without generating replies. */
    initialPrompts?: LanguageModelMessage[];
}

/** Per-operation options. */
interface LanguageModelPromptOptions {
    /**
     * A JSON Schema from Bento's supported subset. The response is JSON text validated
     * against it. Unsupported keywords reject with a `NotSupportedError` `DOMException`;
     * malformed supported keywords reject with a `TypeError`.
     */
    responseConstraint?: object;
    /**
     * Suppresses the constraint's instructional input while keeping constrained decoding.
     * `measureContextUsage()` requires a `responseConstraint` alongside it.
     */
    omitResponseConstraintInput?: boolean;
    /** Cancels this operation only. */
    signal?: AbortSignal;
}

/** Options for `clone()`. */
interface LanguageModelCloneOptions {
    /** Cancels cloning. A completed clone has its own lifetime. */
    signal?: AbortSignal;
}

/** Options for `append()`. */
interface LanguageModelAppendOptions {
    /** Cancels this append only. */
    signal?: AbortSignal;
}

/** Events a `LanguageModel` session dispatches. */
interface LanguageModelEventMap {
    contextoverflow: Event;
}

/**
 * One conversation with the operating system's on-device model.
 *
 * Instances come from `LanguageModel.create()`; the interface is not constructible. Each
 * isolate owns its own sessions: a worker cannot reach another isolate's session.
 */
declare class LanguageModel extends EventTarget {
    private constructor();

    /** Whether a model can serve `options` for this host, after validating them. */
    static availability(options?: LanguageModelCreateCoreOptions): Promise<Availability>;

    /**
     * Starts a conversation. Rejects with a `NotAllowedError` `DOMException` when the host
     * has not granted model access, and with a `NotSupportedError` when the model is
     * unavailable.
     */
    static create(options?: LanguageModelCreateOptions): Promise<LanguageModel>;

    /** Generates one response to `input` and appends both to the conversation. */
    prompt(input: LanguageModelPrompt, options?: LanguageModelPromptOptions): Promise<string>;

    /**
     * Generates one response as a stream, returned immediately. Text streams deliver
     * incremental chunks whose concatenation is the response; a constrained stream delivers
     * one validated JSON string. Throws synchronously for an already-aborted signal.
     */
    promptStreaming(
        input: LanguageModelPrompt,
        options?: LanguageModelPromptOptions,
    ): ReadableStream<string>;

    /** Commits `input` to the conversation without generating a response. */
    append(input: LanguageModelPrompt, options?: LanguageModelAppendOptions): Promise<void>;

    /** Tokens `input` would add, without changing the conversation. */
    measureContextUsage(
        input: LanguageModelPrompt,
        options?: LanguageModelPromptOptions,
    ): Promise<number>;

    /** An independent session starting from this conversation's committed history. */
    clone(options?: LanguageModelCloneOptions): Promise<LanguageModel>;

    /**
     * Makes this session unusable at once and cancels its pending work with an `AbortError`
     * `DOMException`. Idempotent; native resources are released in the background.
     */
    destroy(): undefined;

    /** Tokens the conversation currently occupies. */
    readonly contextUsage: number;

    /** Tokens the conversation may occupy in total. */
    readonly contextWindow: number;

    /** Fires once for an operation that cannot fit in the context window. */
    oncontextoverflow: ((this: LanguageModel, event: Event) => unknown) | null;

    addEventListener<K extends keyof LanguageModelEventMap>(
        type: K,
        listener: (this: LanguageModel, event: LanguageModelEventMap[K]) => unknown,
        options?: boolean | AddEventListenerOptions,
    ): void;
    addEventListener(
        type: string,
        listener: EventListenerOrEventListenerObject,
        options?: boolean | AddEventListenerOptions,
    ): void;
    removeEventListener<K extends keyof LanguageModelEventMap>(
        type: K,
        listener: (this: LanguageModel, event: LanguageModelEventMap[K]) => unknown,
        options?: boolean | EventListenerOptions,
    ): void;
    removeEventListener(
        type: string,
        listener: EventListenerOrEventListenerObject,
        options?: boolean | EventListenerOptions,
    ): void;
}
