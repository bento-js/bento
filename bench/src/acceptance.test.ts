import type { AddressInfo } from "node:net";

import { once } from "node:events";
import { createServer, type IncomingMessage, type ServerResponse } from "node:http";
import { promisify } from "node:util";
import { afterEach, describe, expect, test } from "vite-plus/test";

import type { ExpectedEntry } from "./document-gate.ts";
import type { Mode } from "./results-model.ts";

import { failingCheck, runAcceptance } from "./acceptance.ts";

type Behavior =
    | "accepts-invalid"
    | "correct"
    | "drops-entry"
    | "empty-document"
    | "unminified-assets";

const SEEDED: readonly ExpectedEntry[] = [
    { name: "Ada Lovelace", message: "Notes on the Analytical Engine" },
    { name: "Grace Hopper", message: "Nanoseconds & other lengths <of wire>" },
];

const READABLE_SCRIPT = Array.from({ length: 40 }, (_, line) => `    let v${line} = ${line};`);
const MINIFIED_SCRIPT = "export const a=1;".repeat(80);

function escapeHtml(text: string): string {
    return text.replaceAll("&", "&amp;").replaceAll("<", "&lt;").replaceAll(">", "&gt;");
}

interface Rejection {
    name: string;
    message: string;
    errors: Partial<Record<"message" | "name", string>>;
}

function renderPage(entries: readonly ExpectedEntry[], mode: Mode, rejected?: Rejection): string {
    let fingerprint = mode === "production" ? ".@Abc123" : "";
    let items = entries
        .map(
            ({ name, message }) =>
                `<li><p>${escapeHtml(name)}</p><p>${escapeHtml(message)}</p></li>`,
        )
        .join("");
    let errors = Object.entries(rejected?.errors ?? {})
        .map(([field, text]) => `<p id="guest-book-${field}-error">${text}</p>`)
        .join("");
    return (
        `<!DOCTYPE html><html lang="en"><head><link href="/assets/page${fingerprint}.css" rel="stylesheet" />` +
        `<script type="importmap">{"imports":{"/assets/dep.js":"/assets/dep${fingerprint}.js"}}</script>` +
        `<link href="/assets/dep${fingerprint}.js" rel="modulepreload" />` +
        `<script src="/assets/entry${fingerprint}.ts" type="module"></script></head><body><main>` +
        `<h2 id="guest-book-heading">Guest Book</h2><ul>${items}</ul>` +
        `<form action="/" method="POST"><input name="name" value="${escapeHtml(rejected?.name ?? "")}" />` +
        `<textarea name="message">${escapeHtml(rejected?.message ?? "")}</textarea>${errors}` +
        `<button type="submit">Sign</button></form></main></body></html>`
    );
}

function validate(name: string, message: string): Rejection["errors"] {
    return {
        ...(name.length === 0 && { name: "Enter your name." }),
        ...(message.length === 0 && { message: "Write a message to sign the guest book." }),
        ...(message.length > 280 && { message: "Keep your message to 280 characters or fewer." }),
    };
}

async function readForm(request: IncomingMessage): Promise<URLSearchParams> {
    let chunks: Buffer[] = [];
    for await (let chunk of request) chunks.push(chunk as Buffer);
    return new URLSearchParams(Buffer.concat(chunks).toString("utf8"));
}

