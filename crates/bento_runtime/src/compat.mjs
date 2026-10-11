let addEventListener = EventTarget.prototype.addEventListener;
let removeEventListener = EventTarget.prototype.removeEventListener;
EventTarget.prototype.addEventListener = function (type, listener, options) {
    let signal = typeof options === "object" && options !== null ? options.signal : undefined;
    if (signal === undefined || signal instanceof AbortSignal) {
        return addEventListener.call(this, type, listener, options);
    }
    if (signal === null || !("aborted" in Object(signal))) {
        throw new TypeError("signal must have an aborted property");
    }
    if (signal.aborted) return;
    addEventListener.call(this, type, listener, { ...options, signal: undefined });
    signal.addEventListener(
        "abort",
        () => {
            removeEventListener.call(this, type, listener, options);
        },
        { once: true },
    );
};
