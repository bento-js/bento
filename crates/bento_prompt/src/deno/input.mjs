// Argument conversion, validation, and canonicalization for the Prompt API, in the shape of
// the pinned draft. Nothing here reaches the model: it decides what the arguments mean and
// which of them this backend can represent at all.

import { core, primordials } from "ext:core/mod.js";

let webidl = core.loadExtScript("ext:deno_webidl/00_webidl.js");
let { DOMException, QuotaExceededError } = core.loadExtScript("ext:deno_web/01_dom_exception.js");
let { AbortSignal } = core.loadExtScript("ext:deno_web/03_abort_signal.js");
let { isBlob } = core.loadExtScript("ext:deno_web/09_file.js");
let { ArrayBufferIsView, ObjectPrototypeIsPrototypeOf, ReflectApply, SymbolIterator } = primordials;
// Snapshot globals use cached platform constructors, not the temporary core.ops constructors.
let loadImage =
    core.ops.ImageBitmap === undefined ? null : core.createLazyLoader("ext:deno_image/01_image.js");
let loadCanvas =
    core.ops.OffscreenCanvas === undefined
        ? null
        : core.createLazyLoader("ext:deno_canvas/01_canvas.js");

export function domError(name, message) {
    return new DOMException(message, name);
}

export function notSupported(message) {
    return domError("NotSupportedError", message);
}

/** A refusal the Rust side reported, as the error the draft names for its category. */
export function webError(failure) {
    if (failure.error === "TypeError") {
        return new TypeError(failure.message);
    }
    if (failure.error === "QuotaExceededError") {
        // The draft reports context exhaustion as the WebIDL interface, which says what the
        // operation needed and what the session holds. Numbers the failure did not know stay
        // absent rather than arriving as NaN.
        let options = {};
        if (failure.requested !== undefined) {
            options.requested = failure.requested;
        }
        if (failure.quota !== undefined) {
            options.quota = failure.quota;
        }
        return new QuotaExceededError(failure.message, options);
    }
    return domError(failure.error, failure.message);
}

// Dictionaries and enums, in the draft's shape. Members Bento does not implement are absent,
// so they follow ordinary WebIDL handling for unknown members instead of a bespoke parser.

let roleConverter = webidl.createEnumConverter("LanguageModelMessageRole", [
    "system",
    "user",
    "assistant",
]);

// Every type the draft names is a valid enum value; the ones Bento cannot serve are refused
// as unsupported capabilities, which is a different answer from an invalid enum value.
let typeConverter = webidl.createEnumConverter("LanguageModelMessageType", [
    "text",
    "image",
    "audio",
    "tool-call",
    "tool-response",
]);

function messageValueConverter(value, prefix, context) {
    if (isBlob(value)) {
        return webidl.converters.Blob(value, prefix, context);
    }
    // BufferSource excludes a bare SharedArrayBuffer, so WebIDL sends it to the string branch.
    if (core.isArrayBuffer(value) || ArrayBufferIsView(value)) {
        return webidl.converters.BufferSource(value, prefix, context);
    }
    if (webidl.type(value) === "Object") {
        let { ImageDataPrototype } = core.loadExtScript("ext:deno_web/16_image_data.js");
        if (
            ObjectPrototypeIsPrototypeOf(ImageDataPrototype, value) ||
            (loadImage !== null &&
                ObjectPrototypeIsPrototypeOf(loadImage().ImageBitmap.prototype, value)) ||
            (loadCanvas !== null &&
                ObjectPrototypeIsPrototypeOf(loadCanvas().OffscreenCanvas.prototype, value))
        ) {
            return value;
        }
    }
    return webidl.converters.DOMString(value, prefix, context);
}

let contentConverter = webidl.createDictionaryConverter("LanguageModelMessageContent", [
    { key: "type", converter: typeConverter, required: true },
    { key: "value", converter: messageValueConverter, required: true },
]);

let contentSequenceConverter = webidl.createSequenceConverter(contentConverter);

let messageContentConverter = (value, prefix, context) => {
    let sequence = asSequence(value);
    if (sequence !== null) {
        return contentSequenceConverter(sequence, prefix, context);
    }
    return webidl.converters.DOMString(value, prefix, context);
};

let messageConverter = webidl.createDictionaryConverter("LanguageModelMessage", [
    { key: "role", converter: roleConverter, required: true },
    { key: "content", converter: messageContentConverter, required: true },
    { key: "prefix", converter: webidl.converters.boolean, defaultValue: false },
]);

let messageSequenceConverter = webidl.createSequenceConverter(messageConverter);

export let promptConverter = (value, prefix, context) => {
    let sequence = asSequence(value);
    if (sequence !== null) {
        return messageSequenceConverter(sequence, prefix, context);
    }
    return webidl.converters.DOMString(value, prefix, context);
};

// `languages` is a sequence, not a union with a single tag: a lone string is an ordinary
// WebIDL conversion failure, decided here rather than by what the model or the host answers.
let stringSequenceConverter = webidl.createSequenceConverter(webidl.converters.DOMString);

