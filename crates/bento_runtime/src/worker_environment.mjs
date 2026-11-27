import { core, primordials } from "ext:core/mod.js";

let { ObjectEntries, ObjectFromEntries } = primordials;
let { op_bento_own_environment } = core.ops;

/**
 * A worker's `process.env` starts as a copy of its creator's, as in Node, unless the worker
 * shares the environment. Moving the copy into an environment of the worker's own makes every
 * way to it agree: `process.env`, `Deno.env`, the processes the worker starts, and
 * `process.permission.drop('env', name)` all see the worker's variables and nobody else's.
 */
export function install() {
    let process = core.createLazyLoader("node:process")().default;
    let shared = core.loadExtScript("ext:deno_node/_process/process.ts").env;
    if (process.env === shared) return;
    op_bento_own_environment(ObjectFromEntries(ObjectEntries(process.env)));
    process.env = shared;
}
