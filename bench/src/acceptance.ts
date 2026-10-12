import type { ExpectedEntry } from "./document-gate.ts";
import type { HttpEvidence, Mode } from "./results-model.ts";

import { evaluateAsset } from "./asset-gate.ts";
import { assetUrlsOf, evaluateDocument } from "./document-gate.ts";
import { exchange, RequestFailure } from "./http-exchange.ts";
import { ACCEPTANCE_ENTRY } from "./seed-entries.ts";
import { evaluateRedirect, evaluateRejection } from "./submission-gate.ts";

export type CheckName = "assets" | "document" | "invalid-submissions" | "valid-submission";

export interface CheckResult {
    name: CheckName;
    passed: boolean;
    failures: string[];
    evidence: HttpEvidence[];
}

export interface AcceptanceReport {
    passed: boolean;
    checks: CheckResult[];
}

export interface AcceptanceOptions {
    baseUrl: string;
    mode: Mode;
    /** The entries the database holds before the checker signs anything, in rendered order. */
    seededEntries: readonly ExpectedEntry[];
}

const NAME_REQUIRED = "Enter your name.";
const MESSAGE_REQUIRED = "Write a message to sign the guest book.";
const MESSAGE_TOO_LONG = "Keep your message to 280 characters or fewer.";

export function failingCheck(report: AcceptanceReport): CheckResult | undefined {
    return report.checks.find(check => !check.passed);
}

/** Runs the four gates in order and stops at the first one that fails. */
export async function runAcceptance(options: AcceptanceOptions): Promise<AcceptanceReport> {
    let checks: CheckResult[] = [];
    let entries = [...options.seededEntries];
    let documentBody = "";

    let steps: Array<[CheckName, (evidence: HttpEvidence[]) => Promise<string[]>]> = [
        [
            "document",
            async evidence => {
                let { snapshot, evidence: seen } = await exchange(options.baseUrl, { path: "/" });
                evidence.push(seen);
                documentBody = snapshot.body;
                return evaluateDocument(snapshot, entries);
            },
        ],
        ["assets", evidence => checkAssets(options, documentBody, evidence)],
        [
            "valid-submission",
            async evidence => {
                let failures = await checkValidSubmission(options.baseUrl, entries, evidence);
                if (failures.length === 0) entries.push(ACCEPTANCE_ENTRY);
                return failures;
            },
        ],
        [
            "invalid-submissions",
            evidence => checkInvalidSubmissions(options.baseUrl, entries, evidence),
        ],
    ];

    for (let [name, execute] of steps) {
        let result = await runCheck(name, execute);
        checks.push(result);
        if (!result.passed) break;
    }
    return { passed: checks.every(check => check.passed), checks };
}

async function runCheck(
    name: CheckName,
    execute: (evidence: HttpEvidence[]) => Promise<string[]>,
): Promise<CheckResult> {
    let evidence: HttpEvidence[] = [];
    try {
        let failures = await execute(evidence);
        return { name, passed: failures.length === 0, failures, evidence };
    } catch (error) {
        if (!(error instanceof RequestFailure)) throw error;
        return { name, passed: false, failures: [error.message], evidence };
    }
}

async function checkAssets(
    options: AcceptanceOptions,
    documentBody: string,
    evidence: HttpEvidence[],
): Promise<string[]> {
    let failures: string[] = [];
    let urls = assetUrlsOf(documentBody);
    if (urls.length === 0) return ["the page references no assets"];
    for (let url of urls) {
        let { snapshot, evidence: seen } = await exchange(options.baseUrl, { path: url });
        let problems = evaluateAsset(url, snapshot, options.mode);
        if (problems.length > 0) evidence.push(seen);
        failures.push(...problems.map(problem => `${url}: ${problem}`));
    }
    return failures;
}

async function checkValidSubmission(
    baseUrl: string,
    entries: readonly ExpectedEntry[],
    evidence: HttpEvidence[],
): Promise<string[]> {
    let form = { name: ACCEPTANCE_ENTRY.name, message: ACCEPTANCE_ENTRY.message };
    let submitted = await exchange(baseUrl, { path: "/", method: "POST", form });
    evidence.push(submitted.evidence);
    let failures = evaluateRedirect(submitted.snapshot);

    let page = await exchange(baseUrl, { path: "/" });
    evidence.push(page.evidence);
    let rendered = evaluateDocument(page.snapshot, [...entries, ACCEPTANCE_ENTRY]);
    return [...failures, ...rendered.map(problem => `after the submission: ${problem}`)];
}

async function checkInvalidSubmissions(
    baseUrl: string,
    entries: readonly ExpectedEntry[],
    evidence: HttpEvidence[],
): Promise<string[]> {
    let blank = "   ";
    let tooLong = "x".repeat(281);
    let cases = [
        {
            label: "whitespace-only fields",
            form: { name: blank, message: blank },
            fields: [
                { field: "name", value: blank, error: NAME_REQUIRED },
                { field: "message", value: blank, error: MESSAGE_REQUIRED },
            ],
        },
        {
            label: "a 281-character message",
            form: { name: "Too Wordy", message: tooLong },
            fields: [
                { field: "name", value: "Too Wordy", error: undefined },
                { field: "message", value: tooLong, error: MESSAGE_TOO_LONG },
            ],
        },
    ] as const;

    let failures: string[] = [];
    for (let { label, form, fields } of cases) {
        let rejected = await exchange(baseUrl, { path: "/", method: "POST", form });
        evidence.push(rejected.evidence);
        let problems = evaluateRejection(rejected.snapshot, fields);
        failures.push(...problems.map(problem => `${label}: ${problem}`));
    }

    let page = await exchange(baseUrl, { path: "/" });
    evidence.push(page.evidence);
    let unchanged = evaluateDocument(page.snapshot, entries);
    return [...failures, ...unchanged.map(problem => `after the rejections: ${problem}`)];
}
