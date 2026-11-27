// `process.permission`: what this thread holds, and giving part of it up.
import { nodeCode } from "ext:bento_instance/permission/errors.mjs";
import { ownsEnvironment } from "ext:bento_instance/worker_environment.mjs";
import { core, primordials } from "ext:core/mod.js";

let {
    Error,
    ErrorCaptureStackTrace,
    ObjectKeys,
    ObjectPrototypeIsPrototypeOf,
    StringPrototypeIncludes,
    StringPrototypeStartsWith,
    TypeError,
    TypedArrayPrototypeGetSymbolToStringTag,
} = primordials;
let { op_bento_permission_drop, op_bento_permission_has } = core.ops;

/** Whether the calling thread holds all of `scope`. */
export function held(scope) {
    return op_bento_permission_has(scope, undefined);
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
    let shown = inspect(value);
    if (shown.length > 28) shown = `${shown.slice(0, 25)}...`;
    return ` Received type ${typeof value} (${shown})`;
}

function inspect(value) {
    return core.createLazyLoader("node:util")().inspect(value, { colors: false });
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

export function has(scope, reference) {
    validateString(scope, "scope");
    let normalized = normalizeReference(scope, reference);
    // An empty reference names nothing Node could hold.
    return normalized !== "" && op_bento_permission_has(scope, normalized);
}

export function drop(scope, reference) {
    validateString(scope, "scope");
    let normalized = normalizeReference(scope, reference);
    if (scope === "env") {
        validateVariableReference(reference, normalized);
        refuseSharedEnvironment();
    }
    // Node 26 reads an empty reference as none, and drops the whole scope.
    if (normalized === "") normalized = undefined;
    op_bento_permission_drop(scope, normalized);
    if (scope === "env") removeVariables(normalized);
}

/** Node 27 refuses an empty or wildcard name when dropping environment variables. */
function validateVariableReference(reference, name) {
    let message;
    if (name === "") {
        message = `The argument 'reference' must not be empty. Received ${inspect(reference)}`;
    } else if (name !== undefined && StringPrototypeIncludes(name, "*")) {
        message = "Wildcards are not supported when dropping environment variables";
    } else {
        return;
    }
    let error = new TypeError(message);
    nodeCode(error, "ERR_INVALID_ARG_VALUE");
    throw error;
}

/** Node 27 lets only a thread that owns its environment drop variables from it. */
function refuseSharedEnvironment() {
    if (ownsEnvironment()) return;
    let error = new Error(
        "Environment variables can only be dropped from an Environment that owns the process state",
    );
    nodeCode(error, "ERR_INVALID_STATE");
    throw error;
}

/** Takes the variables a drop gave up out of this thread's own environment. */
function removeVariables(name) {
    let { env } = core.createLazyLoader("node:process")().default;
    if (name !== undefined) {
        delete env[name];
        return;
    }
    for (let key of ObjectKeys(env)) {
        if (!op_bento_permission_has("env", key)) delete env[key];
    }
}
