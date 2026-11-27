import { describe, expect, test } from "vitest";

import type { ResponseSnapshot } from "./document-gate.ts";

import { assetUrlsOf, entryScriptUrl, evaluateDocument } from "./document-gate.ts";
import { readSample, RECORDED_ENTRIES } from "./samples/index.ts";

function respond(body: string, status = 200): ResponseSnapshot {
    return { status, headers: { "content-type": "text/html; charset=UTF-8" }, body };
}

describe("a complete guest-book document", () => {
    test("recorded production page passes", () => {
        let problems = evaluateDocument(
            respond(readSample("production-document")),
            RECORDED_ENTRIES,
        );

        expect(problems).toEqual([]);
    });

    test("recorded development page passes", () => {
        let problems = evaluateDocument(
            respond(readSample("development-document")),
            RECORDED_ENTRIES,
        );

        expect(problems).toEqual([]);
    });
});

describe("a document that is not the guest book", () => {
    test("an empty 200 body fails, as Bun's current server-render failure produces", () => {
        let problems = evaluateDocument(respond(""), RECORDED_ENTRIES);

        expect(problems).toEqual(["response body is empty"]);
    });

    test("a non-200 status fails", () => {
        let problems = evaluateDocument(
            respond(readSample("production-document"), 500),
            RECORDED_ENTRIES,
        );

        expect(problems).toContain("expected status 200, received 500");
    });

    test("a truncated body fails", () => {
        let page = readSample("production-document");
        let truncated = page.slice(0, page.indexOf("<main"));

        let problems = evaluateDocument(respond(truncated), RECORDED_ENTRIES);

        expect(problems).toContain("document ends before the closing </html>");
        expect(problems.join("\n")).toContain("guest-book heading");
        expect(problems.join("\n")).toContain("sign form");
    });

    test("an error page fails even with status 200", () => {
        let problems = evaluateDocument(
            respond("<!DOCTYPE html><html><body>Internal error</body></html>"),
            RECORDED_ENTRIES,
        );

        expect(problems.length).toBeGreaterThan(0);
        expect(problems.join("\n")).toContain("guest-book heading");
    });
});

describe("entries", () => {
    test("different entries fail with the expected and rendered lists", () => {
        let problems = evaluateDocument(respond(readSample("production-document")), [
            { name: "Ada Lovelace", message: "Notes on the Analytical Engine" },
        ]);

        expect(problems).toHaveLength(1);
        expect(problems[0]).toContain("entries");
        expect(problems[0]).toContain("Grace Hopper");
    });

    test("entries in a different order fail", () => {
        let problems = evaluateDocument(
            respond(readSample("production-document")),
            [...RECORDED_ENTRIES].reverse(),
        );

        expect(problems).toHaveLength(1);
        expect(problems[0]).toContain("entries");
    });

    test("escaped markup in an entry is compared as text", () => {
        let page = readSample("production-document");

        expect(page).toContain("Nanoseconds &amp; other lengths &lt;of wire&gt;");
        expect(evaluateDocument(respond(page), RECORDED_ENTRIES)).toEqual([]);
    });
});

describe("browser enhancement markup", () => {
    test("a page without the import map fails", () => {
        let page = readSample("production-document").replace(/type="importmap"/, 'type="text/x"');

        expect(evaluateDocument(respond(page), RECORDED_ENTRIES)).toEqual([
            "import map is missing or is not JSON with an imports object",
        ]);
    });

    test("a page without the module script fails", () => {
        let page = readSample("production-document").replace(/type="module"/, 'type="text/x"');

        expect(evaluateDocument(respond(page), RECORDED_ENTRIES)).toEqual([
            "module script with a src is missing",
        ]);
    });

    test("a page without the stylesheet link fails", () => {
        let page = readSample("production-document").replace(/rel="stylesheet"/, 'rel="x"');

        expect(evaluateDocument(respond(page), RECORDED_ENTRIES)).toEqual([
            "stylesheet link is missing",
        ]);
    });

    test("a form without the message field fails", () => {
        let page = readSample("production-document").replace(/name="message"/g, 'name="body"');

        expect(evaluateDocument(respond(page), RECORDED_ENTRIES)).toEqual([
            "sign form needs a name input, a message textarea, and a POST action of /",
        ]);
    });
});

describe("asset discovery", () => {
    test("collects the stylesheet, script, preloads, and import-map targets once each", () => {
        let urls = assetUrlsOf(readSample("production-document"));

        expect(urls).toHaveLength(57);
        expect(new Set(urls).size).toBe(urls.length);
        expect(urls).toContain("/assets/app/actions/public/preflight.@DeqRbq.css");
        expect(urls).toContain("/assets/app/actions/public/entry.@rl6cLT.ts");
        expect(urls.every(url => url.startsWith("/assets/"))).toBe(true);
    });

    test("finds the entry script that the page loads as a module", () => {
        expect(entryScriptUrl(readSample("production-document"))).toBe(
            "/assets/app/actions/public/entry.@rl6cLT.ts",
        );
        expect(entryScriptUrl(readSample("development-document"))).toBe(
            "/assets/app/actions/public/entry.ts",
        );
    });

    test("includes import-map scope targets, which the page loads but no tag names", () => {
        let development = assetUrlsOf(readSample("development-document"));

        expect(development).toContain(
            "/assets/npm/.pnpm/es-module-lexer%402.3.2/node_modules/es-module-lexer/dist/lexer.js",
        );
        expect(development).toHaveLength(56);
    });

    test("a scope target is an asset even when no other markup mentions it", () => {
        let page = [
            '<script type="importmap">',
            JSON.stringify({
                imports: { "#app/a.ts": "/assets/a.ts" },
                scopes: {
                    "/assets/npm/": { lexer: "/assets/npm/lexer.js", "#app/a.ts": "/assets/a.ts" },
                },
            }),
            "</script>",
        ].join("");

        expect(assetUrlsOf(page)).toEqual(["/assets/a.ts", "/assets/npm/lexer.js"]);
    });

    test("an empty page has no assets", () => {
        expect(assetUrlsOf("")).toEqual([]);
        expect(entryScriptUrl("")).toBeUndefined();
    });
});
