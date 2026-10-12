import { column as c, table } from "remix/data-table";

export let guestBook = table({
    name: "guest_book",
    columns: {
        id: c.integer().primaryKey(),
        name: c.text().notNull(),
        message: c.text().notNull(),
        createdAt: c.timestamp().defaultNow(),
    },
});