let expectedConverter = webidl.createDictionaryConverter("LanguageModelExpected", [
    { key: "type", converter: typeConverter, required: true },
    { key: "languages", converter: stringSequenceConverter },
]);

let expectedSequenceConverter = webidl.createSequenceConverter(expectedConverter);

let signalConverter = webidl.createInterfaceConverter("AbortSignal", AbortSignal.prototype);

let coreOptionsMembers = [
    { key: "expectedInputs", converter: expectedSequenceConverter },
    { key: "expectedOutputs", converter: expectedSequenceConverter },
];

export let coreOptionsConverter = webidl.createDictionaryConverter(
    "LanguageModelCreateCoreOptions",
    coreOptionsMembers,
);

export let createOptionsConverter = webidl.createDictionaryConverter(
    "LanguageModelCreateOptions",
    coreOptionsMembers,
    [
        { key: "signal", converter: signalConverter },
        // `CreateMonitorCallback` is a callback function, so only something callable is a
        // monitor; an object carrying a method is not one.
        { key: "monitor", converter: webidl.converters.Function },
        { key: "initialPrompts", converter: messageSequenceConverter, defaultValue: [] },
    ],
);

export let promptOptionsConverter = webidl.createDictionaryConverter("LanguageModelPromptOptions", [
    { key: "responseConstraint", converter: webidl.converters.object },
    {
        key: "omitResponseConstraintInput",
        converter: webidl.converters.boolean,
        defaultValue: false,
    },
    { key: "signal", converter: signalConverter },
]);

export let signalOptionsConverter = webidl.createDictionaryConverter("LanguageModelAppendOptions", [
    { key: "signal", converter: signalConverter },
]);

function asSequence(value) {
    if (webidl.type(value) !== "Object") {
        return null;
    }
    let iterator = value[SymbolIterator];
    if (iterator === undefined || iterator === null) {
        return null;
    }
    if (typeof iterator !== "function") {
        throw new TypeError("Symbol.iterator must be callable");
    }
    // Union selection must not read an observable iterator getter again during conversion.
    return { [SymbolIterator]: () => ReflectApply(iterator, value, []) };
}

// Language tags

function canonicalLanguages(languages) {
    let canonical = [];
    for (let language of languages) {
        // Rejects a structurally invalid tag with a RangeError and returns its canonical form.
        let [canonicalized] = Intl.getCanonicalLocales(language);
        if (!canonical.includes(canonicalized)) {
            canonical.push(canonicalized);
        }
    }
    return canonical;
}

export function canonicalizeOptions(options) {
    for (let expected of options.expectedInputs ?? []) {
        if (expected.languages !== undefined) {
            expected.languages = canonicalLanguages(expected.languages);
        }
    }
    for (let expected of options.expectedOutputs ?? []) {
        if (expected.languages !== undefined) {
            expected.languages = canonicalLanguages(expected.languages);
        }
    }
}

/**
 * Whether the configuration asks for a content type this backend has no support for. The
 * draft takes the availability of every expected type together, and a type that cannot be
 * served makes the configuration unavailable rather than invalid.
 */
export function unsupportedType(options) {
    let expected = [...(options.expectedInputs ?? []), ...(options.expectedOutputs ?? [])];
    return expected.some(one => one.type !== "text");
}

// Prompts

/**
 * Turns one prompt into the draft's longhand message list and refuses what this backend
 * cannot represent. `hasAppendedInput` is true once a conversation holds anything.
 */
export function canonicalizePrompt(input, hasAppendedInput) {
    if (typeof input === "string") {
        return [{ role: "user", content: input }];
    }
    if (input.length === 0) {
        return [{ role: "user", content: "" }];
    }
    let messages = [];
    for (let index = 0; index < input.length; index += 1) {
        let message = input[index];
        let content =
            typeof message.content === "string"
                ? [{ type: "text", value: message.content }]
                : message.content;
        if (message.prefix) {
            if (message.role !== "assistant") {
                throw domError("SyntaxError", "Only an assistant message may be a prefix");
            }
            if (index !== input.length - 1) {
                throw domError("SyntaxError", "A prefix must be the last message");
            }
            // Syntactically valid, but this backend cannot continue a partial assistant turn.
            throw notSupported("Continuing an assistant message is not supported");
        }
        if (message.role === "system" && (hasAppendedInput || index !== 0)) {
            throw new TypeError(
                "A system message is only allowed first, in a conversation with no history",
            );
        }
        messages.push({ role: message.role, content: textOf(content, message.role) });
    }
    return messages;
}

function textOf(content, role) {
    if (content.length === 0) {
        return "";
    }
    let text = "";
    for (let item of content) {
        if (item.type === "tool-call" && role !== "assistant") {
            throw new TypeError("A tool call may only come from an assistant message");
        }
        if (item.type === "tool-response" && role !== "user") {
            throw new TypeError("A tool response may only come from a user message");
        }
        if (item.type !== "text") {
            throw notSupported(`Content of type "${item.type}" is not supported`);
        }
        if (typeof item.value !== "string") {
            throw new TypeError("Text content must be a string");
        }
        // Adjacent text content is one message's text for the model.
        text += item.value;
    }
    return text;
}
