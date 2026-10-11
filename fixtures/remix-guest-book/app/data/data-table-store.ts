import type { Database, TableRow } from "remix/data-table";

import type { GuestBookEntry, GuestBookStore } from "#app/data/guest-book-store.ts";

import { guestBook } from "#app/data/schema.ts";

export function createDataTableStore(db: Database): GuestBookStore {
    return {
        async list() {
            let rows = await db.findMany(guestBook, { orderBy: ["id", "asc"] });
            return rows.map(toEntry);
        },
        async append(entry) {
            await db.create(guestBook, entry);
        },
    };
}

// `c.timestamp()` is untyped; the migration stores SQLite `CURRENT_TIMESTAMP` text.
function toEntry({ id, name, message, createdAt }: TableRow<typeof guestBook>): GuestBookEntry {
    if (typeof createdAt !== "string") {
        throw new TypeError(`Expected guest_book.createdAt to be text, got ${typeof createdAt}`);
    }
    return { id, name, message, createdAt };
}
