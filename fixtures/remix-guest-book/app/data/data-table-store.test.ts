import type { TestContext } from "remix/test";

import * as assert from "remix/assert";
import { describe, it } from "remix/test";

import { createDataTableStore } from "#app/data/data-table-store.ts";
import { loadAppMigrations, openDatabase } from "#app/db.ts";

async function openGuestBook(t: TestContext) {
    let db = openDatabase(":memory:");
    t.after(() => db.close());
    await db.migrate(await loadAppMigrations());
    return createDataTableStore(db);
}

describe("data-table guest book store", () => {
    it("lists nothing before anyone signs", async t => {
        let guestBook = await openGuestBook(t);

        assert.deepEqual(await guestBook.list(), []);
    });

    it("lists appended entries oldest first with numeric ids", async t => {
        let guestBook = await openGuestBook(t);

        await guestBook.append({ name: "Ada", message: "Hello from 1843" });
        await guestBook.append({ name: "Grace", message: "Hello from 1952" });

        let entries = await guestBook.list();
        assert.deepEqual(
            entries.map(({ id, name, message }) => ({ id, name, message })),
            [
                { id: 1, name: "Ada", message: "Hello from 1843" },
                { id: 2, name: "Grace", message: "Hello from 1952" },
            ],
        );
    });

    it("presents createdAt as the UTC second the entry was saved, in SQLite timestamp text", async t => {
        let guestBook = await openGuestBook(t);
        let before = Math.floor(Date.now() / 1000) * 1000;

        await guestBook.append({ name: "Ada", message: "Hello" });

        let after = Date.now();
        let [entry] = await guestBook.list();
        assert.equal(typeof entry.createdAt, "string");
        assert.match(entry.createdAt, /^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}$/);
        let savedAt = Date.parse(`${entry.createdAt.replace(" ", "T")}Z`);
        assert.ok(savedAt >= before && savedAt <= after, `${entry.createdAt} is not now in UTC`);
    });
});
