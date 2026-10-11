import { describe, expect, test } from "vite-plus/test";

import { detectBento } from "./runtimes.ts";

describe("Bento availability", () => {
    test("is unavailable, with the reason, when BENTO_BIN is unset", async () => {
        let availability = await detectBento({});

        expect(availability).toEqual({
            available: false,
            reason: "BENTO_BIN is not set; build the bento executable and point BENTO_BIN at it",
        });
    });

    test("is unavailable, naming the path, when BENTO_BIN points at nothing", async () => {
        let availability = await detectBento({ BENTO_BIN: "/tmp/bento-bench/does-not-exist" });

        expect(availability).toEqual({
            available: false,
            reason: "BENTO_BIN=/tmp/bento-bench/does-not-exist is not an executable file",
        });
    });

    test("is unavailable when BENTO_BIN is a directory", async () => {
        let availability = await detectBento({ BENTO_BIN: "/tmp" });

        expect(availability.available).toBe(false);
    });

    test("is available at the executable BENTO_BIN names", async () => {
        let availability = await detectBento({ BENTO_BIN: process.execPath });

        expect(availability).toMatchObject({ available: true, executable: process.execPath });
    });
});
