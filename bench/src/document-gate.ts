import { decodeEntities, findTags, textAfter } from "./html-tags.ts";
import { lookup } from "./json-values.ts";

export interface ResponseSnapshot {
    status: number;
    /** Header names lower-cased. */
    headers: Readonly<Record<string, string>>;
    body: string;
}

export interface ExpectedEntry {
    name: string;
    message: string;
}

const ENTRY_PATTERN = /<li\b[^>]*>\s*<p\b[^>]*>(.*?)<\/p>\s*<p\b[^>]*>(.*?)<\/p>\s*<\/li>/gis;
const CLOSING_HTML_PATTERN = /<\/html>\s*(?:<!--[\s\S]*?-->\s*)*$/i;

/** Problems that keep `body` from being the complete, correctly rendered guest-book page. */
export function evaluateDocument(
    response: ResponseSnapshot,
    expectedEntries: readonly ExpectedEntry[],
): string[] {
    let problems: string[] = [];
    if (response.status !== 200) problems.push(`expected status 200, received ${response.status}`);
    if (response.body.length === 0) return [...problems, "response body is empty"];

    let html = response.body;
    if (!/^\s*<!doctype html>/i.test(html))
        problems.push("document does not start with <!DOCTYPE html>");
    if (!CLOSING_HTML_PATTERN.test(html)) problems.push("document ends before the closing </html>");
    if (!hasGuestBookHeading(html)) {
        problems.push(
            'guest-book heading (<h2 id="guest-book-heading">Guest Book</h2>) is missing',
        );
    }
    problems.push(...entryProblems(html, expectedEntries));
    if (!hasSignForm(html)) {
        problems.push("sign form needs a name input, a message textarea, and a POST action of /");
    }
    if (importMapOf(html) === undefined) {
        problems.push("import map is missing or is not JSON with an imports object");
    }
    if (entryScriptUrl(html) === undefined) problems.push("module script with a src is missing");
    if (
        !findTags(html, "link").some(
            tag => hasToken(tag.attributes.rel, "stylesheet") && tag.attributes.href,
        )
    ) {
        problems.push("stylesheet link is missing");
    }
    return problems;
}

/** Stylesheet, module-script, preload, and import-map URLs the page references, once each. */
export function assetUrlsOf(html: string): string[] {
    let urls = new Set<string>();
    for (let tag of findTags(html, "link")) {
        let relation = tag.attributes.rel;
        let isAsset = hasToken(relation, "stylesheet") || hasToken(relation, "modulepreload");
        if (isAsset && tag.attributes.href) urls.add(tag.attributes.href);
    }
    let script = entryScriptUrl(html);
    if (script !== undefined) urls.add(script);
    for (let url of Object.values(importMapOf(html)?.imports ?? {})) urls.add(url);
    return [...urls];
}

/** The `src` of the first `<script type="module">`: the page's browser entry. */
export function entryScriptUrl(html: string): string | undefined {
    let script = findTags(html, "script").find(
        tag => tag.attributes.type === "module" && tag.attributes.src,
    );
    return script?.attributes.src;
}

export function renderedEntries(html: string): ExpectedEntry[] {
    return [...html.matchAll(ENTRY_PATTERN)].map(match => ({
        name: decodeEntities(match[1]!),
        message: decodeEntities(match[2]!),
    }));
}

function hasToken(attribute: string | undefined, token: string): boolean {
    return attribute?.split(/\s+/).includes(token) ?? false;
}

function hasGuestBookHeading(html: string): boolean {
    return findTags(html, "h2").some(
        tag =>
            tag.attributes.id === "guest-book-heading" &&
            textAfter(html, tag, "h2")?.trim() === "Guest Book",
    );
}

function entryProblems(html: string, expected: readonly ExpectedEntry[]): string[] {
    let rendered = renderedEntries(html);
    if (JSON.stringify(rendered) === JSON.stringify(expected)) return [];
    return [
        `entries differ: expected ${JSON.stringify(expected)} but rendered ${JSON.stringify(rendered)}`,
    ];
}

function hasSignForm(html: string): boolean {
    return findTags(html, "form").some(form => {
        if (form.attributes.action !== "/" || form.attributes.method?.toLowerCase() !== "post") {
            return false;
        }
        let contents = textAfter(html, form, "form") ?? "";
        let hasNameInput = findTags(contents, "input").some(tag => tag.attributes.name === "name");
        let hasMessageField = findTags(contents, "textarea").some(
            tag => tag.attributes.name === "message",
        );
        return hasNameInput && hasMessageField;
    });
}

function importMapOf(html: string): { imports: Record<string, string> } | undefined {
    let script = findTags(html, "script").find(tag => tag.attributes.type === "importmap");
    let source = script && textAfter(html, script, "script");
    if (source === undefined) return undefined;
    let parsed: unknown;
    try {
        parsed = JSON.parse(source);
    } catch {
        return undefined;
    }
    let imports = lookup(parsed, "imports");
    if (typeof imports !== "object" || imports === null) return undefined;
    let urls = Object.entries(imports).filter(
        (entry): entry is [string, string] => typeof entry[1] === "string",
    );
    return { imports: Object.fromEntries(urls) };
}
