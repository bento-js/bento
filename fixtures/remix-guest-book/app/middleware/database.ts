import type { Database } from "remix/data-table";

import { createContextKey, type Middleware } from "remix/router";

let databaseKey = createContextKey<Database>();

/** Exposes the app database to actions as `context.db`. */
export type DatabaseMiddleware = Middleware<{
    key: typeof databaseKey;
    value: Database;
    property: "db";
}>;

export function loadDatabase(db: Database): DatabaseMiddleware {
    return (context, next) => {
        context.set(databaseKey, db, { property: "db" });
        return next();
    };
}
