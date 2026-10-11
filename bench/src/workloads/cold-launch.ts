import type { Workload } from "../workload.ts";

import {
    STARTUP_TIMEOUT_MS,
    waitForDocument,
    waitUntilListening,
    withServer,
} from "../app-server.ts";
import { SEED_ENTRIES } from "../seed-entries.ts";
import { BOTH_MODES } from "../workload.ts";

export const coldLaunch: Workload = {
    name: "cold-launch",
    title: "Cold launch",
    question:
        "How long from spawning the process until the port accepts connections, and until the first response that passes the document gate? The second milestone includes application initialization.",
    metrics: [
        { key: "listeningMs", label: "Spawn to listening", unit: "ms" },
        { key: "firstPageMs", label: "Spawn to first correct page", unit: "ms" },
    ],
    variants: BOTH_MODES,
    requiresApplicationGate: true,
    async runTrial({ runtime, executable, fixture, mode, trial }) {
        let request = { runtime, executable, fixture, mode, watch: false };
        return withServer(request, async server => {
            let listeningAt = await waitUntilListening(server, STARTUP_TIMEOUT_MS);
            let page = await waitForDocument(server, SEED_ENTRIES, STARTUP_TIMEOUT_MS);
            return {
                trial,
                listeningMs: listeningAt - server.spawnedAt,
                firstPageMs: page.at - server.spawnedAt,
            };
        });
    },
};
