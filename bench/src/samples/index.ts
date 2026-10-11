import { readFileSync } from "node:fs";

import type { ExpectedEntry } from "../document-gate.ts";

export type SampleName =
    | "development-document"
    | "message-too-long"
    | "production-document"
    | "validation-errors";

/** Pages recorded from the guest book running under Node 26.11.1, seeded with `RECORDED_ENTRIES`. */
export function readSample(name: SampleName): string {
    return readFileSync(new URL(`./${name}.txt`, import.meta.url), "utf8");
}

export const RECORDED_ENTRIES: readonly ExpectedEntry[] = [
    { name: "Ada Lovelace", message: "Notes on the Analytical Engine" },
    { name: "Grace Hopper", message: "Nanoseconds & other lengths <of wire>" },
];
