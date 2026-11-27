import { describe, expect, test } from "vitest";

import type { ResponseSnapshot } from "./document-gate.ts";

import { readSample } from "./samples/index.ts";
import { evaluateRedirect, evaluateRejection } from "./submission-gate.ts";

const NAME_REQUIRED = "Enter your name.";
const MESSAGE_REQUIRED = "Write a message to sign the guest book.";
const MESSAGE_TOO_LONG = "Keep your message to 280 characters or fewer.";

function respond(
    status: number,
    body: string,
    headers: Record<string, string> = {},
): ResponseSnapshot {
    return { status, headers, body };
}

describe("a valid submission", () => {
    test("303 with Location: / passes", () => {
        expect(evaluateRedirect(respond(303, "", { location: "/" }))).toEqual([]);
    });

    test("any other status fails", () => {
        expect(evaluateRedirect(respond(200, "<html></html>", {}))).toContain(
            "expected status 303, received 200",
        );
        expect(evaluateRedirect(respond(302, "", { location: "/" }))).toContain(
            "expected status 303, received 302",
        );
    });

    test("a redirect elsewhere fails", () => {
        expect(evaluateRedirect(respond(303, "", { location: "/elsewhere" }))).toEqual([
            "expected Location: /, received /elsewhere",
        ]);
        expect(evaluateRedirect(respond(303, "", {}))).toEqual([
            "expected Location: /, received no Location header",
        ]);
    });
});

describe("an invalid submission", () => {
    test("whitespace-only fields: 400 with both field errors and the submitted values", () => {
        let problems = evaluateRejection(respond(400, readSample("validation-errors")), [
            { field: "name", value: "   ", error: NAME_REQUIRED },
            { field: "message", value: "   ", error: MESSAGE_REQUIRED },
        ]);

        expect(problems).toEqual([]);
    });

    test("a 281-character message: 400 with the length error and preserved values", () => {
        let problems = evaluateRejection(respond(400, readSample("message-too-long")), [
            { field: "name", value: "Too Wordy", error: undefined },
            { field: "message", value: "x".repeat(281), error: MESSAGE_TOO_LONG },
        ]);

        expect(problems).toEqual([]);
    });

    test("status 200 fails: the invalid entry must be refused", () => {
        let problems = evaluateRejection(respond(200, readSample("validation-errors")), [
            { field: "name", value: "   ", error: NAME_REQUIRED },
        ]);

        expect(problems).toEqual(["expected status 400, received 200"]);
    });

    test("a redirect fails even though no error text is rendered", () => {
        let problems = evaluateRejection(respond(303, "", { location: "/" }), [
            { field: "name", value: "   ", error: NAME_REQUIRED },
        ]);

        expect(problems).toContain("expected status 400, received 303");
    });

    test("a missing field error fails", () => {
        let page = readSample("validation-errors").replace(NAME_REQUIRED, "");

        let problems = evaluateRejection(respond(400, page), [
            { field: "name", value: "   ", error: NAME_REQUIRED },
        ]);

        expect(problems).toEqual([`name error "${NAME_REQUIRED}" is not shown`]);
    });

    test("a field error shown for a valid field fails", () => {
        let problems = evaluateRejection(respond(400, readSample("validation-errors")), [
            { field: "name", value: "   ", error: undefined },
        ]);

        expect(problems).toEqual(["name shows an error although it was valid"]);
    });

    test("a cleared name input fails", () => {
        let page = readSample("validation-errors").replace('value="   "', 'value=""');

        let problems = evaluateRejection(respond(400, page), [
            { field: "name", value: "   ", error: NAME_REQUIRED },
        ]);

        expect(problems).toEqual(['name value was not preserved (expected "   ", rendered "")']);
    });

    test("a cleared message textarea fails", () => {
        let page = readSample("message-too-long").replace(/>x{281}<\/textarea>/, "></textarea>");

        let problems = evaluateRejection(respond(400, page), [
            { field: "message", value: "x".repeat(281), error: MESSAGE_TOO_LONG },
        ]);

        expect(problems).toHaveLength(1);
        expect(problems[0]).toContain("message value was not preserved");
    });
});
