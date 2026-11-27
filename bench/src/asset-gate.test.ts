import { describe, expect, test } from "vitest";

import type { ResponseSnapshot } from "./document-gate.ts";

import { evaluateAsset } from "./asset-gate.ts";

const MINIFIED_SCRIPT = Array.from(
    { length: 30 },
    (_, index) => `export function render${index}(a){return a.map(b=>b.name).join(", ")}`,
).join("");
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

    test("an empty body fails for a script and for a stylesheet", () => {
        for (let [url, contentType] of [
            ["/assets/app/entry.ts", "application/javascript; charset=utf-8"],
            ["/assets/app/preflight.css", "text/css; charset=utf-8"],
        ] as const) {
            expect(evaluateAsset(url, asset(contentType, ""), "development")).toEqual([
                "response body is empty",
            ]);
        }
    });

    test("a whitespace-only body fails for a script and for a stylesheet", () => {
        for (let [url, contentType] of [
            ["/assets/app/entry.ts", "application/javascript; charset=utf-8"],
            ["/assets/app/preflight.css", "text/css; charset=utf-8"],
        ] as const) {
            expect(evaluateAsset(url, asset(contentType, " \n\t\n"), "development")).toEqual([
                "response body contains only whitespace",
            ]);
        }
    });

    test("a script that is not a JavaScript module fails", () => {
        for (let body of ["<!DOCTYPE html><html></html>", "let count: number = 1;"]) {
            let problems = evaluateAsset(
                "/assets/app/entry.ts",
                asset("application/javascript; charset=utf-8", body),
                "development",
            );

            expect(problems).toHaveLength(1);
            expect(problems[0]).toMatch(/^script does not parse as a JavaScript module: /);
        }
    });

    test("a script with no statements fails", () => {
        for (let body of [
            "// compiled by nothing",
            "/* */\n;;\n//# sourceMappingURL=entry.js.map",
        ]) {
            expect(
                evaluateAsset(
                    "/assets/app/entry.ts",
                    asset("application/javascript; charset=utf-8", body),
                    "development",
                ),
            ).toEqual(["script contains no code"]);
        }
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

    test("a tiny script still needs content", () => {
        let url = "/assets/pkg/self.@893yhP.js";
        let contentType = "application/javascript; charset=utf-8";

        expect(evaluateAsset(url, asset(contentType, ""), "production")).toEqual([
            "response body is empty",
        ]);
        expect(evaluateAsset(url, asset(contentType, "\n\n"), "production")).toEqual([
            "response body contains only whitespace",
        ]);
        expect(evaluateAsset(url, asset(contentType, "//"), "production")).toEqual([
            "script contains no code",
        ]);
    });

    test("an empty production stylesheet fails", () => {
        expect(
            evaluateAsset(
                "/assets/app/actions/public/preflight.@DeqRbq.css",
                asset("text/css; charset=utf-8", ""),
                "production",
            ),
        ).toEqual(["response body is empty"]);
    });
});
