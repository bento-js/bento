import { core, primordials } from "ext:core/mod.js";

let {
    Error,
    ErrorCaptureStackTrace,
    ObjectDefineProperty,
    ObjectPrototypeIsPrototypeOf,
    ObjectSetPrototypeOf,
    PromisePrototypeCatch,
    ReflectApply,
    ReflectConstruct,
    RegExpPrototypeExec,
    StringPrototypeIncludes,
    StringPrototypeStartsWith,
    SymbolFor,
    TypeError,
    TypedArrayPrototypeGetSymbolToStringTag,
} = primordials;
let {
    op_bento_permission_drop,
    op_bento_permission_has,
    op_bento_permission_withholds,
    op_bento_register_env_warning,
} = core.ops;

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

/** Marks the properties Bento's host reads back from an uncaught error, `code` among them. */
const ADDITIONAL_PROPERTY_KEYS = SymbolFor("errorAdditionalPropertyKeys");

function restricted(flag) {
    return `Access to this API has been restricted. Use ${flag} to manage permissions.`;
}

/** Gives `error` Node's own properties, in Node's order, `code` among them. */
function nodeProperties(error, properties) {
    for (let [key, value] of properties) error[key] = value;
    ObjectDefineProperty(error, ADDITIONAL_PROPERTY_KEYS, {
        __proto__: null,
        value: ["code"],
        configurable: true,
    });
}

function nodeCode(error, code) {
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

const IP = /^(?:\d{1,3}(?:\.\d{1,3}){3}|[0-9a-fA-F:]*:[0-9a-fA-F:.]*)$/;

/** `host:port`, `[v6]:port`, or a bare host, as Deno names a network resource. */
function endpoint(text) {
    let match =
        RegExpPrototypeExec(/^\[([^\]]*)\](?::(\d+))?$/, text) ??
        RegExpPrototypeExec(/^([^:]*)(?::(\d+))?$/, text);
    return match === null ? { host: text } : { host: match[1], port: match[2] };
}

/**
 * Node reports a refused connection as its socket layer does: a failed `connect` to an IP
 * address, or a failed `getaddrinfo` for a host name, which Node refuses before resolving it.
 */
