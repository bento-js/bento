// WASI and the inspector, the APIs Bento provides that Node's other permissions govern.
import { accessDenied, isDenial } from "ext:bento_instance/permission/errors.mjs";
import { held } from "ext:bento_instance/permission/process_permission.mjs";
import { core, primordials } from "ext:core/mod.js";

let { ObjectDefineProperty, ReflectApply, ReflectConstruct } = primordials;

/**
 * WASI refuses construction as Node does: after validating the options as Node's
 * JavaScript does, before its binding would start. The guard is a plain function sharing
 * the class's prototype, so no prototype chain leads a guest back to the unguarded class.
 */
export function guardWasi() {
    let wasi = core.loadExtScript("ext:deno_node/wasi.ts");
    let Wasi = wasi.WASI;
    function WASI(...args) {
        // Calling the class without `new` throws its own error.
        if (new.target === undefined) return ReflectApply(Wasi, this, args);
        validateWasiOptions(args[0]);
        if (!held("wasi")) throw accessDenied("WASI", "--allow-wasi", "");
        return ReflectConstruct(Wasi, args, new.target);
    }
    WASI.prototype = Wasi.prototype;
    ObjectDefineProperty(Wasi.prototype, "constructor", {
        __proto__: null,
        value: WASI,
        writable: true,
        configurable: true,
    });
    wasi.WASI = WASI;
    wasi.default.WASI = WASI;
}

/** The options Node's `WASI` validates before its binding checks the permission. */
function validateWasiOptions(options = {}) {
    let { validateArray, validateInt32, validateObject, validateString } = core.loadExtScript(
        "ext:deno_node/internal/validators.mjs",
    );
    validateObject(options, "options");
    let { version, args, env, preopens, stdin = 0, stdout = 1, stderr = 2 } = options;
    validateString(version, "options.version");
    if (version !== "preview1" && version !== "unstable") {
        let { ERR_INVALID_ARG_VALUE } = core.loadExtScript("ext:deno_node/internal/errors.ts");
        throw new ERR_INVALID_ARG_VALUE("options.version", version, "unsupported WASI version");
    }
    if (args !== undefined) validateArray(args, "options.args");
    if (env !== undefined) validateObject(env, "options.env");
    if (preopens !== undefined) validateObject(preopens, "options.preopens");
    validateInt32(stdin, "options.stdin", 0);
    validateInt32(stdout, "options.stdout", 0);
    validateInt32(stderr, "options.stderr", 0);
}

/**
 * The inspector refuses its entry points as Node does. The modules' ESM wrappers copy these
 * properties when they first load, so patching the shared script first covers every way to
 * import it. The operation behind a session refuses after the session's own checks, as
 * Node's binding does (`permissions::deno_permissions`); Node names the call it refused.
 */
export function guardInspector() {
    let inspector = core.loadExtScript("ext:deno_node/inspector.js");
    let { prototype } = inspector.Session;
    prototype.connect = namingRefusal(prototype.connect, "Connect");
    prototype.connectToMainThread = namingRefusal(
        prototype.connectToMainThread,
        "ConnectToMainThread",
    );
    let { open, url } = inspector;
    inspector.open = function (...args) {
        if (!held("inspector")) {
            throw accessDenied("Inspector", "--allow-inspector", "StartIoThread");
        }
        return ReflectApply(open, this, args);
    };
    // Without the permission no inspector can be open, so there is no URL to report.
    inspector.url = function (...args) {
        return held("inspector") ? ReflectApply(url, this, args) : undefined;
    };
}

function namingRefusal(original, resource) {
    return function (...args) {
        try {
            return ReflectApply(original, this, args);
        } catch (error) {
            if (!isDenial(error, "Inspector")) throw error;
            throw accessDenied("Inspector", "--allow-inspector", resource);
        }
    };
}
