export interface Summary {
    count: number;
    min: number;
    max: number;
    median: number;
    q1: number;
    q3: number;
    iqr: number;
    percentiles: Partial<Record<"p90" | "p95" | "p99", number>>;
}

// A percentile is only reported when at least 1 / (1 - p) samples exist, so that it is not
// simply the maximum in disguise.
const PERCENTILES: ReadonlyArray<readonly [key: "p90" | "p95" | "p99", fraction: number]> = [
    ["p90", 0.9],
    ["p95", 0.95],
    ["p99", 0.99],
];

/** Linear interpolation between closest ranks (the "type 7" definition). */
export function percentile(sortedAscending: readonly number[], fraction: number): number {
    let rank = (sortedAscending.length - 1) * fraction;
    let lowerIndex = Math.floor(rank);
    let upperIndex = Math.ceil(rank);
    let lower = sortedAscending[lowerIndex]!;
    let upper = sortedAscending[upperIndex]!;
    return lower + (upper - lower) * (rank - lowerIndex);
}

export function median(values: readonly number[]): number {
    return percentile(sortAscending(values), 0.5);
}

export function summarize(values: readonly number[]): Summary | undefined {
    if (values.length === 0) return undefined;
    let sorted = sortAscending(values);
    let q1 = percentile(sorted, 0.25);
    let q3 = percentile(sorted, 0.75);
    let percentiles: Summary["percentiles"] = {};
    for (let [key, fraction] of PERCENTILES) {
        if (sorted.length * (1 - fraction) >= 1 - 1e-9) {
            percentiles[key] = percentile(sorted, fraction);
        }
    }
    return {
        count: sorted.length,
        min: sorted[0]!,
        max: sorted.at(-1)!,
        median: percentile(sorted, 0.5),
        q1,
        q3,
        iqr: q3 - q1,
        percentiles,
    };
}

function sortAscending(values: readonly number[]): number[] {
    return [...values].sort((left, right) => left - right);
}
