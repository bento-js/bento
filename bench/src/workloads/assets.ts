import type { SampleRow } from "../results-model.ts";
import type { Workload } from "../workload.ts";

import {
    STARTUP_TIMEOUT_MS,
    waitForDocument,
    waitUntilListening,
    withServer,
} from "../app-server.ts";
import { assetUrlsOf } from "../document-gate.ts";
import { exchange } from "../http-exchange.ts";
import { SEED_ENTRIES } from "../seed-entries.ts";
import { median } from "../statistics.ts";
import { BOTH_MODES } from "../workload.ts";

interface AssetTimings {
    url: string;
    firstMs: number;
    repeatMs: number[];
}

export const assets: Workload = {
    name: "assets",
    title: "Asset requests",
    question:
        "How long does the first request for each script and stylesheet the page references take, compared with repeat requests in the same process that hit the fixture's in-memory cache? The fixture compiles its entry graph while the server starts, so first requests for those assets can already be warm.",
    metrics: [
        { key: "firstScriptsMs", label: "First request, all scripts (sum)", unit: "ms" },
        { key: "firstStylesheetsMs", label: "First request, all stylesheets (sum)", unit: "ms" },
        { key: "repeatScriptsMs", label: "Repeat request, all scripts (median sum)", unit: "ms" },
        {
            key: "repeatStylesheetsMs",
            label: "Repeat request, all stylesheets (median sum)",
            unit: "ms",
        },
    ],
    variants: BOTH_MODES,
    requiresApplicationGate: true,
    async runTrial({ runtime, executable, fixture, mode, trial, context }) {
        let request = { runtime, executable, fixture, mode, watch: false };
        return withServer(request, async server => {
            await waitUntilListening(server, STARTUP_TIMEOUT_MS);
            let page = await waitForDocument(server, SEED_ENTRIES, STARTUP_TIMEOUT_MS);
            let timings: AssetTimings[] = assetUrlsOf(page.snapshot.body).map(url => ({
                url,
                firstMs: 0,
                repeatMs: [],
            }));

            for (let asset of timings) asset.firstMs = await timeRequest(server.baseUrl, asset.url);
            for (let round = 0; round < context.options.iterations; round++) {
                for (let asset of timings) {
                    asset.repeatMs.push(await timeRequest(server.baseUrl, asset.url));
                }
            }
            return summarizeTrial(trial, timings);
        });
    },
};

async function timeRequest(baseUrl: string, url: string): Promise<number> {
    let started = performance.now();
    let { snapshot } = await exchange(baseUrl, { path: url });
    let elapsed = performance.now() - started;
    if (snapshot.status !== 200) throw new Error(`GET ${url} returned ${snapshot.status}`);
    return elapsed;
}

function summarizeTrial(trial: number, timings: AssetTimings[]): SampleRow {
    let scripts = timings.filter(({ url }) => !isStylesheet(url));
    let stylesheets = timings.filter(({ url }) => isStylesheet(url));
    return {
        trial,
        firstScriptsMs: sumFirst(scripts),
        firstStylesheetsMs: sumFirst(stylesheets),
        repeatScriptsMs: medianRepeatSum(scripts),
        repeatStylesheetsMs: medianRepeatSum(stylesheets),
        assets: timings,
    };
}

function isStylesheet(url: string): boolean {
    return url.split("?")[0]!.endsWith(".css");
}

function sumFirst(timings: AssetTimings[]): number {
    return timings.reduce((total, { firstMs }) => total + firstMs, 0);
}

function medianRepeatSum(timings: AssetTimings[]): number {
    let rounds = timings[0]?.repeatMs.length ?? 0;
    let sums = Array.from({ length: rounds }, (_, round) =>
        timings.reduce((total, { repeatMs }) => total + repeatMs[round]!, 0),
    );
    return median(sums);
}
