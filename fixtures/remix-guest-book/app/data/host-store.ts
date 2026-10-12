import * as host from "host:guestbook";

import type { GuestBookStore } from "#app/data/guest-book-store.ts";

import { createdAtFromRfc3339 } from "#app/data/created-at.ts";

export let hostStore: GuestBookStore = {
    async list() {
        let entries = await host.list();
        return entries.map(({ id, name, message, createdAt }) => ({
            id,
            name,
            message,
            createdAt: createdAtFromRfc3339(createdAt),
        }));
    },
    async append({ name, message }) {
        await host.append({ name, message });
    },
};
