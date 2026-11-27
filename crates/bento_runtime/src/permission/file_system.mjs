// `node:fs` functions whose refusals Node reports otherwise than Deno's implementation does.
import { fileDenied } from "ext:bento_instance/permission/errors.mjs";
import { held } from "ext:bento_instance/permission/process_permission.mjs";
import { core, primordials } from "ext:core/mod.js";

let {
    ObjectDefineProperty,
    ObjectGetOwnPropertyDescriptor,
    ObjectKeys,
    ObjectPrototypeIsPrototypeOf,
    Promise,
    PromisePrototype,
    PromisePrototypeCatch,
    ReflectApply,
    ReflectOwnKeys,
    StringPrototypeIndexOf,
    StringPrototypeSlice,
    SymbolFor,
    TypedArrayPrototypeGetSymbolToStringTag,
} = primordials;
let { op_bento_permission_has } = core.ops;

// Node's `lstat` checks the permission in JavaScript, with a message that names no flag.
const LSTAT_REFUSED = "Access to this API has been restricted";

const PROMISIFY_CUSTOM = SymbolFor("nodejs.util.promisify.custom");

let pathModule = () => core.createLazyLoader("node:path")().default;

export function guardFileSystem() {
    let fs = core.loadExtScript("ext:deno_node/fs.ts");
    let { lstatSync } = fs;
    // The promise functions first: Deno's promise `realpath` is built from the callback one.
    replace(fs.promises, FILE_SYSTEM_PROMISES, lstatSync, fs);
    replace(fs, FILE_SYSTEM, lstatSync, fs);
}

// How Node 26 reports a refused file system call where Deno's implementation reports it
// otherwise. Each entry builds the replacement from the original function, the module's own
// `lstatSync`, and the module.

const FILE_SYSTEM = {
    cp: adjusting(copied),
    cpSync: checking(["fs.read", 0], ["fs.write", 1]),
    chmod: checking(["fs.write", 0]),
    exists: existing,
    lstat: adjusting(briefly),
    lstatSync: adjusting(briefly),
    lutimes: checking(["fs.write", 0, true]),
    mkdir: checking(["fs.write", 0]),
    mkdtemp: adjusting(templated),
    mkdtempSync: adjusting(templated),
    readlink: checking(["fs.read", 0, true]),
    realpath: walkingRealpath,
    realpathSync: adjusting(resolved),
    rm: adjusting(briefly),
    rmSync: adjusting(briefly),
    rmdir: checking(["fs.write", 0, true]),
    utimes: checking(["fs.write", 0]),
    watch: checking(["fs.read", 0]),
    watchFile: watchingFile,
};

const FILE_SYSTEM_PROMISES = {
    cp: adjusting(copied),
    lstat: adjusting(briefly),
    mkdtemp: adjusting(templated),
    realpath: bindingRealpath,
    rm: adjusting(briefly),
};

function replace(target, replacements, lstatSync, fs) {
    for (let name of ObjectKeys(replacements)) {
        let original = target[name];
        target[name] = standingIn(original, replacements[name](original, lstatSync, fs));
    }
}

/** Gives `replacement` the name, length, and own properties of `original` it lacks. */
function standingIn(original, replacement) {
    for (let key of ReflectOwnKeys(original)) {
        if (key === "prototype" || key === "arguments" || key === "caller") continue;
        let replaced = key === "name" || key === "length";
        if (!replaced && ObjectGetOwnPropertyDescriptor(replacement, key) !== undefined) continue;
        ObjectDefineProperty(replacement, key, ObjectGetOwnPropertyDescriptor(original, key));
    }
    return replacement;
}

/** A path argument as Node names it in a denial, or `undefined` for anything else. */
function pathName(value) {
    if (typeof value === "string") return value;
    if (ObjectPrototypeIsPrototypeOf(URL.prototype, value)) {
        return core.createLazyLoader("node:url")().fileURLToPath(value);
    }
    if (TypedArrayPrototypeGetSymbolToStringTag(value) === "Uint8Array") {
        return new TextDecoder().decode(value);
    }
}

/**
 * Whether the operation's own check would refuse `path`. The permission query follows a
 * link, as Bento's following operations do; an operation on the link itself is left to its
 * own check, which looks where the link sits.
 */
function refuses(scope, path, onLink, lstatSync) {
    if (held(scope) || op_bento_permission_has(scope, path)) return false;
    if (!onLink) return true;
    try {
        return !lstatSync(path).isSymbolicLink();
    } catch {
        return true;
    }
}

