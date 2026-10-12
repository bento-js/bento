import * as assert from "remix/assert";
import { describe, it } from "remix/test";

import { createdAtFromRfc3339 } from "#app/data/created-at.ts";

describe("createdAt from a host RFC 3339 timestamp", () => {
    it("matches the data-table form for a UTC timestamp", () => {
        assert.equal(createdAtFromRfc3339("2026-10-10T18:30:05Z"), "2026-10-10 18:30:05");
    });

    it("normalizes offsets to UTC and drops fractional seconds like SQLite", () => {
        assert.equal(createdAtFromRfc3339("2026-10-10T20:30:05.987+02:00"), "2026-10-10 18:30:05");
    });

    it("rejects timestamps that do not name their offset", () => {
        assert.throws(() => createdAtFromRfc3339("2026-10-10 18:30:05"), /RFC 3339/);
    });
});
