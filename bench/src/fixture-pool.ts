import path from "node:path";

import type { Fixture } from "./fixture.ts";
import type { RuntimeName } from "./results-model.ts";

import { prepareFixture } from "./fixture.ts";

/** One fresh disposable fixture per runtime, prepared the first time a runtime needs it. */
export function createFixturePool(options: { sourceDirectory: string; runDirectory: string }) {
    let fixtures = new Map<RuntimeName, Promise<Fixture>>();
    let prepared: Fixture[] = [];

    function fixtureFor(runtime: RuntimeName): Promise<Fixture> {
        let fixture =
            fixtures.get(runtime) ??
            prepareFixture({
                sourceDirectory: options.sourceDirectory,
                directory: path.join(options.runDirectory, runtime),
                nodeExecutable: process.execPath,
            }).then(ready => {
                prepared.push(ready);
                return ready;
            });
        fixtures.set(runtime, fixture);
        return fixture;
    }

    return { fixtureFor, prepared: () => prepared };
}
