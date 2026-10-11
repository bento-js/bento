import type { ExpectedEntry } from "./document-gate.ts";

/** Signed through the application's own `POST /` before any runtime is measured. */
export const SEED_ENTRIES: readonly ExpectedEntry[] = [
    { name: "Ada Lovelace", message: "Notes on the Analytical Engine" },
    { name: "Grace Hopper", message: "Nanoseconds & other lengths <of wire>" },
    { name: "Margaret Hamilton", message: 'Landed "on the Moon" with software' },
    { name: "Katherine Johnson", message: "Check the numbers again." },
    { name: "Alan Turing", message: "Can machines think? ☺" },
];

/** The valid entry the acceptance checker signs. */
export const ACCEPTANCE_ENTRY: ExpectedEntry = {
    name: "Acceptance Checker",
    message: "Signed to prove submissions are stored.",
};
