import { createContextKey, type Middleware } from "remix/router";

import type { ResultStore } from "../../lib/model.ts";

let resultStoreKey = createContextKey<ResultStore>();

export type ResultStoreMiddleware = Middleware<{
    key: typeof resultStoreKey;
    value: ResultStore;
    property: "results";
}>;

export function loadResultStore(store: ResultStore): ResultStoreMiddleware {
    return (context, next) => {
        context.set(resultStoreKey, store, { property: "results" });
        return next();
    };
}
