import { execFile } from "node:child_process";
import { chmod, mkdir, readdir, rename, rm, stat } from "node:fs/promises";
import path from "node:path";
import { promisify } from "node:util";

import type { RunReport } from "../lib/model.ts";

import { CORPUS_DIRECTORIES } from "./corpus.ts";
import { SetupError, describeError, errorCode, errorOutput } from "./errors.ts";

const EXEC_FILE = promisify(execFile);

export const NODE_VERSION = "26.12.1";
export const NODE_TAG = `v${NODE_VERSION}`;
export const NODE_COMMIT = "bc2e01dcf6acd5cf7d169b5f26cf89abaaeb41f1";
export const NODE_REPOSITORY = "https://github.com/nodejs/node.git";

export const CHECKOUT_PATHS = [
    "benchmark",
    "deps/acorn",
    "deps/npm",
    "deps/undici",
    "deps/v8/src/objects",
    "doc",
    "lib",
    "src",
    "test",
    "tools",
];

export const SUITE: RunReport["suite"] = {
    version: NODE_VERSION,
    revision: NODE_COMMIT,
    source: `nodejs/node ${NODE_TAG}`,
    scope: `${CORPUS_DIRECTORIES.map(directory => `test/${directory}`).join(",")} test-*.{js,mjs,cjs}; scratch copies add {"private":true} package.json to each directory because bento run requires a project manifest; NODE_TEST_KNOWN_GLOBALS=0 disables the harness global-leak check for both runtimes`,
};

const APP_ROOT = path.resolve(import.meta.dirname, "..");
export const CACHE_DIRECTORY = path.join(APP_ROOT, ".data", "upstream", `node-${NODE_COMMIT}`);

export async function git(cwd: string, args: string[]): Promise<string> {
    try {
        let { stdout } = await EXEC_FILE("git", args, {
            cwd,
            encoding: "utf8",
            env: { ...process.env, GIT_TERMINAL_PROMPT: "0" },
            maxBuffer: 64 * 1024 * 1024,
        });
        return stdout;
    } catch (error) {
        throw new SetupError(
            `git ${args.join(" ")} failed in ${cwd}: ${errorOutput(error) || describeError(error)}`,
        );
    }
}

export async function verifyCheckout(
    directory: string,
    commit: string,
    paths: string[],
): Promise<void> {
    let head = (await git(directory, ["rev-parse", "--verify", "HEAD^{commit}"])).trim();
    if (head !== commit)
        throw new SetupError(`${directory} is at ${head}, not the pinned ${commit}`);
    let sparse = (await git(directory, ["sparse-checkout", "list"]))
        .split("\n")
        .filter(Boolean)
        .sort();
    if (JSON.stringify(sparse) !== JSON.stringify([...paths].sort())) {
        throw new SetupError(
            `${directory} checks out ${sparse.join(", ")} instead of ${paths.join(", ")}`,
        );
    }
    let status = await git(directory, [
        "--no-optional-locks",
        "status",
        "--porcelain",
        "--untracked-files=all",
        "--ignored=matching",
    ]);
    if (status.trim() !== "") {
        let changes = status.trim().split("\n");
        throw new SetupError(
            `${directory} differs from ${commit}: ${changes.slice(0, 5).join("; ")}${changes.length > 5 ? "; …" : ""}`,
        );
    }
}

export async function verifyUpstream(): Promise<string> {
    if (!(await exists(CACHE_DIRECTORY))) {
        throw new SetupError(
            `The upstream corpus is not prepared at ${CACHE_DIRECTORY}. Run \`mise run compat:prepare\`.`,
        );
    }
    try {
        await verifyCheckout(CACHE_DIRECTORY, NODE_COMMIT, CHECKOUT_PATHS);
    } catch (error) {
        throw new SetupError(
            `${describeError(error)}. Run \`mise run compat:prepare\` to replace the cache.`,
        );
    }
    return CACHE_DIRECTORY;
}

export async function prepareUpstream(log: (line: string) => void): Promise<void> {
    if (await exists(CACHE_DIRECTORY)) {
        try {
            await verifyCheckout(CACHE_DIRECTORY, NODE_COMMIT, CHECKOUT_PATHS);
            log(`Verified Node ${NODE_TAG} (${NODE_COMMIT}) at ${CACHE_DIRECTORY}`);
            return;
        } catch (error) {
            if (!(error instanceof SetupError)) throw error;
            log(`The cached checkout failed verification and will be replaced: ${error.message}`);
            await rm(CACHE_DIRECTORY, { recursive: true, force: true });
        }
    }
    let partial = `${CACHE_DIRECTORY}.partial-${process.pid}`;
    await rm(partial, { recursive: true, force: true });
    await mkdir(partial, { recursive: true });
    try {
        await git(partial, ["init", "--quiet"]);
        await git(partial, ["remote", "add", "origin", NODE_REPOSITORY]);
        await git(partial, ["sparse-checkout", "set", "--cone", ...CHECKOUT_PATHS]);
        log(`Fetching ${NODE_REPOSITORY} at ${NODE_COMMIT} (${NODE_TAG})`);
        await git(partial, [
            "fetch",
            "--quiet",
            "--depth=1",
            "--filter=blob:none",
            "--no-tags",
            "origin",
            NODE_COMMIT,
        ]);
        await git(partial, [
            "-c",
            "advice.detachedHead=false",
            "checkout",
            "--quiet",
            "--detach",
            NODE_COMMIT,
        ]);
        await removeWritePermission(partial);
        await git(partial, ["update-index", "-q", "--refresh"]);
        await verifyCheckout(partial, NODE_COMMIT, CHECKOUT_PATHS);
        await rename(partial, CACHE_DIRECTORY);
    } catch (error) {
        await rm(partial, { recursive: true, force: true });
        throw error;
    }
    log(`Prepared Node ${NODE_TAG} (${NODE_COMMIT}) at ${CACHE_DIRECTORY}`);
}

async function removeWritePermission(directory: string): Promise<void> {
    let gitDirectory = path.join(directory, ".git");
    let entries = await readdir(directory, { recursive: true, withFileTypes: true });
    for (let entry of entries) {
        let file = path.join(entry.parentPath, entry.name);
        if (!entry.isFile() || file.startsWith(gitDirectory + path.sep)) continue;
        let { mode } = await stat(file);
        await chmod(file, mode & ~0o222);
    }
}

async function exists(file: string): Promise<boolean> {
    try {
        await stat(file);
        return true;
    } catch (error) {
        if (errorCode(error) === "ENOENT") return false;
        throw error;
    }
}
