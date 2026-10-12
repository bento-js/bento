import type { Mode, RuntimeName } from "./results-model.ts";

export interface Command {
    command: string;
    args: string[];
}

const SERVER_ENTRY = "server.ts";

// The fixture README's grants. Reads are unscoped because remix/assets probes package.json files
// in node_modules directories above the project root (Node's package lookup locations).
const BENTO_SERVER_GRANTS = [
    "--allow-read",
    "--allow-write=./db",
    "--allow-net",
    "--allow-env=PORT,DATABASE_URL,NODE_ENV",
    "--allow-addons=./node_modules",
];

// A script that only reads and loads the locked native packages.
const BENTO_SCRIPT_GRANTS = ["--allow-read", "--allow-addons=./node_modules"];

const NODE_LOADER = ["--import", "remix/node-tsx"];

/** The launch path each runtime uses for the fixture's `server.ts`, run from the fixture root. */
export function serverCommand(
    runtime: RuntimeName,
    executable: string,
    options: { watch: boolean },
): Command {
    let watch = options.watch ? ["--watch"] : [];
    switch (runtime) {
        case "node":
            return { command: executable, args: [...watch, ...NODE_LOADER, SERVER_ENTRY] };
        case "bun":
            return { command: executable, args: [...watch, SERVER_ENTRY] };
        case "bento":
            return {
                command: executable,
                args: ["run", ...BENTO_SERVER_GRANTS, ...watch, SERVER_ENTRY],
            };
    }
}

/** Runs a standalone script from the fixture root through the runtime's direct-source path. */
export function scriptCommand(
    runtime: RuntimeName,
    executable: string,
    script: string,
    scriptArguments: readonly string[],
): Command {
    switch (runtime) {
        case "node":
            return { command: executable, args: [...NODE_LOADER, script, ...scriptArguments] };
        case "bun":
            return { command: executable, args: [script, ...scriptArguments] };
        case "bento":
            return {
                command: executable,
                args: ["run", ...BENTO_SCRIPT_GRANTS, script, ...scriptArguments],
            };
    }
}

/** The environment every launched process shares; no variable of the harness's own leaks in. */
export function baseEnvironment(): Record<string, string> {
    return {
        PATH: process.env.PATH ?? "",
        HOME: process.env.HOME ?? "",
        TMPDIR: process.env.TMPDIR ?? "/tmp",
    };
}

/** The same environment for every runtime; Bento's `--allow-env` exposes only these three. */
export function serverEnvironment(options: {
    port: number;
    databaseUrl: string;
    mode: Mode;
}): Record<string, string> {
    return {
        ...baseEnvironment(),
        PORT: String(options.port),
        DATABASE_URL: options.databaseUrl,
        NODE_ENV: options.mode,
    };
}
