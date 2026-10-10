import type { TestContext } from "remix/test";

import * as assert from "remix/assert";
import { describe, it } from "remix/test";

import { MAX_MESSAGE_LENGTH } from "#app/actions/guest-book/schema.ts";
import { guestBook } from "#app/data/schema.ts";
import { loadAppMigrations, openDatabase } from "#app/db.ts";
import { createAppRouter } from "#app/router.ts";
import { routes } from "#app/routes.ts";

const ORIGIN = "https://guest-book.test";

async function setup(t: TestContext) {
    let db = openDatabase(":memory:");
    t.after(() => db.close());
    await db.migrate(await loadAppMigrations());

    let router = createAppRouter({ db });

    function sign(fields: Record<string, string>) {
        let body = new FormData();
        for (let [name, value] of Object.entries(fields)) body.set(name, value);

        let url = new URL(routes.guestBook.action.href(), ORIGIN);
        return router.fetch(new Request(url, { method: "POST", body }));
    }

    return { db, router, sign };
}

describe("guest book controller", () => {
    it("lists signed entries", async t => {
        let { db, router } = await setup(t);
        await db.create(guestBook, { name: "Ada", message: "Hello from 1843" });

        let response = await router.fetch(new URL(routes.guestBook.index.href(), ORIGIN));

        assert.equal(response.status, 200);
        let html = await response.text();
        assert.match(html, /Ada/);
        assert.match(html, /Hello from 1843/);
    });

    it("saves a trimmed entry and redirects back to the guest book", async t => {
        let { db, sign } = await setup(t);

        let response = await sign({ name: "  Ada  ", message: "  Hello  " });

        assert.equal(response.status, 303);
        assert.equal(response.headers.get("Location"), routes.guestBook.index.href());
        let entries = await db.findMany(guestBook);
        assert.deepEqual(
            entries.map(({ name, message }) => ({ name, message })),
            [{ name: "Ada", message: "Hello" }],
        );
    });

    it("rejects blank fields with field errors and keeps the submitted values", async t => {
        let { db, sign } = await setup(t);

        let response = await sign({ name: "   ", message: "Still here" });

        assert.equal(response.status, 400);
        let html = await response.text();
        assert.match(html, /Enter your name\./);
        let nameInput = html.match(/<input[^>]*name="name"[^>]*>/)?.[0] ?? "";
        assert.match(nameInput, /aria-invalid="true"/);
        assert.match(html, /Still here/);
        assert.equal(await db.count(guestBook), 0);
    });

    it("accepts messages up to the length limit and rejects longer ones", async t => {
        let { db, sign } = await setup(t);

        let atLimit = await sign({ name: "Ada", message: "x".repeat(MAX_MESSAGE_LENGTH) });
        let overLimit = await sign({ name: "Ada", message: "x".repeat(MAX_MESSAGE_LENGTH + 1) });

        assert.equal(atLimit.status, 303);
        assert.equal(overLimit.status, 400);
        assert.match(await overLimit.text(), /Keep your message to 280 characters or fewer\./);
        assert.equal(await db.count(guestBook), 1);
    });
});
