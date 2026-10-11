import { describe, expect, test } from "vite-plus/test";

import type { ResponseSnapshot } from "./document-gate.ts";

import { evaluateAsset } from "./asset-gate.ts";

const MINIFIED_SCRIPT = `export function render(a){return a.map(b=>b.name).join(", ")}`.repeat(30);
const READABLE_SCRIPT = Array.from(
    { length: 60 },
    (_, line) => `    let value${line} = compute(${line});`,
).join("\n");

function asset(contentType: string, body: string, status = 200): ResponseSnapshot {
    return { status, headers: { "content-type": contentType }, body };
}

describe("every asset the page references", () => {
    test("a 404 fails", () => {
        let problems = evaluateAsset(
            "/assets/app/entry.ts",
            asset("text/plain", "Not Found", 404),
            "development",
        );

        expect(problems).toContain("expected status 200, received 404");
    });

    test("a script served as HTML fails", () => {
        let problems = evaluateAsset(
            "/assets/app/entry.ts",
            asset("text/html; charset=utf-8", READABLE_SCRIPT),
            "development",
        );

        expect(problems).toEqual([
            "expected a JavaScript content type, received text/html; charset=utf-8",
        ]);
    });

    test("a stylesheet served as JavaScript fails", () => {
        let problems = evaluateAsset(
            "/assets/app/preflight.css",
            asset("application/javascript; charset=utf-8", "body{}"),
            "development",
        );

        expect(problems).toEqual([
            "expected a text/css content type, received application/javascript; charset=utf-8",
        ]);
    });

    test("TypeScript and TSX module URLs are scripts that the server compiled to JavaScript", () => {
        for (let url of ["/assets/app/entry.ts", "/assets/app/field.tsx", "/assets/run.js"]) {
            let problems = evaluateAsset(
                url,
                asset("application/javascript; charset=utf-8", READABLE_SCRIPT),
                "development",
            );

            expect(problems).toEqual([]);
        }
    });

    test("text/javascript is also accepted", () => {
        expect(
            evaluateAsset(
                "/assets/run.js",
                asset("text/javascript", READABLE_SCRIPT),
                "development",
            ),
        ).toEqual([]);
    });
});

describe("development mode", () => {
    test("readable, unfingerprinted assets pass", () => {
        let problems = evaluateAsset(
            "/assets/app/actions/public/entry.ts",
            asset("application/javascript; charset=utf-8", READABLE_SCRIPT),
            "development",
        );

        expect(problems).toEqual([]);
    });
});

describe("production mode", () => {
    test("fingerprinted, minified scripts and fingerprinted stylesheets pass", () => {
        expect(
            evaluateAsset(
                "/assets/app/actions/public/entry.@rl6cLT.ts",
                asset("application/javascript; charset=utf-8", MINIFIED_SCRIPT),
                "production",
            ),
        ).toEqual([]);
        expect(
            evaluateAsset(
                "/assets/app/actions/public/preflight.@DeqRbq.css",
                asset("text/css; charset=utf-8", "body{margin:0}"),
                "production",
            ),
        ).toEqual([]);
    });

    test("an unfingerprinted URL fails", () => {
        let problems = evaluateAsset(
            "/assets/app/actions/public/entry.ts",
            asset("application/javascript; charset=utf-8", MINIFIED_SCRIPT),
            "production",
        );

        expect(problems).toEqual(["production URL is not fingerprinted"]);
    });

    test("an unminified script fails", () => {
        let problems = evaluateAsset(
            "/assets/app/actions/public/entry.@rl6cLT.ts",
            asset("application/javascript; charset=utf-8", READABLE_SCRIPT),
            "production",
        );

        expect(problems).toHaveLength(1);
        expect(problems[0]).toContain("script is not minified");
    });

    test("a tiny script is minified by construction", () => {
        let problems = evaluateAsset(
            "/assets/pkg/self.@893yhP.js",
            asset("application/javascript; charset=utf-8", "export default globalThis;"),
            "production",
        );

        expect(problems).toEqual([]);
    });
});
