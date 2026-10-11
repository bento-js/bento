import type { Fixture } from "./fixture.ts";

import {
    STARTUP_TIMEOUT_MS,
    waitForDocument,
    waitUntilListening,
    withServer,
} from "./app-server.ts";
import { saveSeedDatabase } from "./database.ts";
import { exchange } from "./http-exchange.ts";
import { SEED_ENTRIES } from "./seed-entries.ts";
import { evaluateRedirect } from "./submission-gate.ts";

/**
 * Signs the seed entries through the application's own `POST /`, served by Node, so the database
 * every runtime starts from was written by the application and not by the harness.
 */
export async function seedDatabase(fixture: Fixture, nodeExecutable: string): Promise<void> {
    let request = {
        runtime: "node",
        executable: nodeExecutable,
        fixture,
        mode: "production",
        watch: false,
    } as const;
    await withServer(request, async server => {
        await waitUntilListening(server, STARTUP_TIMEOUT_MS);
        await waitForDocument(server, [], STARTUP_TIMEOUT_MS);
        for (let { name, message } of SEED_ENTRIES) {
            let { snapshot } = await exchange(server.baseUrl, {
                path: "/",
                method: "POST",
                form: { name, message },
            });
            let problems = evaluateRedirect(snapshot);
            if (problems.length > 0) {
                throw new Error(`Seeding ${name} failed: ${problems.join("; ")}`);
            }
        }
        await waitForDocument(server, SEED_ENTRIES, STARTUP_TIMEOUT_MS);
    });
    await saveSeedDatabase(fixture.directory);
}
