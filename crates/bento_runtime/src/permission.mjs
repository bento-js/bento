// Node's permission model in the guest: Node's errors for Deno's refusals, the guards that
// refuse where Node does, and `process.permission`. Each concern lives in `permission/`.
import { guardChildProcesses } from "ext:bento_instance/permission/child_process.mjs";
import { installDenialErrors } from "ext:bento_instance/permission/errors.mjs";
import { guardFileSystem } from "ext:bento_instance/permission/file_system.mjs";
import { guardNetwork } from "ext:bento_instance/permission/network.mjs";
import { drop, has } from "ext:bento_instance/permission/process_permission.mjs";
import { guardInspector, guardWasi } from "ext:bento_instance/permission/wasi_inspector.mjs";
import { core, primordials } from "ext:core/mod.js";

let { ObjectDefineProperty } = primordials;
let { op_bento_permission_withholds, op_bento_register_env_warning } = core.ops;

export function install() {
    let process = core.createLazyLoader("node:process")().default;
    installDenialErrors();
    op_bento_register_env_warning(name => {
        process.emitWarning(
            `The permission model removed the environment variable "${name}" at startup. Use --allow-env to manage permissions.`,
        );
    });
    // An instance that withholds nothing can refuse nothing, so it needs none of the guards.
    if (!op_bento_permission_withholds()) return;
    // First, before anything loads `node:net` or `node:dgram`.
    guardNetwork();
    guardChildProcesses();
    guardWasi();
    guardInspector();
    guardFileSystem();
    ObjectDefineProperty(process, "permission", {
        __proto__: null,
        enumerable: true,
        configurable: false,
        value: { has, drop },
    });
}
