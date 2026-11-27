import { describe, expect, test } from "vitest";

import { parseProcessTable, treeRssKb } from "./process-memory.ts";

describe("process table", () => {
    test("parses pid, parent pid, and resident kilobytes from ps output", () => {
        let table = parseProcessTable("  101     1   2048\n  202   101 307200\n\n");

        expect(table).toEqual([
            { pid: 101, parentPid: 1, rssKb: 2048 },
            { pid: 202, parentPid: 101, rssKb: 307200 },
        ]);
    });
});

describe("process tree resident size", () => {
    let table = [
        { pid: 1, parentPid: 0, rssKb: 100 },
        { pid: 10, parentPid: 1, rssKb: 1_000 },
        { pid: 11, parentPid: 10, rssKb: 2_000 },
        { pid: 12, parentPid: 11, rssKb: 4_000 },
        { pid: 20, parentPid: 1, rssKb: 8_000 },
    ];

    test("sums the root and every descendant, nothing else", () => {
        expect(treeRssKb(table, 10)).toBe(7_000);
        expect(treeRssKb(table, 11)).toBe(6_000);
        expect(treeRssKb(table, 12)).toBe(4_000);
    });

    test("a root that has exited contributes nothing", () => {
        expect(treeRssKb(table, 999)).toBe(0);
    });
});