function refusedConnection(resource, flag) {
    let { host, port } = endpoint(resource);
    if (RegExpPrototypeExec(IP, host) === null) {
        return {
            message: `getaddrinfo ERR_ACCESS_DENIED ${host}`,
            properties: [
                ["errno", undefined],
                ["code", "ERR_ACCESS_DENIED"],
                ["syscall", "getaddrinfo"],
                ["hostname", host],
            ],
        };
    }
    let properties = [
        ["errno", { code: "ERR_ACCESS_DENIED", permission: "Net", resource: host }],
        ["code", "ERR_ACCESS_DENIED"],
        ["syscall", "connect"],
        ["address", host],
    ];
    if (port !== undefined) properties.push(["port", Number(port)]);
    return { message: `connect ERR_ACCESS_DENIED ${restricted(flag)}`, properties };
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
    if (name === "net" && quoted !== undefined) return refusedConnection(resource, flag);
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
function installDenialErrors() {
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

/** Node's `ERR_INVALID_ARG_TYPE` for an argument `name` that is not a string. */
function validateString(value, name) {
    if (typeof value === "string") return;
    let error = new TypeError(`The "${name}" argument must be of type string.${received(value)}`);
    nodeCode(error, "ERR_INVALID_ARG_TYPE");
    ErrorCaptureStackTrace(error, validateString);
    throw error;
}

/** How Node's argument errors describe the value they were given. */
function received(value) {
    if (value === null || value === undefined) return ` Received ${value}`;
    if (typeof value === "function") return ` Received function ${value.name}`;
    if (typeof value === "object") {
        let name = value.constructor?.name;
        return name ? ` Received an instance of ${name}` : " Received [Object: null prototype]";
    }
    let { inspect } = core.createLazyLoader("node:util")();
    let shown = inspect(value, { colors: false });
    if (shown.length > 28) shown = `${shown.slice(0, 25)}...`;
    return ` Received type ${typeof value} (${shown})`;
}

/** Node's reading of a reference: a `file:` URL or bytes name a path for file scopes. */
function normalizeReference(scope, reference) {
    if (reference === null || reference === undefined) return undefined;
    if (ObjectPrototypeIsPrototypeOf(URL.prototype, reference)) {
        if (StringPrototypeStartsWith(scope, "fs")) {
            return core.createLazyLoader("node:url")().fileURLToPath(reference);
        }
        return reference.href;
    }
    if (TypedArrayPrototypeGetSymbolToStringTag(reference) === "Uint8Array") {
        return new TextDecoder().decode(reference);
    }
    validateString(reference, "reference");
    return reference;
}

function has(scope, reference) {
    validateString(scope, "scope");
    return op_bento_permission_has(scope, normalizeReference(scope, reference));
}

function drop(scope, reference) {
    validateString(scope, "scope");
    let normalized = normalizeReference(scope, reference);
    if (scope === "env" && normalized !== undefined && StringPrototypeIncludes(normalized, "*")) {
        let error = new TypeError(
            "Wildcards are not supported when dropping environment variables",
        );
        nodeCode(error, "ERR_INVALID_ARG_VALUE");
        throw error;
    }
    op_bento_permission_drop(scope, normalized);
}

/** Wraps the ways to start a process so a refusal throws Node's error before any starts. */
function guardChildProcesses(process) {
    let childProcess = core.createLazyLoader("node:child_process")().default;
    let guard = (original, resource) =>
        function (...args) {
            if (!op_bento_permission_has("child", undefined)) {
                throw accessDenied("ChildProcess", "--allow-child-process", resource(...args));
            }
            return ReflectApply(original, this, args);
        };
    let shell = (_command, options) =>
        typeof options?.shell === "string" ? options.shell : "/bin/sh";
    let file = command => command;
    let resources = {
        spawn: file,
        spawnSync: file,
        execFile: file,
        execFileSync: file,
        exec: shell,
        execSync: shell,
        fork: (_module, args, options) =>
            (Array.isArray(args) ? options : args)?.execPath ?? process.execPath,
    };
    for (let name in resources) {
        childProcess[name] = guard(childProcess[name], resources[name]);
    }
    let prototype = childProcess.ChildProcess.prototype;
    prototype.spawn = guard(prototype.spawn, options => options?.file);
}

/**
 * Node refuses `server.listen()` itself, while Deno's Node layer reports the refusal as a
 * failed connection; check first so a network listener is refused as Node refuses it. A
 * listener on a path is a file system operation and is left to its own checks.
 */
function guardListeners() {
    let { Server } = core.createLazyLoader("node:net")().default;
    let listen = Server.prototype.listen;
    Server.prototype.listen = function (...args) {
        let [first] = args;
        let path = typeof first === "string" ? Number.isNaN(Number(first)) : first?.path != null;
        if (!path && !op_bento_permission_has("net", undefined)) {
            throw accessDenied("Net", "--allow-net", "");
        }
        return ReflectApply(listen, this, args);
    };
}

/**
 * WASI and the inspector, which Bento provides, refuse their entry points as Node does. The
 * modules' ESM wrappers copy these properties when they first load, so patching the shared
 * scripts first covers every way to import them. The operations behind the inspector refuse
 * as well (`permissions::deno_permissions`).
 */
function guardWasiAndInspector() {
    let held = scope => op_bento_permission_has(scope, undefined);
    let wasi = core.loadExtScript("ext:deno_node/wasi.ts");
    let Wasi = wasi.WASI;
    let Guarded = class WASI extends Wasi {
        constructor(...args) {
            if (!held("wasi")) throw accessDenied("WASI", "--allow-wasi", "");
            super(...args);
        }
    };
    wasi.WASI = Guarded;
    if (wasi.default?.WASI === Wasi) wasi.default.WASI = Guarded;

    let inspector = core.loadExtScript("ext:deno_node/inspector.js");
    let refuse = resource => {
        if (!held("inspector")) throw accessDenied("Inspector", "--allow-inspector", resource);
    };
    let { connect } = inspector.Session.prototype;
    inspector.Session.prototype.connect = function (...args) {
        refuse("Connect");
        return ReflectApply(connect, this, args);
    };
    let { open, url } = inspector;
    inspector.open = function (...args) {
        refuse("StartIoThread");
        return ReflectApply(open, this, args);
    };
    // Without the permission no inspector can be open, so there is no URL to report.
    inspector.url = function (...args) {
        return held("inspector") ? ReflectApply(url, this, args) : undefined;
    };
}

export function install() {
    let process = core.createLazyLoader("node:process")().default;
    installDenialErrors();
    // The loader's stand-in for a module the instance may not read throws through this.
    ObjectDefineProperty(globalThis, SymbolFor("bento.accessDenied"), {
        __proto__: null,
        value: accessDenied,
    });
    op_bento_register_env_warning(name => {
        process.emitWarning(
            `The permission model removed the environment variable "${name}" at startup. Use --allow-env to manage permissions.`,
        );
    });
    guardChildProcesses(process);
    guardListeners();
    guardWasiAndInspector();
    if (op_bento_permission_withholds()) {
        ObjectDefineProperty(process, "permission", {
            __proto__: null,
            enumerable: true,
            configurable: false,
            value: { has, drop },
        });
    }
}
