// Starting a child process.
import { accessDenied } from "ext:bento_instance/permission/errors.mjs";
import { held } from "ext:bento_instance/permission/process_permission.mjs";
import { core, primordials } from "ext:core/mod.js";

let { ReflectApply } = primordials;

/**
 * Node's process bindings check the permission once JavaScript has validated the arguments
 * and settled on the file to start, which is the shell for `exec` and for `shell`. Deno's
 * layer is built the same way, so the check sits at the same two points.
 */
export function guardChildProcesses() {
    let internal = core.loadExtScript("ext:deno_node/internal/child_process.ts");
    let refuse = file => {
        if (!held("child")) throw accessDenied("ChildProcess", "--allow-child-process", file);
    };
    let { prototype } = internal.ChildProcess;
    let { spawn } = prototype;
    prototype.spawn = function (options) {
        // Without a file to name, the original rejects the options as Node does.
        if (typeof options?.file === "string") refuse(options.file);
        return ReflectApply(spawn, this, arguments);
    };
    let { spawnSync } = internal.default;
    internal.default.spawnSync = function (options) {
        refuse(options.file);
        return ReflectApply(spawnSync, this, arguments);
    };
}
