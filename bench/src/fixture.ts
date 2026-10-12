import { copyFile, mkdir, realpath, symlink } from "node:fs/promises";
import path from "node:path";

import { DATABASE_URL } from "./database.ts";
import { findOnPath } from "./executables.ts";
import { baseEnvironment } from "./launch-commands.ts";
import { readPackageVersion } from "./package-version.ts";
import { runToCompletion } from "./processes.ts";
import { seedDatabase } from "./seed-database.ts";

export interface Fixture {
    /** A disposable copy: installed, migrated, and seeded. */
    directory: string;
    databaseUrl: string;
    remixVersion: string;
}

/** The locked packages the asset pipeline calls, linked into `.bench/node_modules` for the probe. */
const ASSET_PACKAGES = [
    "oxc-parser",
    "oxc-resolver",
    "oxc-transform",
    "oxc-minify",
    "lightningcss",
];

const PROBE_SOURCE = new URL("./asset-operations/probe.ts", import.meta.url);

export async function prepareFixture(options: {
    sourceDirectory: string;
    directory: string;
    nodeExecutable: string;
}): Promise<Fixture> {
    let { sourceDirectory, directory, nodeExecutable } = options;
    await mkdir(directory, { recursive: true });
    await copySource(sourceDirectory, directory);
    await installDependencies(directory);
    await migrate(directory);

    let fixture: Fixture = {
        directory,
        databaseUrl: DATABASE_URL,
        remixVersion: await readPackageVersion(path.join(directory, "node_modules/remix")),
    };
    await seedDatabase(fixture, nodeExecutable);
    await installProbe(directory);
    return fixture;
}

async function copySource(sourceDirectory: string, directory: string): Promise<void> {
    await runToCompletion(
        {
            command: "rsync",
            args: [
                "-a",
                "--exclude=node_modules",
                "--exclude=db/*.db",
                "--exclude=db/*.db-*",
                `${sourceDirectory}/`,
                `${directory}/`,
            ],
        },
        { cwd: sourceDirectory, env: baseEnvironment(), label: "rsync of the fixture" },
    );
}

async function installDependencies(directory: string): Promise<void> {
    let pnpm = await findOnPath("pnpm", process.env);
    if (pnpm === undefined) throw new Error("pnpm 12 is required on PATH to install the fixture");
    await runToCompletion(
        { command: pnpm, args: ["install", "--frozen-lockfile"] },
        { cwd: directory, env: process.env, label: "pnpm install --frozen-lockfile" },
    );
}

async function migrate(directory: string): Promise<void> {
    await runToCompletion(
        { command: path.join(directory, "node_modules/.bin/remix"), args: ["db", "migrate"] },
        {
            cwd: directory,
            env: { ...process.env, DATABASE_URL },
            label: "remix db migrate",
        },
    );
}

/**
 * Places the probe in the fixture next to symlinks to the very packages `remix/assets` resolves,
 * so each runtime loads the locked native modules from their real pnpm locations.
 */
async function installProbe(directory: string): Promise<void> {
    let benchDirectory = path.join(directory, ".bench");
    let linkDirectory = path.join(benchDirectory, "node_modules");
    await mkdir(linkDirectory, { recursive: true });
    await copyFile(PROBE_SOURCE, path.join(benchDirectory, "probe.ts"));

    let remixDirectory = await realpath(path.join(directory, "node_modules/remix"));
    let assetsDirectory = await realpath(path.join(remixDirectory, "..", "@remix-run", "assets"));
    for (let name of ASSET_PACKAGES) {
        let target = await realpath(path.resolve(assetsDirectory, "..", "..", name));
        await symlink(target, path.join(linkDirectory, name));
    }
}
