import type { TestContext } from "remix/test";

import { chmod, mkdtemp, realpath, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import * as assert from "remix/assert";
import { it } from "remix/test";

import { SetupError } from "./errors.ts";
import { bentoIdentity, changedExecutable, nodeIdentity } from "./runtimes.ts";

async function executable(t: TestContext, output: string) {
    let directory = await mkdtemp(path.join(tmpdir(), "bento-compat-runtime-"));
    t.after(() => rm(directory, { recursive: true, force: true }));
    let file = path.join(directory, "runtime");
    await writeFile(file, `#!/bin/sh\necho ${JSON.stringify(output)}\n`);
    await chmod(file, 0o755);
    return { directory, file };
}

it(
    "records the version a Bento executable reports, with its hash and launch arguments",
    { timeout: 20_000 },
    async t => {
        let { directory, file } = await executable(t, "bento 9.9.9");
        let identity = await bentoIdentity("./runtime", ["--permission"], {
            cwd: directory,
            environment: process.env,
        });
        assert.equal(identity.version, "9.9.9");
        assert.equal(identity.executable, await realpath(file));
        assert.match(identity.sha256, /^[0-9a-f]{64}$/);
        assert.deepEqual(identity.arguments, ["run", "--permission"]);
    },
);

it("fails setup instead of inventing a runtime version", { timeout: 20_000 }, async t => {
    let { directory } = await executable(t, "something else");
    let environment = process.env;
    await assert.rejects(
        bentoIdentity("./runtime", [], { cwd: directory, environment }),
        SetupError,
    );
    await assert.rejects(
        bentoIdentity("./missing", [], { cwd: directory, environment }),
        SetupError,
    );
    let wrongNode = await executable(t, "v25.0.0");
    await assert.rejects(
        nodeIdentity(wrongNode.file, { cwd: wrongNode.directory, environment }),
        SetupError,
    );
});

it(
    "detects an executable replaced after its identity was recorded",
    { timeout: 20_000 },
    async t => {
        let { directory, file } = await executable(t, "bento 1.0.0");
        let identity = await bentoIdentity(file, [], { cwd: directory, environment: process.env });
        assert.equal(await changedExecutable("Bento", identity), null);
        await writeFile(file, "#!/bin/sh\necho 'bento 1.0.1'\n");
        assert.match(
            (await changedExecutable("Bento", identity)) ?? "",
            /Bento executable .* changed/,
        );
    },
);