async function startFakeGuestBook(behavior: Behavior, mode: Mode) {
    let entries = [...SEEDED];

    async function handle(request: IncomingMessage, response: ServerResponse) {
        let url = request.url ?? "/";
        if (request.method === "POST") {
            let form = await readForm(request);
            let name = (form.get("name") ?? "").trim();
            let message = (form.get("message") ?? "").trim();
            let errors = validate(name, message);
            if (Object.keys(errors).length > 0 && behavior !== "accepts-invalid") {
                response.writeHead(400, { "content-type": "text/html; charset=UTF-8" });
                let rejected = {
                    name: form.get("name") ?? "",
                    message: form.get("message") ?? "",
                    errors,
                };
                response.end(renderPage(entries, mode, rejected));
                return;
            }
            if (behavior !== "drops-entry") entries.push({ name, message });
            response.writeHead(303, { location: "/" }).end();
            return;
        }
        if (url.startsWith("/assets/")) {
            let isStylesheet = url.endsWith(".css");
            let readable = mode === "development" || behavior === "unminified-assets";
            response.writeHead(200, {
                "content-type": isStylesheet
                    ? "text/css; charset=utf-8"
                    : "application/javascript; charset=utf-8",
            });
            response.end(
                isStylesheet
                    ? "body{margin:0}"
                    : readable
                      ? READABLE_SCRIPT.join("\n")
                      : MINIFIED_SCRIPT,
            );
            return;
        }
        response.writeHead(200, { "content-type": "text/html; charset=UTF-8" });
        response.end(behavior === "empty-document" ? "" : renderPage(entries, mode));
    }

    let server = createServer((request, response) => void handle(request, response));
    server.listen(0, "127.0.0.1");
    await once(server, "listening");
    let { port } = server.address() as AddressInfo;

    async function close() {
        server.closeAllConnections();
        await promisify(server.close.bind(server))();
    }

    return { baseUrl: `http://127.0.0.1:${port}`, close };
}

let running: Array<{ close: () => Promise<void> }> = [];

afterEach(async () => {
    await Promise.all(running.map(server => server.close()));
    running = [];
});

async function accept(behavior: Behavior, mode: Mode) {
    let server = await startFakeGuestBook(behavior, mode);
    running.push(server);
    return runAcceptance({ baseUrl: server.baseUrl, mode, seededEntries: SEEDED });
}

describe("a compatible guest book", () => {
    test("passes all four gates in production", async () => {
        let report = await accept("correct", "production");

        expect(failingCheck(report)).toBeUndefined();
        expect(report.passed).toBe(true);
        expect(report.checks.map(check => check.name)).toEqual([
            "document",
            "assets",
            "valid-submission",
            "invalid-submissions",
        ]);
    });

    test("passes all four gates in development", async () => {
        let report = await accept("correct", "development");

        expect(report.passed).toBe(true);
    });
});

describe("an incompatible guest book", () => {
    test("an empty 200 document fails the first gate with response evidence and stops", async () => {
        let report = await accept("empty-document", "production");

        let failed = failingCheck(report);
        expect(report.passed).toBe(false);
        expect(failed?.name).toBe("document");
        expect(failed?.failures).toEqual(["response body is empty"]);
        expect(failed?.evidence[0]).toMatchObject({ method: "GET", status: 200, bodyBytes: 0 });
        expect(report.checks).toHaveLength(1);
    });

    test("unminified production assets fail the asset gate naming the URL", async () => {
        let report = await accept("unminified-assets", "production");

        let failed = failingCheck(report);
        expect(failed?.name).toBe("assets");
        expect(failed?.failures.join("\n")).toContain("/assets/entry.@Abc123.ts");
        expect(failed?.failures.join("\n")).toContain("script is not minified");
    });

    test("a server that stores nothing fails the valid-submission gate", async () => {
        let report = await accept("drops-entry", "development");

        let failed = failingCheck(report);
        expect(failed?.name).toBe("valid-submission");
        expect(failed?.failures.join("\n")).toContain("entries");
    });

    test("a server that accepts invalid entries fails the invalid-submission gate", async () => {
        let report = await accept("accepts-invalid", "development");

        let failed = failingCheck(report);
        expect(failed?.name).toBe("invalid-submissions");
        expect(failed?.failures.join("\n")).toContain("expected status 400, received 303");
    });

    test("a refused connection fails the first gate without evidence", async () => {
        let server = await startFakeGuestBook("correct", "development");
        await server.close();

        let report = await runAcceptance({
            baseUrl: server.baseUrl,
            mode: "development",
            seededEntries: SEEDED,
        });

        let failed = failingCheck(report);
        expect(failed?.name).toBe("document");
        expect(failed?.failures[0]).toMatch(/^GET \/ failed: /);
        expect(failed?.evidence).toEqual([]);
    });
});
