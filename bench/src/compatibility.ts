import type { Fixture } from "./fixture.ts";
import type { HttpEvidence, Mode, RuntimeName } from "./results-model.ts";

import { failingCheck, runAcceptance } from "./acceptance.ts";
import {
    ServerStartupError,
    STARTUP_TIMEOUT_MS,
    waitUntilListening,
    withServer,
} from "./app-server.ts";
import { resetDatabase } from "./database.ts";
import { SEED_ENTRIES } from "./seed-entries.ts";

export type Compatibility =
    | { compatible: true }
    | { compatible: false; check: string; failures: string[]; evidence: HttpEvidence[] };

export interface CompatibilityRequest {
    runtime: RuntimeName;
    executable: string;
    fixture: Fixture;
    mode: Mode;
}

/** Starts the runtime on the fixture and runs the acceptance gates; no timing is taken. */
export async function checkCompatibility(request: CompatibilityRequest): Promise<Compatibility> {
    try {
        return await withServer({ ...request, watch: false }, async server => {
            await waitUntilListening(server, STARTUP_TIMEOUT_MS);
            let report = await runAcceptance({
                baseUrl: server.baseUrl,
                mode: request.mode,
                seededEntries: SEED_ENTRIES,
            });
            let failed = failingCheck(report);
            if (failed === undefined) return { compatible: true };
            return {
                compatible: false,
                check: failed.name,
                failures: failed.failures,
                evidence: failed.evidence,
            };
        });
    } catch (error) {
        if (!(error instanceof ServerStartupError)) throw error;
        return { compatible: false, check: "startup", failures: [error.message], evidence: [] };
    } finally {
        await resetDatabase(request.fixture.directory);
    }
}

/** Remembers each runtime's verdict per mode so gates run once per run. */
export function createCompatibilityGate() {
    let verdicts = new Map<string, Promise<Compatibility>>();
    return (request: CompatibilityRequest): Promise<Compatibility> => {
        let key = `${request.runtime}:${request.mode}`;
        let verdict = verdicts.get(key) ?? checkCompatibility(request);
        verdicts.set(key, verdict);
        return verdict;
    };
}
