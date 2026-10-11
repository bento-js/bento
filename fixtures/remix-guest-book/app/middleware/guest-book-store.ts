import { createContextKey, type Middleware } from "remix/router";

import type { GuestBookStore } from "#app/data/guest-book-store.ts";

let guestBookStoreKey = createContextKey<GuestBookStore>();

/** Exposes the guest book's storage to actions as `context.guestBook`. */
export type GuestBookStoreMiddleware = Middleware<{
    key: typeof guestBookStoreKey;
    value: GuestBookStore;
    property: "guestBook";
}>;

export function loadGuestBookStore(store: GuestBookStore): GuestBookStoreMiddleware {
    return (context, next) => {
        context.set(guestBookStoreKey, store, { property: "guestBook" });
        return next();
    };
}
