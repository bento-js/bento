import { describe, expect, test } from "vitest";

import type { RunRecord, WorkloadResult } from "./results-model.ts";

import { renderSummary } from "./summary.ts";

const ENVIRONMENT: RunRecord["environment"] = {
    hardware: "Mac16,1, Apple M4, 10 cores, 32 GiB",
    os: "macOS 27.0.1 (arm64)",
    runtimes: { node: "v26.11.1", bun: "1.4.3", bento: "unavailable" },
    tools: { oha: "oha 1.16.0", pnpm: "12.11.2" },
    revisions: { bento: "abc1234", fixture: "def5678" },
    remixVersion: "3.0.0",
};

function coldLaunch(): WorkloadResult {
    return {
        workload: "cold-launch",
        title: "Cold launch",
        question: "How long from process spawn to listening and to the first correct page?",
        metrics: [
            { key: "listeningMs", label: "Listening", unit: "ms" },
            { key: "firstPageMs", label: "First page", unit: "ms" },
        ],
        notes: ["Bun fails gate 1; see issue #2."],
        series: [
            {
                runtime: "node",
                variant: "production",
                outcome: {
                    status: "measured",
                    failures: [{ trial: 2, message: "port never accepted connections" }],
                    samples: [
                        { trial: 0, listeningMs: 10, firstPageMs: 40 },
                        { trial: 1, listeningMs: 12.5, firstPageMs: 44 },
                        { trial: 3, listeningMs: 15, firstPageMs: 48 },
                    ],
                },
            },
            {
                runtime: "bun",
                variant: "production",
                outcome: {
                    status: "incompatible",
                    check: "document",
                    failures: ["response body is empty"],
                    evidence: [
                        {
                            method: "GET",
                            url: "http://127.0.0.1:17701/",
                            status: 200,
                            contentType: "text/html",
                            location: null,
                            bodyBytes: 0,
                            bodyExcerpt: "",
                        },
                    ],
                },
            },
            {
                runtime: "bento",
                variant: "production",
                outcome: { status: "unavailable", reason: "BENTO_BIN is not set" },
            },
        ],
    };
}

function render(workloads: WorkloadResult[]): string {
    return renderSummary({
        startedAt: "2026-10-10T23:00:00.000Z",
        options: {},
        environment: ENVIRONMENT,
        workloads,
    });
}

describe("summary.md", () => {
    test("records the environment so a result can be reproduced", () => {
        let summary = render([coldLaunch()]);

        expect(summary).toContain("Mac16,1, Apple M4, 10 cores, 32 GiB");
        expect(summary).toContain("macOS 27.0.1 (arm64)");
        expect(summary).toContain("v26.11.1");
        expect(summary).toContain("oha 1.16.0");
        expect(summary).toContain("abc1234");
        expect(summary).toContain("Remix 3.0.0");
    });

    test("shows median, range, IQR and sample count for a measured runtime", () => {
        let summary = render([coldLaunch()]);

        expect(summary).toContain(
            "| Listening (ms) | 12.5 (10.0–15.0, IQR 2.5, n=3) | incompatible | unavailable |",
        );
        expect(summary).toContain(
            "| First page (ms) | 44.0 (40.0–48.0, IQR 4.0, n=3) | incompatible | unavailable |",
        );
    });

    test("an incompatible runtime gets its failing check and evidence, never a timing", () => {
        let summary = render([coldLaunch()]);

        expect(summary).toContain("**bun** incompatible at the document check");
        expect(summary).toContain("response body is empty");
        expect(summary).toContain("GET http://127.0.0.1:17701/ → 200, 0 bytes, text/html");
    });

    test("response evidence includes a redirect's Location and omits it otherwise", () => {
        let withoutRedirect = render([coldLaunch()]);
        let workload = coldLaunch();
        workload.series = [
            {
                runtime: "bun",
                variant: "production",
                outcome: {
                    status: "incompatible",
                    check: "valid-submission",
                    failures: ["Location should be /"],
                    evidence: [
                        {
                            method: "POST",
                            url: "http://127.0.0.1:17701/",
                            status: 303,
                            contentType: null,
                            location: "/elsewhere",
                            bodyBytes: 0,
                            bodyExcerpt: "",
                        },
                    ],
                },
            },
        ];

        expect(withoutRedirect).not.toContain("Location");
        expect(render([workload])).toContain(
            "POST http://127.0.0.1:17701/ → 303, 0 bytes, no content type, Location: /elsewhere",
        );
    });

    test("an unavailable runtime is named with the reason", () => {
        expect(render([coldLaunch()])).toContain("**bento** unavailable: BENTO_BIN is not set");
    });

    test("trial failures and notes are listed", () => {
        let summary = render([coldLaunch()]);

        expect(summary).toContain("**node** trial 2 failed: port never accepted connections");
        expect(summary).toContain("Bun fails gate 1; see issue #2.");
    });

    test("a series with no successful trial is reported failed with every reason", () => {
        let workload = coldLaunch();
        workload.series = [
            {
                runtime: "node",
                variant: "production",
                outcome: {
                    status: "failed",
                    failures: [
                        { trial: 0, message: "exited with code 1" },
                        { trial: 1, message: "exited with code 1" },
                    ],
                },
            },
        ];

        let summary = render([workload]);

        expect(summary).toContain("| Listening (ms) | failed |");
        expect(summary).toContain("**node** trial 0 failed: exited with code 1");
        expect(summary).toContain("**node** trial 1 failed: exited with code 1");
    });

    test("percentiles appear in the cell once the trial count supports them", () => {
        let workload = coldLaunch();
        workload.series = [
            {
                runtime: "node",
                variant: "production",
                outcome: {
                    status: "measured",
                    failures: [],
                    samples: Array.from({ length: 20 }, (_, trial) => ({
                        trial,
                        listeningMs: trial + 1,
                        firstPageMs: 50,
                    })),
                },
            },
        ];

        let summary = render([workload]);

        expect(summary).toContain("10.5 (1.0–20.0, IQR 9.5, p90 18.1, p95 19.1, n=20)");
    });
});
