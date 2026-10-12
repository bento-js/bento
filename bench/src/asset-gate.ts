import { parseSync } from "oxc-parser";

import type { ResponseSnapshot } from "./document-gate.ts";
import type { Mode } from "./results-model.ts";

const FINGERPRINTED_PATH = /\.@[\w-]+\.[a-z]+$/;
const JAVASCRIPT_CONTENT_TYPE = /^(?:text|application)\/javascript\b/;
const STYLESHEET_CONTENT_TYPE = /^text\/css\b/;
const MINIFICATION_MIN_BYTES = 500;
const MINIFIED_MIN_AVERAGE_LINE = 100;

/** Problems with one referenced asset: status, content type, content, and production output. */
export function evaluateAsset(url: string, response: ResponseSnapshot, mode: Mode): string[] {
    let problems: string[] = [];
    let pathname = url.split("?")[0]!;
    let isStylesheet = pathname.endsWith(".css");
    let contentType = response.headers["content-type"];

    if (response.status !== 200) problems.push(`expected status 200, received ${response.status}`);
    if (isStylesheet && !STYLESHEET_CONTENT_TYPE.test(contentType ?? "")) {
        problems.push(`expected a text/css content type, received ${contentType ?? "none"}`);
    }
    if (!isStylesheet && !JAVASCRIPT_CONTENT_TYPE.test(contentType ?? "")) {
        problems.push(`expected a JavaScript content type, received ${contentType ?? "none"}`);
    }
    if (mode === "production" && !FINGERPRINTED_PATH.test(pathname)) {
        problems.push("production URL is not fingerprinted");
    }
    if (response.status === 200) problems.push(...bodyProblems(response.body, isStylesheet, mode));
    return problems;
}

function bodyProblems(body: string, isStylesheet: boolean, mode: Mode): string[] {
    if (body.length === 0) return ["response body is empty"];
    if (body.trim().length === 0) return ["response body contains only whitespace"];
    if (isStylesheet) return [];
    let problems = scriptProblems(body);
    if (problems.length > 0 || mode !== "production") return problems;
    return minificationProblems(body);
}

function scriptProblems(script: string): string[] {
    let { program, errors } = parseSync("asset.js", script, { lang: "js", sourceType: "module" });
    let error = errors.find(diagnostic => diagnostic.severity === "Error");
    if (error) return [`script does not parse as a JavaScript module: ${error.message}`];
    if (program.body.every(statement => statement.type === "EmptyStatement")) {
        return ["script contains no code"];
    }
    return [];
}

function minificationProblems(script: string): string[] {
    if (script.length < MINIFICATION_MIN_BYTES) return [];
    let lines = script.split("\n").length;
    let averageLine = script.length / lines;
    if (averageLine >= MINIFIED_MIN_AVERAGE_LINE) return [];
    return [
        `script is not minified (average line length ${averageLine.toFixed(0)} over ${lines} lines)`,
    ];
}
