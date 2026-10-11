import { copyFile, mkdir, readdir, rm } from "node:fs/promises";
import path from "node:path";
import { DatabaseSync } from "node:sqlite";

export const DATABASE_URL = "db/bench.db";

const DATABASE_DIRECTORY = "db";
const DATABASE_PREFIX = "bench.db";
const SEED_DIRECTORY = ".bench/seed";

/** Keeps the migrated and seeded database (including any SQLite sidecar files) for later resets. */
export async function saveSeedDatabase(fixtureDirectory: string): Promise<void> {
    let seedDirectory = path.join(fixtureDirectory, SEED_DIRECTORY);
    await rm(seedDirectory, { recursive: true, force: true });
    await mkdir(seedDirectory, { recursive: true });
    for (let file of await databaseFiles(path.join(fixtureDirectory, DATABASE_DIRECTORY))) {
        await copyFile(
            path.join(fixtureDirectory, DATABASE_DIRECTORY, file),
            path.join(seedDirectory, file),
        );
    }
}

/** Replaces the live database with the saved seed so every trial starts from identical state. */
export async function resetDatabase(fixtureDirectory: string): Promise<void> {
    let liveDirectory = path.join(fixtureDirectory, DATABASE_DIRECTORY);
    for (let file of await databaseFiles(liveDirectory)) await rm(path.join(liveDirectory, file));
    let seedDirectory = path.join(fixtureDirectory, SEED_DIRECTORY);
    for (let file of await databaseFiles(seedDirectory)) {
        await copyFile(path.join(seedDirectory, file), path.join(liveDirectory, file));
    }
}

export function countEntries(fixtureDirectory: string): number {
    let database = new DatabaseSync(path.join(fixtureDirectory, DATABASE_URL), { readOnly: true });
    try {
        let row = database.prepare("SELECT count(*) AS count FROM guest_book").get();
        return Number(row?.count);
    } finally {
        database.close();
    }
}

async function databaseFiles(directory: string): Promise<string[]> {
    let names = await readdir(directory);
    return names.filter(name => name.startsWith(DATABASE_PREFIX));
}
