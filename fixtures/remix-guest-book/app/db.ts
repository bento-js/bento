import * as path from "node:path";
import { loadMigrations } from "remix/data-table/migrations/node";
import { createSqliteDatabase } from "remix/data-table/sqlite";

const PROJECT_ROOT = path.resolve(import.meta.dirname, "..");

/** Opens the app's SQLite database. Relative paths resolve from the project root, like `remix db`. */
export function openDatabase(filename: string) {
    return createSqliteDatabase({
        filename: filename === ":memory:" ? filename : path.resolve(PROJECT_ROOT, filename),
    });
}

export function loadAppMigrations() {
    return loadMigrations(path.join(PROJECT_ROOT, "db/migrations"));
}
