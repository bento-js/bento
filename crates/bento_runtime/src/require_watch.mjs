import { core, primordials } from "ext:core/mod.js";

let { ObjectKeys, ReflectApply } = primordials;
let recordFailure = core.ops.op_bento_failed_require;
let recordMissingTarget = core.ops.op_bento_missing_require_target;
export function install() {
    let { Module } = core.createLazyLoader("node:module")();
    let loadTarget;
    let load = Module.prototype.load;
    Module.prototype.load = function (filename) {
        loadTarget ??= filename;
        return ReflectApply(load, this, [filename]);
    };
    // Upstream package maps defer missing-target validation until _load.
    for (let name of ["_resolveFilename", "_load"]) {
        let original = Module[name];
        let resolving = name === "_resolveFilename";
        Module[name] = function (request, parent, isMain, options) {
            let previousTarget = loadTarget;
            if (!resolving) loadTarget = undefined;
            try {
                return ReflectApply(original, this, [request, parent, isMain, options]);
            } catch (error) {
                let missing = resolving
                    ? error?.code === "MODULE_NOT_FOUND" || error?.code === "ERR_MODULE_NOT_FOUND"
                    : error?.name === "NotFound" || error?.code === "ENOENT";
                if (
                    missing &&
                    options?.paths === undefined &&
                    typeof request === "string" &&
                    typeof parent?.filename === "string" &&
                    Module._cache[parent.filename] === parent
                ) {
                    if (resolving) {
                        recordFailure(request, parent.filename, ObjectKeys(Module._extensions));
                    } else if (typeof loadTarget === "string") {
                        recordMissingTarget(loadTarget, parent.filename);
                    }
                }
                throw error;
            } finally {
                if (!resolving) loadTarget = previousTarget;
            }
        };
    }
}
