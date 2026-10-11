import type { ResponseSnapshot } from "./document-gate.ts";

import { decodeEntities, findTags, textAfter } from "./html-tags.ts";

export interface FieldExpectation {
    field: "message" | "name";
    /** The raw text that was submitted. */
    value: string;
    /** The field error the page must show, or undefined when the field was valid. */
    error: string | undefined;
}

/** Problems with the response to a valid submission: 303 back to `/`. */
export function evaluateRedirect(response: ResponseSnapshot): string[] {
    let problems: string[] = [];
    if (response.status !== 303) problems.push(`expected status 303, received ${response.status}`);
    let location = response.headers.location;
    if (location !== "/") {
        problems.push(`expected Location: /, received ${location ?? "no Location header"}`);
    }
    return problems;
}

/** Problems with the response to an invalid submission: 400 with errors and preserved values. */
export function evaluateRejection(
    response: ResponseSnapshot,
    fields: readonly FieldExpectation[],
): string[] {
    let problems: string[] = [];
    if (response.status !== 400) problems.push(`expected status 400, received ${response.status}`);
    for (let expectation of fields) problems.push(...fieldProblems(response.body, expectation));
    return problems;
}

function fieldProblems(html: string, { field, value, error }: FieldExpectation): string[] {
    let problems: string[] = [];
    let shownError = errorTextOf(html, field);
    if (error === undefined && shownError !== undefined) {
        problems.push(`${field} shows an error although it was valid`);
    }
    if (error !== undefined && shownError !== error) {
        problems.push(`${field} error "${error}" is not shown`);
    }
    let renderedValue = submittedValueOf(html, field);
    if (renderedValue !== value) {
        problems.push(
            `${field} value was not preserved (expected "${value}", rendered "${renderedValue ?? ""}")`,
        );
    }
    return problems;
}

function errorTextOf(html: string, field: string): string | undefined {
    let paragraph = findTags(html, "p").find(
        tag => tag.attributes.id === `guest-book-${field}-error`,
    );
    return paragraph && decodeEntities(textAfter(html, paragraph, "p") ?? "");
}

function submittedValueOf(html: string, field: "message" | "name"): string | undefined {
    if (field === "name") {
        return findTags(html, "input").find(tag => tag.attributes.name === "name")?.attributes
            .value;
    }
    let textarea = findTags(html, "textarea").find(tag => tag.attributes.name === "message");
    return textarea && decodeEntities(textAfter(html, textarea, "textarea") ?? "");
}
