// The errors Node gives a refused operation, and the redirection that turns Deno's refusals
// into them.
import { core, primordials } from "ext:core/mod.js";

let {
    Error,
    ErrorCaptureStackTrace,
    ObjectDefineProperty,
    ObjectSetPrototypeOf,
    PromisePrototypeCatch,
    ReflectApply,
    ReflectConstruct,
    RegExpPrototypeExec,
    SymbolFor,
    TypeError,
} = primordials;

/** Node's `permission` property and the flag that allows it, by Deno's permission name. */
const DENIALS = {
    read: ["FileSystemRead", "--allow-fs-read"],
    write: ["FileSystemWrite", "--allow-fs-write"],
    net: ["Net", "--allow-net"],
    run: ["ChildProcess", "--allow-child-process"],
    worker: ["WorkerThreads", "--allow-worker"],
    // The one system query Bento refuses (`permissions::deno_permissions`).
    sys: ["Inspector", "--allow-inspector"],
};

// Deno's permission errors: `Requires read access to "/etc/hosts", run again with ...`.
const DENIAL = /^Requires (\w+) access(?: to (.*?))?, (?:run again with|specify the required)/s;

// Node's refusal to create a link under scoped file permissions; Bento's link operations
// throw it with this message (`fs_ops::mutations`).
const SYMLINK_REFUSED = "fs.symlink API requires full fs.read and fs.write permissions.";

/** The code Node's DNS binding completes a refused request with. */
export const DNS_REFUSED = "ERR_ACCESS_DENIED";

/** Marks the properties Bento's host reads back from an uncaught error, `code` among them. */
const ADDITIONAL_PROPERTY_KEYS = SymbolFor("errorAdditionalPropertyKeys");

function restricted(flag) {
    return `Access to this API has been restricted. Use ${flag} to manage permissions.`;
}

/** Gives `error` Node's own properties, in Node's order, `code` among them. */
export function nodeProperties(error, properties) {
    for (let [key, value] of properties) error[key] = value;
    ObjectDefineProperty(error, ADDITIONAL_PROPERTY_KEYS, {
        __proto__: null,
        value: ["code"],
        configurable: true,
    });
}

export function nodeCode(error, code) {
    nodeProperties(error, [["code", code]]);
}

/** Node's `ERR_ACCESS_DENIED` for `permission`, naming `resource` as Node does. */
export function accessDenied(permission, flag, resource) {
    let error = new Error(restricted(flag));
    nodeProperties(error, [
        ["code", "ERR_ACCESS_DENIED"],
        ["permission", permission],
        ["resource", resource],
    ]);
    ErrorCaptureStackTrace(error, accessDenied);
    return error;
}

export function netDenied(resource) {
    return accessDenied("Net", "--allow-net", resource);
}

export function fileDenied(scope, resource) {
    return scope === "fs.read"
        ? accessDenied("FileSystemRead", "--allow-fs-read", resource)
        : accessDenied("FileSystemWrite", "--allow-fs-write", resource);
}

export function isDenial(error, permission) {
    return error?.code === "ERR_ACCESS_DENIED" && error.permission === permission;
}

const IP = /^(?:\d{1,3}(?:\.\d{1,3}){3}|[0-9a-fA-F:]*:[0-9a-fA-F:.]*)$/;

/** `host:port`, `[v6]:port`, or a bare host, as Deno names a network resource. */
function endpoint(text) {
    let match =
        RegExpPrototypeExec(/^\[([^\]]*)\](?::(\d+))?$/, text) ??
        RegExpPrototypeExec(/^([^:]*)(?::(\d+))?$/, text);
    return match === null ? { host: text } : { host: match[1], port: match[2] };
}

/**
 * Node's socket error for a denial its binding hands back in place of an error number: the
 * denial is the `errno`, and its message follows the call's name in place of the address.
 */
export function socketDenial(denied, syscall, address, port) {
    let properties = [
        ["errno", denied],
        ["code", denied.code],
        ["syscall", syscall],
        ["address", address],
    ];
    if (port) properties.push(["port", port]);
    return { message: `${syscall} ${denied.code} ${denied.message}`, properties };
}

/**
 * Node reports a refused connection as its socket layer does: a failed `connect` to an IP
 * address, or a failed `getaddrinfo` for a host name, which Node refuses before resolving it.
 */
function refusedConnection(resource) {
    let { host, port } = endpoint(resource);
    if (RegExpPrototypeExec(IP, host) === null) {
        return {
            message: `getaddrinfo ${DNS_REFUSED} ${host}`,
            properties: [
                ["errno", undefined],
                ["code", DNS_REFUSED],
                ["syscall", "getaddrinfo"],
                ["hostname", host],
            ],
        };
    }
    return socketDenial(netDenied(host), "connect", host, port && Number(port));
}

/**
 * The message and properties Node gives the denial a Deno permission error describes, or
 * `undefined` for any other message.
 */
function denial(message) {
    if (message === SYMLINK_REFUSED) {
        return {
            message,
            properties: [
                ["code", "ERR_ACCESS_DENIED"],
                ["permission", ""],
                ["resource", ""],
            ],
        };
    }
    let match = typeof message === "string" ? RegExpPrototypeExec(DENIAL, message) : null;
    if (match === null) return undefined;
    let [, name, quoted] = match;
    let resource = quoted === undefined ? "" : quoted.replace(/^"(.*)"$/s, "$1");
    if (name === "ffi") {
        return {
            message: "Cannot load native addon because loading addons is disabled.",
            properties: [["code", "ERR_DLOPEN_DISABLED"]],
        };
    }
    let known = DENIALS[name];
    if (known === undefined) return undefined;
    let [permission, flag] = known;
    if (name === "sys") resource = "";
    if (name === "net" && quoted !== undefined) return refusedConnection(resource);
    return {
        message: restricted(flag),
        properties: [
            ["code", "ERR_ACCESS_DENIED"],
            ["permission", permission],
            ["resource", resource],
        ],
    };
}

/**
 * Deno's operations throw `NotCapable` for every permission they refuse; Node's code and
 * Bento's guests expect Node's errors. `NotCapable` is registered with the core before any
 * extension runs and its builder cannot be replaced, so the class's own `super()` call is
 * redirected here: a denial becomes Node's error, still a `NotCapable` for the Node
 * compatibility layer's own checks, and any other message builds an ordinary `NotCapable`.
 */
export function installDenialErrors() {
    let { NotCapable } = core;
    function DenialError(message, options) {
        let shape = denial(message);
        let error = ReflectConstruct(Error, [shape?.message ?? message, options], new.target);
        if (shape === undefined) return error;
        // `NotCapable`'s constructor names every instance; a denial keeps Node's `Error`.
        ObjectDefineProperty(error, "name", {
            __proto__: null,
            get: () => "Error",
            set: () => {},
            configurable: true,
        });
        nodeProperties(error, shape.properties);
        return error;
    }
    DenialError.prototype = Error.prototype;
    ObjectSetPrototypeOf(NotCapable, DenialError);

    // Node's fetch rejects a refused connection as a failed fetch caused by the denial.
    let fetch = globalThis.fetch;
    globalThis.fetch = function fetch_(...args) {
        return PromisePrototypeCatch(ReflectApply(fetch, this, args), error => {
            if (error?.code === "ERR_ACCESS_DENIED") {
                throw new TypeError("fetch failed", { cause: error });
            }
            throw error;
        });
    };
    ObjectDefineProperty(globalThis.fetch, "name", { __proto__: null, value: "fetch" });
}
