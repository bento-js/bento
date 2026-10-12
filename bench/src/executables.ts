import { execFile } from "node:child_process";
import { access, constants, stat } from "node:fs/promises";
import path from "node:path";
import { promisify } from "node:util";

let execFileAsync = promisify(execFile);

/** Runs a short-lived command and returns its trimmed stdout; a failure throws. */
export async function capture(command: string, args: string[], cwd?: string): Promise<string> {
    let { stdout } = await execFileAsync(command, args, { cwd });
    return stdout.trim();
}

export async function isExecutableFile(candidate: string): Promise<boolean> {
    try {
        if (!(await stat(candidate)).isFile()) return false;
        await access(candidate, constants.X_OK);
        return true;
    } catch {
        return false;
    }
}

export async function findOnPath(
    name: string,
    environment: NodeJS.ProcessEnv,
): Promise<string | undefined> {
    for (let directory of (environment.PATH ?? "").split(path.delimiter)) {
        let candidate = path.join(directory, name);
        if (directory !== "" && (await isExecutableFile(candidate))) return candidate;
    }
    return undefined;
}
