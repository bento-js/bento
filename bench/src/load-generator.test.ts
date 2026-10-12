import { expect, test } from "vite-plus/test";

import { parseLoadResult } from "./load-generator.ts";

// Excerpt of `oha 1.16.0 --output-format json` for a one-second GET / run against the fixture.
const RECORDED_REPORT = {
    summary: { successRate: 1, requestsPerSec: 114.60355183601202, sizePerRequest: 4403 },
    latencyPercentiles: {
        p50: 0.032369375,
        p90: 0.035856166,
        p95: 0.037944333,
        p99: 0.081673208,
        "p99.9": 0.08417425,
    },
    statusCodeDistribution: { "200": 118 },
    errorDistribution: {},
};

test("oha's JSON becomes throughput, latency in milliseconds, and status counts", () => {
    let result = parseLoadResult(RECORDED_REPORT);

    expect(result.requests).toBe(118);
    expect(result.requestsPerSecond).toBeCloseTo(114.6036, 4);
    expect(result.successRate).toBe(1);
    expect(result.bytesPerRequest).toBe(4403);
    expect(result.statusCodes).toEqual({ "200": 118 });
    expect(result.errors).toEqual({});
    expect(result.latencyMs.p50).toBeCloseTo(32.369, 3);
    expect(result.latencyMs.p99).toBeCloseTo(81.673, 3);
});

test("failed requests stay visible as status and error counts", () => {
    let result = parseLoadResult({
        ...RECORDED_REPORT,
        statusCodeDistribution: { "200": 90, "500": 10 },
        errorDistribution: { "connection closed before message completed": 3 },
    });

    expect(result.requests).toBe(100);
    expect(result.statusCodes).toEqual({ "200": 90, "500": 10 });
    expect(result.errors).toEqual({ "connection closed before message completed": 3 });
});

test("a report without the numbers the harness needs is rejected", () => {
    expect(() => parseLoadResult({ summary: {} })).toThrow(
        "Expected a number at summary.requestsPerSec",
    );
});
