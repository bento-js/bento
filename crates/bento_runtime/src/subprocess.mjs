// Guest imports of `node:child_process` resolve here, so the named exports are the functions
// Bento's permission layer wrapped on the module object rather than the originals the module
// namespace binds.
import childProcess from "node:child_process";

export const { ChildProcess, exec, execFile, execFileSync, execSync, fork, spawn, spawnSync } =
    childProcess;
export default childProcess;