/** Node checks first, throwing from the call itself: `[scope, argument index, onLink]`. */
function checking(...checks) {
    return (original, lstatSync) =>
        function (...args) {
            for (let [scope, index, onLink] of checks) {
                let path = pathName(args[index]);
                if (path && refuses(scope, path, onLink, lstatSync)) throw fileDenied(scope, path);
            }
            return ReflectApply(original, this, args);
        };
}

/** Node reports the refusal as Deno does, with `adjust` applied to the error. */
function adjusting(adjust) {
    return original =>
        function (...args) {
            let refused = error => {
                if (error?.code === "ERR_ACCESS_DENIED") adjust(error, args);
                return error;
            };
            let last = args.length - 1;
            let callback = args[last];
            if (typeof callback === "function") {
                args[last] = function (error, ...results) {
                    return ReflectApply(callback, this, [refused(error), ...results]);
                };
            }
            let result;
            try {
                result = ReflectApply(original, this, args);
            } catch (error) {
                throw refused(error);
            }
            if (!ObjectPrototypeIsPrototypeOf(PromisePrototype, result)) return result;
            return PromisePrototypeCatch(result, error => {
                throw refused(error);
            });
        };
}

function briefly(error) {
    if (error.permission === "FileSystemRead") error.message = LSTAT_REFUSED;
}

/** Node's `cp` checks with `lstat`, or with `stat` when it dereferences links. */
function copied(error, [, , options]) {
    if (!options?.dereference) briefly(error);
}

/** Node names the template, where Deno names the directory it tried to create. */
function templated(error, [prefix]) {
    if (error.permission === "FileSystemWrite") error.resource = `${pathName(prefix)}XXXXXX`;
}

function resolved(error, [path]) {
    if (error.permission === "FileSystemRead") {
        error.resource = pathModule().resolve(pathName(path));
    }
}

/**
 * Node's callback `realpath` resolves the path in JavaScript one component at a time with
 * `lstat`, so it is refused at the first component from the root this thread may not read.
 */
function walkingRealpath(original) {
    return function realpath(path, ...rest) {
        let callback = rest[rest.length - 1];
        let name = pathName(path);
        let component = name && typeof callback === "function" && unreadableComponent(name);
        if (!component) return ReflectApply(original, this, [path, ...rest]);
        let error = fileDenied("fs.read", component);
        briefly(error);
        core.createLazyLoader("node:process")().default.nextTick(callback, error);
    };
}

function unreadableComponent(path) {
    if (held("fs.read")) return undefined;
    let absolute = pathModule().resolve(path);
    for (let end = 0; end < absolute.length;) {
        end = StringPrototypeIndexOf(absolute, "/", end + 1);
        if (end === -1) end = absolute.length;
        let component = StringPrototypeSlice(absolute, 0, end);
        if (!op_bento_permission_has("fs.read", component)) return component;
    }
}

/**
 * Node's promise `realpath` asks its binding, which names the path as given. Deno's
 * promisifies the callback function, so it is built from the one that does not walk.
 */
function bindingRealpath(_original, _lstatSync, fs) {
    let realpath = core.createLazyLoader("node:util")().promisify(fs.realpath);
    return function (path, options) {
        return realpath(path, options);
    };
}

/** Node validates the listener before refusing, and names the resolved path. */
function watchingFile(original, lstatSync) {
    return function watchFile(filename, ...rest) {
        let path = pathName(filename);
        if (path && typeof rest[rest.length - 1] === "function") {
            let absolute = pathModule().resolve(path);
            if (refuses("fs.read", absolute, false, lstatSync)) {
                throw fileDenied("fs.read", absolute);
            }
        }
        return ReflectApply(original, this, [filename, ...rest]);
    };
}

/** Node's `exists`, which answers `false` for a path it may not check. */
function existing(_original, _lstatSync, fs) {
    function exists(path, callback) {
        core.loadExtScript("ext:deno_node/internal/validators.mjs").validateFunction(
            callback,
            "cb",
        );
        try {
            fs.access(path, fs.constants.F_OK, error => callback(!error));
        } catch {
            callback(false);
        }
    }
    ObjectDefineProperty(exists, PROMISIFY_CUSTOM, {
        __proto__: null,
        value: function exists(path) {
            return new Promise(resolve => fs.exists(path, resolve));
        },
    });
    return exists;
}
