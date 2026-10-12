import { asyncContext } from "remix/middleware/async-context";
import { compression } from "remix/middleware/compression";
import { formData } from "remix/middleware/form-data";
import { logger } from "remix/middleware/logger";
import { render } from "remix/middleware/render";
import { staticFiles } from "remix/middleware/static";
import { createRouter, type MiddlewareContext } from "remix/router";

import type { GuestBookStore } from "#app/data/guest-book-store.ts";
import type { AssetEntryMiddleware } from "#app/middleware/asset-entry.ts";
import type { GuestBookStoreMiddleware } from "#app/middleware/guest-book-store.ts";

import rootController from "#app/actions/controller.ts";
import guestBookController from "#app/actions/guest-book/controller.tsx";
import { loadAssetEntry } from "#app/middleware/asset-entry.ts";
import { loadGuestBookStore } from "#app/middleware/guest-book-store.ts";
import { routes } from "#app/routes.ts";
import { assets } from "#app/utils/assets.ts";

let formDataMiddleware = formData();
let renderMiddleware = render({ assets });

type AppContext = MiddlewareContext<
    [
        typeof formDataMiddleware,
        GuestBookStoreMiddleware,
        AssetEntryMiddleware,
        typeof renderMiddleware,
    ]
>;

declare module "remix" {
    interface RouterTypes {
        context: AppContext;
    }
}

export interface AppRouterOptions {
    guestBook: GuestBookStore;
}

export function createAppRouter({ guestBook }: AppRouterOptions) {
    let router = createRouter<AppContext>({
        middleware: [
            ...(process.env.NODE_ENV === "development" ? [logger()] : []),
            compression(),
            staticFiles("./public", { index: false }),
            formDataMiddleware,
            asyncContext(),
            loadGuestBookStore(guestBook),
            loadAssetEntry(),
            renderMiddleware,
        ],
    });

    router.map(routes, rootController);
    router.map(routes.guestBook, guestBookController);

    return router;
}
