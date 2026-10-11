import { expect, test } from "vite-plus/test";

import { median, percentile, summarize } from "./statistics.ts";

function sequence(count: number): number[] {
    return Array.from({ length: count }, (_, index) => index + 1);
}

test("median takes the middle of an odd count and the mean of the middle pair otherwise", () => {
    expect(median([9, 1, 5])).toBe(5);
    expect(median([4, 1, 3, 2])).toBe(2.5);
    expect(median([7])).toBe(7);
});

test("percentile interpolates linearly between ranks", () => {
    expect(percentile([1, 2, 3, 4, 5], 0.5)).toBe(3);
    expect(percentile([1, 2, 3, 4, 5], 0.25)).toBe(2);
    expect(percentile([10, 20], 0.5)).toBe(15);
    expect(percentile([10, 20, 30, 40], 0.9)).toBeCloseTo(37, 10);
    expect(percentile([10, 20, 30], 0)).toBe(10);
    expect(percentile([10, 20, 30], 1)).toBe(30);
});

test("summary reports spread from unsorted samples", () => {
    let summary = summarize([5, 1, 4, 2, 3]);

    expect(summary).toMatchObject({
        count: 5,
        min: 1,
        max: 5,
        mean: 3,
        median: 3,
        q1: 2,
        q3: 4,
        iqr: 2,
    });
});

test("summary of one sample has no spread", () => {
    expect(summarize([42])).toMatchObject({ count: 1, min: 42, max: 42, median: 42, iqr: 0 });
});

test("summary of no samples is absent rather than zero", () => {
    expect(summarize([])).toBeUndefined();
});

test("percentiles appear only once the sample count can support them", () => {
    expect(summarize(sequence(9))?.percentiles).toEqual({});
    expect(Object.keys(summarize(sequence(10))?.percentiles ?? {})).toEqual(["p90"]);
    expect(Object.keys(summarize(sequence(19))?.percentiles ?? {})).toEqual(["p90"]);
    expect(Object.keys(summarize(sequence(20))?.percentiles ?? {})).toEqual(["p90", "p95"]);
    expect(Object.keys(summarize(sequence(99))?.percentiles ?? {})).toEqual(["p90", "p95"]);
    expect(Object.keys(summarize(sequence(100))?.percentiles ?? {})).toEqual(["p90", "p95", "p99"]);
});

test("percentile values come from the sorted samples", () => {
    let percentiles = summarize(sequence(100).reverse())?.percentiles;

    expect(percentiles?.p90).toBeCloseTo(90.1, 10);
    expect(percentiles?.p95).toBeCloseTo(95.05, 10);
    expect(percentiles?.p99).toBeCloseTo(99.01, 10);
});
