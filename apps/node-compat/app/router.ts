import { compression } from "remix/middleware/compression";
import { csp } from "remix/middleware/csp";
import { logger } from "remix/middleware/logger";
import { render } from "remix/middleware/render";
import { createRouter, type MiddlewareContext } from "remix/router";

import type { ResultStoreMiddleware } from "#app/middleware/result-store.ts";

import rootController from "#app/actions/controller.tsx";
import runsController from "#app/actions/runs/controller.tsx";
import { loadResultStore } from "#app/middleware/result-store.ts";
import { routes } from "#app/routes.ts";

import type { ResultStore } from "../lib/model.ts";

let renderMiddleware = render();

type AppContext = MiddlewareContext<[ResultStoreMiddleware, typeof renderMiddleware]>;

declare module "remix" {
    interface RouterTypes {
        context: AppContext;
    }
}

export interface AppRouterOptions {
    results: ResultStore;
}

export function createAppRouter({ results }: AppRouterOptions) {
    let router = createRouter<AppContext>({
        middleware: [
            ...(process.env.NODE_ENV === "development" ? [logger()] : []),
            compression(),
            // Logs and test paths come from executed upstream code; the dashboard itself runs no scripts.
            csp({
                policy: {
                    defaultSrc: ["'self'"],
                    scriptSrc: ["'none'"],
                    styleSrc: ["'self'", "'unsafe-inline'"],
                    objectSrc: ["'none'"],
                    baseUri: ["'none'"],
                    formAction: ["'self'"],
                    frameAncestors: ["'none'"],
                },
            }),
            loadResultStore(results),
            renderMiddleware,
        ],
    });

    router.map(routes, rootController);
    router.map(routes.runs, runsController);

    return router;
}
