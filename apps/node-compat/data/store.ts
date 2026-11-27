import { mkdir } from "node:fs/promises";
import path from "node:path";
import { loadMigrations } from "remix/data-table/migrations/node";
import { createSqliteDatabase } from "remix/data-table/sqlite";

import type { ResultStore, RunReport, RunSummary } from "../lib/model.ts";

import { summarizeRun } from "../lib/summary.ts";
import { runs } from "./schema.ts";

const APP_ROOT = path.resolve(import.meta.dirname, "..");

export async function openStore(filename = ".data/results.sqlite"): Promise<ResultStore> {
    let databasePath = filename === ":memory:" ? filename : path.resolve(APP_ROOT, filename);
    if (databasePath !== ":memory:") await mkdir(path.dirname(databasePath), { recursive: true });
    let db = createSqliteDatabase({ filename: databasePath });
    try {
        await db.executeScript("PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000;");
        await db.migrate(await loadMigrations(path.join(import.meta.dirname, "migrations")));
    } catch (error) {
        await db.close();
        throw error;
    }
    return {
        async list() {
            let rows = await db.query(runs).select("summary").orderBy("startedAt", "desc").all();
            return rows.map(row => JSON.parse(row.summary) as RunSummary);
        },
        async get(id) {
            let row = await db.find(runs, id);
            return row === null ? null : (JSON.parse(row.report) as RunReport);
        },
        async save(report) {
            await db.query(runs).upsert({
                id: report.id,
                startedAt: report.startedAt,
                summary: JSON.stringify(summarizeRun(report)),
                report: JSON.stringify(report),
            });
        },
        close: () => db.close(),
    };
}
