import { setTimeout as sleep } from "node:timers/promises";

import type { Workload } from "../workload.ts";

import {
    STARTUP_TIMEOUT_MS,
    waitForDocument,
    waitUntilListening,
    withServer,
} from "../app-server.ts";
import { assetUrlsOf } from "../document-gate.ts";
import { exchange } from "../http-exchange.ts";
import { sampleTreeRssKb, startRssSampler } from "../process-memory.ts";
import { SEED_ENTRIES } from "../seed-entries.ts";
import { median } from "../statistics.ts";
import { BOTH_MODES } from "../workload.ts";

const PEAK_SAMPLE_INTERVAL_MS = 20;
const IDLE_SETTLE_MS = 1_500;
const IDLE_SAMPLES = 10;
const IDLE_SAMPLE_INTERVAL_MS = 100;
const KILOBYTES_PER_MEGABYTE = 1024;

export const memory: Workload = {
    name: "memory",
    title: "Memory",
    question:
        "How much resident memory does the whole process tree hold? Peak is the largest sample from spawn through startup and the first request for every asset; idle is the median of samples taken after the server has been quiet for a moment. Sampling is about every 20 ms, so the peak is a lower bound.",
    metrics: [
        {
            key: "peakRssMb",
            label: "Peak RSS, startup through first asset compilation",
            unit: "MB",
        },
        { key: "idleRssMb", label: "Idle RSS after startup", unit: "MB" },
    ],
    variants: BOTH_MODES,
    requiresApplicationGate: true,
    async runTrial({ runtime, executable, fixture, mode, trial }) {
        let request = { runtime, executable, fixture, mode, watch: false };
        return withServer(request, async server => {
            let sampler = startRssSampler(server.process.pid, PEAK_SAMPLE_INTERVAL_MS);
            let startupSamples: number[];
            try {
                await waitUntilListening(server, STARTUP_TIMEOUT_MS);
                let page = await waitForDocument(server, SEED_ENTRIES, STARTUP_TIMEOUT_MS);
                for (let url of assetUrlsOf(page.snapshot.body)) {
                    let { snapshot } = await exchange(server.baseUrl, { path: url });
                    if (snapshot.status !== 200)
                        throw new Error(`GET ${url} returned ${snapshot.status}`);
                }
            } finally {
                startupSamples = await sampler.stop();
            }

            await sleep(IDLE_SETTLE_MS);
            let idleSamples: number[] = [];
            for (let sample = 0; sample < IDLE_SAMPLES; sample++) {
                idleSamples.push(await sampleTreeRssKb(server.process.pid));
                await sleep(IDLE_SAMPLE_INTERVAL_MS);
            }
            return {
                trial,
                peakRssMb: Math.max(...startupSamples) / KILOBYTES_PER_MEGABYTE,
                idleRssMb: median(idleSamples) / KILOBYTES_PER_MEGABYTE,
                startupSampleCount: startupSamples.length,
            };
        });
    },
};
