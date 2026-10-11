import { readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { setTimeout as sleep } from "node:timers/promises";

import type { AppServer } from "../app-server.ts";
import type { ExpectedEntry, ResponseSnapshot } from "../document-gate.ts";
import type { Workload } from "../workload.ts";

import {
    STARTUP_TIMEOUT_MS,
    waitForDocument,
    waitUntilListening,
    withServer,
} from "../app-server.ts";
import { evaluateDocument } from "../document-gate.ts";
import { exchange, RequestFailure } from "../http-exchange.ts";
import { SEED_ENTRIES } from "../seed-entries.ts";
import { evaluateRedirect } from "../submission-gate.ts";

const RELOAD_TIMEOUT_MS = 30_000;
const POLL_INTERVAL_MS = 5;
const SETTLE_MS = 500;

const SIGNED_BEFORE_EDITS: ExpectedEntry = {
    name: "Reload Survivor",
    message: "Signed before the first edit; it must outlive every reload.",
};

const TITLE_TEXT = "Welcome to Remix 3";
const SERVER_LISTENER_LINE = "let server = http.createServer(createRequestListener(router.fetch));";

interface Edit {
    label: string;
    file: string;
    original: string;
    edited: string;
    shows(snapshot: ResponseSnapshot): boolean;
}

export const watchReload: Workload = {
    name: "watch-reload",
    title: "Watch reload",
    question:
        "With the runtime's watch mode running, how long from editing a file until the first response that shows the change? The transitive edit changes a visible string in a module the entry imports; the entry edit adds a response header in server.ts. The database contents and the listener must survive each reload.",
    metrics: [
        {
            key: "transitiveReloadMs",
            label: "Edit of an imported module to changed response",
            unit: "ms",
        },
        { key: "entryReloadMs", label: "Edit of the entry to changed response", unit: "ms" },
    ],
    variants: [{ label: "development", mode: "development" }],
    requiresApplicationGate: true,
    async runTrial({ runtime, executable, fixture, mode, trial }) {
        let [transitive, entry] = await planEdits(fixture.directory, trial);
        try {
            return await withServer(
                { runtime, executable, fixture, mode, watch: true },
                async server => {
                    await waitUntilListening(server, STARTUP_TIMEOUT_MS);
                    await waitForDocument(server, SEED_ENTRIES, STARTUP_TIMEOUT_MS);
                    await signBeforeEdits(server);
                    let expected = [...SEED_ENTRIES, SIGNED_BEFORE_EDITS];

                    let transitiveReloadMs = await reloadAfter(
                        server,
                        fixture.directory,
                        transitive,
                        expected,
                    );
                    let entryReloadMs = await reloadAfter(
                        server,
                        fixture.directory,
                        entry,
                        expected,
                    );
                    return { trial, transitiveReloadMs, entryReloadMs };
                },
            );
        } finally {
            for (let edit of [transitive, entry]) {
                await writeFile(path.join(fixture.directory, edit.file), edit.original);
            }
        }
    },
};

async function planEdits(directory: string, trial: number): Promise<[Edit, Edit]> {
    let marker = `bench-edit-${trial}`;
    let page = "app/actions/guest-book/page.tsx";
    let entry = "server.ts";
    let pageSource = await readFile(path.join(directory, page), "utf8");
    let entrySource = await readFile(path.join(directory, entry), "utf8");
    return [
        {
            label: "transitive",
            file: page,
            original: pageSource,
            edited: replaceOnce(pageSource, TITLE_TEXT, `${TITLE_TEXT} (${marker})`),
            shows: snapshot => snapshot.body.includes(`${TITLE_TEXT} (${marker})`),
        },
        {
            label: "entry",
            file: entry,
            original: entrySource,
            edited: replaceOnce(entrySource, SERVER_LISTENER_LINE, listenerWithHeader(marker)),
            shows: snapshot => snapshot.headers["x-bench-edit"] === marker,
        },
    ];
}

function listenerWithHeader(marker: string): string {
    return [
        "let requestListener = createRequestListener(router.fetch);",
        "let server = http.createServer((request, response) => {",
        `    response.setHeader("x-bench-edit", "${marker}");`,
        "    return requestListener(request, response);",
        "});",
    ].join("\n");
}

function replaceOnce(source: string, target: string, replacement: string): string {
    if (source.split(target).length !== 2) {
        throw new Error(`Expected exactly one occurrence of ${JSON.stringify(target)} to edit`);
    }
    return source.replace(target, () => replacement);
}

async function signBeforeEdits(server: AppServer): Promise<void> {
    let { snapshot } = await exchange(server.baseUrl, {
        path: "/",
        method: "POST",
        form: { name: SIGNED_BEFORE_EDITS.name, message: SIGNED_BEFORE_EDITS.message },
    });
    let problems = evaluateRedirect(snapshot);
    if (problems.length > 0)
        throw new Error(`Signing before the edits failed: ${problems.join("; ")}`);
}

/** Applies an edit, times the first response that shows it, then reverts and waits for quiet. */
async function reloadAfter(
    server: AppServer,
    directory: string,
    edit: Edit,
    expected: readonly ExpectedEntry[],
): Promise<number> {
    await writeFile(path.join(directory, edit.file), edit.edited);
    let editedAt = performance.now();
    let snapshot = await pollUntil(server, edit.label, response => edit.shows(response));
    let elapsed = performance.now() - editedAt;

    let lost = evaluateDocument(snapshot, expected);
    if (lost.length > 0)
        throw new Error(`State was lost across the ${edit.label} reload: ${lost.join("; ")}`);

    await writeFile(path.join(directory, edit.file), edit.original);
    await pollUntil(server, `revert of ${edit.label}`, response => !edit.shows(response));
    await sleep(SETTLE_MS);
    await waitForDocument(server, expected, STARTUP_TIMEOUT_MS);
    return elapsed;
}

async function pollUntil(
    server: AppServer,
    description: string,
    accept: (snapshot: ResponseSnapshot) => boolean,
): Promise<ResponseSnapshot> {
    let deadline = performance.now() + RELOAD_TIMEOUT_MS;
    while (performance.now() < deadline) {
        let status = server.process.exitStatus();
        if (status !== undefined) {
            throw new Error(`The server exited during ${description}:\n${server.process.output()}`);
        }
        try {
            let { snapshot } = await exchange(server.baseUrl, { path: "/", timeoutMs: 2_000 });
            if (snapshot.status === 200 && accept(snapshot)) return snapshot;
        } catch (error) {
            if (!(error instanceof RequestFailure)) throw error;
        }
        await sleep(POLL_INTERVAL_MS);
    }
    throw new Error(
        `No response reflected the ${description} within ${RELOAD_TIMEOUT_MS} ms:\n${server.process.output()}`,
    );
}
