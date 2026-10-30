import { describe, expect, test } from "vitest";

import { parseOptions } from "./options.ts";

describe("command-line options", () => {
    test("default to every runtime and every workload with real-run trial counts", () => {
        let parsed = parseOptions([]);

        expect(parsed).toMatchObject({
            runtimes: ["node", "bun", "bento"],
            workloads: [
                "cold-launch",
                "assets",
                "asset-operations",
                "steady-state",
                "watch-reload",
                "memory",
            ],
        });
        expect(parsed.trials["cold-launch"]).toBeGreaterThanOrEqual(10);
    });

    test("select runtimes and workloads by name, in the given order", () => {
        let parsed = parseOptions(["--runtimes", "bento,node", "--workloads", "memory,assets"]);

        expect(parsed.runtimes).toEqual(["bento", "node"]);
        expect(parsed.workloads).toEqual(["memory", "assets"]);
    });

    test("--trials sets every workload's trial count", () => {
        let parsed = parseOptions(["--trials", "2"]);

        expect(Object.values(parsed.trials).every(count => count === 2)).toBe(true);
    });

    test("steady-state load and output location are configurable", () => {
        let parsed = parseOptions([
            "--duration",
            "2",
            "--warmup",
            "1",
            "--concurrency",
            "4",
            "--iterations",
            "3",
            "--output",
            "/tmp/bento-bench/results",
        ]);

        expect(parsed).toMatchObject({
            steadyState: { durationSeconds: 2, warmupSeconds: 1, concurrency: 4 },
            iterations: 3,
            outputDirectory: "/tmp/bento-bench/results",
        });
    });

    test("an unknown runtime is rejected with the accepted names", () => {
        expect(() => parseOptions(["--runtimes", "node,deno"])).toThrow(
            'Unknown runtime "deno"; expected node, bun, or bento',
        );
    });

    test("an unknown workload is rejected with the accepted names", () => {
        expect(() => parseOptions(["--workloads", "latency"])).toThrow(
            /Unknown workload "latency"; expected one of cold-launch, assets/,
        );
    });

    test("a non-positive trial count is rejected", () => {
        expect(() => parseOptions(["--trials", "0"])).toThrow(
            "--trials must be a positive integer",
        );
        expect(() => parseOptions(["--trials", "many"])).toThrow(
            "--trials must be a positive integer",
        );
    });
});
