import { spawnSync } from "node:child_process";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { afterAll, beforeAll, describe, expect, test } from "vitest";

import type { ProbeOperation } from "../test-support/probe-fixture.ts";

import { baseEnvironment } from "../launch-commands.ts";
import { CLEAN_SOURCES, createProbeFixture } from "../test-support/probe-fixture.ts";

let parent: string;

beforeAll(async () => {
    parent = await mkdtemp(path.join(tmpdir(), "bento-bench-probe-"));
});

afterAll(async () => {
    await rm(parent, { recursive: true, force: true });
});

interface ProbeRun {
    status: number | null;
    stdout: string;
    stderr: string;
}

async function runProbe(options: {
    sources?: Record<string, string>;
    standIns?: Partial<Record<ProbeOperation, string>>;
}): Promise<ProbeRun> {
    let directory = await createProbeFixture(parent, {
        sources: options.sources ?? CLEAN_SOURCES,
        standIns: options.standIns,
    });
    let { status, stdout, stderr } = spawnSync(process.execPath, [".bench/probe.ts", "2"], {
        cwd: directory,
        env: baseEnvironment(),
        encoding: "utf8",
    });
    return { status, stdout, stderr: stderr.trim() };
}

/** The probe stopped, named the failure on stderr, and printed no timing report. */
function expectFailure(run: ProbeRun, message: string) {
    expect(run).toEqual({ status: 1, stdout: "", stderr: message });
}

describe("when every operation succeeds", () => {
    test("each pass times all five operations and digests their outputs", async () => {
        let run = await runProbe({});

        expect(run.status).toBe(0);
        expect(run.stderr).toBe("");
        let report = JSON.parse(run.stdout);
        expect(report).toMatchObject({ scripts: 3, styles: 1 });
        expect(report.passes).toHaveLength(2);
        for (let pass of report.passes) {
            for (let operation of ["parse", "resolve", "transform", "minify", "css"]) {
                expect(pass[operation]).toEqual({
                    ms: expect.any(Number),
                    digest: expect.stringMatching(/^[0-9a-f]{16}$/),
                });
            }
        }
    });

    test("a runtime-provided module the file resolver cannot find is its recorded outcome", async () => {
        let run = await runProbe({
            sources: {
                ...CLEAN_SOURCES,
                "app/server.ts":
                    'import path from "node:path";\nimport store from "host:guestbook";\nexport default [path, store];\n',
            },
        });

        expect(run.status).toBe(0);
        expect(JSON.parse(run.stdout).passes).toHaveLength(2);
    });

    test("a Lightning CSS warning is not a failure", async () => {
        let run = await runProbe({
            standIns: {
                css: `export function transform({ code }) {
                    return { code, warnings: [{ message: "Unknown at rule: @tailwind" }] };
                }`,
            },
        });

        expect(run.status).toBe(0);
        expect(JSON.parse(run.stdout).passes).toHaveLength(2);
    });
});

describe("an operation's reported error stops the probe without a timing", () => {
    test("a parse diagnostic from the real oxc-parser", async () => {
        let run = await runProbe({
            sources: {
                ...CLEAN_SOURCES,
                "app/actions/public/entry.ts": "export const broken = <;",
            },
        });

        expectFailure(run, "parse failed on app/actions/public/entry.ts: Unexpected token");
    });

    test("a transform errors entry", async () => {
        let run = await runProbe({
            standIns: {
                transform: `export async function transform(file, source) {
                    if (!file.endsWith("view.tsx")) return { code: source, map: {}, errors: [] };
                    return { code: "", errors: [
                        { severity: "Error", message: "JSX syntax is disabled" },
                        { severity: "Error", message: "a later diagnostic" },
                    ] };
                }`,
            },
        });

        expectFailure(run, "transform failed on app/view.tsx: JSX syntax is disabled");
    });

    test("a minify errors entry", async () => {
        let run = await runProbe({
            standIns: {
                minify: `export async function minify() {
                    return { code: "", errors: [{ severity: "Error", message: "Unexpected token" }] };
                }`,
            },
        });

        expectFailure(run, "minify failed on app/entry.ts: Unexpected token");
    });

    test("a resolver error for an import that names a file", async () => {
        let run = await runProbe({
            sources: { ...CLEAN_SOURCES, "app/entry.ts": 'import "./missing.ts";\n' },
        });

        expectFailure(
            run,
            `resolve failed on "./missing.ts" from app/entry.ts: Cannot find module './missing.ts'`,
        );
    });

    test("a resolver error for a remote import", async () => {
        let run = await runProbe({
            sources: {
                ...CLEAN_SOURCES,
                "app/entry.ts": 'import "https://example.invalid/no-module.js";\n',
            },
        });

        expectFailure(
            run,
            `resolve failed on "https://example.invalid/no-module.js" from app/entry.ts: Cannot find module 'https://example.invalid/no-module.js'`,
        );
    });
});

const THROWING_STAND_INS: Record<ProbeOperation, string> = {
    parse: 'export function parseSync() { throw new Error("the binding crashed"); }',
    resolve: `export class ResolverFactory {
        async resolveFileAsync() { throw new Error("the binding crashed"); }
    }`,
    transform: 'export async function transform() { throw new Error("the binding crashed"); }',
    minify: 'export async function minify() { throw new Error("the binding crashed"); }',
    css: 'export function transform() { throw new Error("the binding crashed"); }',
};

const ABSENT_STAND_INS: Record<ProbeOperation, string> = {
    parse: "export function parseSync() { return undefined; }",
    resolve: "export class ResolverFactory { async resolveFileAsync() { return undefined; } }",
    transform: "export async function transform() { return undefined; }",
    minify: "export async function minify() { return undefined; }",
    css: "export function transform() { return undefined; }",
};

const FIRST_SUBJECT: Record<ProbeOperation, string> = {
    parse: "app/entry.ts",
    resolve: '"./greeting.ts" from app/entry.ts',
    transform: "app/entry.ts",
    minify: "app/entry.ts",
    css: "app/styles.css",
};

const OPERATIONS = Object.keys(FIRST_SUBJECT) as ProbeOperation[];

describe.each(OPERATIONS)("%s", operation => {
    test("a thrown exception or rejection stops the probe, naming its message", async () => {
        let run = await runProbe({ standIns: { [operation]: THROWING_STAND_INS[operation] } });

        expectFailure(
            run,
            `${operation} failed on ${FIRST_SUBJECT[operation]}: the binding crashed`,
        );
    });

    test("an absent result stops the probe", async () => {
        let run = await runProbe({ standIns: { [operation]: ABSENT_STAND_INS[operation] } });

        expectFailure(
            run,
            `${operation} failed on ${FIRST_SUBJECT[operation]}: returned no result`,
        );
    });
});
