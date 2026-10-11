import childProcess from "node:child_process";

function denied() {
    throw new Deno.errors.NotCapable("Guest subprocess execution is unavailable in Bento");
}

for (let name of ["spawn", "spawnSync", "exec", "execFile", "execSync", "execFileSync", "fork"]) {
    childProcess[name] = denied;
}
childProcess.ChildProcess.prototype.spawn = denied;

export const spawn = denied;
export const spawnSync = denied;
export const exec = denied;
export const execFile = denied;
export const execSync = denied;
export const execFileSync = denied;
export const fork = denied;
export const ChildProcess = childProcess.ChildProcess;
export default childProcess;
