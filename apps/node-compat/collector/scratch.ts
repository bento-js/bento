import { mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import path from "node:path";

import { CORPUS_DIRECTORIES } from "./corpus.ts";
import { git } from "./upstream.ts";

export const SCRATCH_MANIFEST = '{"private":true}\n';
// Upstream tests bind Unix sockets beneath their temporary directory; macOS's
// default $TMPDIR is long enough to exceed the 104-byte socket path limit.
const SCRATCH_PARENT = "/tmp";

export interface Workspace {
    root: string;
    testDirectory: string;
}

export interface Scratch {
    node: Workspace;
    bento: Workspace;
    remove(): Promise<void>;
}

export async function createScratch(checkout: string): Promise<Scratch> {
    let parent = await mkdtemp(path.join(SCRATCH_PARENT, "bento-compat-"));
    let remove = () => rm(parent, { recursive: true, force: true, maxRetries: 3 });
    try {
        let node = await createWorkspace(checkout, parent, "node");
        let bento = await createWorkspace(checkout, parent, "bento");
        return { node, bento, remove };
    } catch (error) {
        await remove();
        throw error;
    }
}

async function createWorkspace(checkout: string, parent: string, name: string): Promise<Workspace> {
    let root = path.join(parent, name);
    let testDirectory = path.join(parent, `${name}-tmp`);
    await git(checkout, [
        "--no-optional-locks",
        "checkout-index",
        "--all",
        "--force",
        `--prefix=${root}${path.sep}`,
    ]);
    for (let directory of CORPUS_DIRECTORIES) {
        await writeFile(path.join(root, "test", directory, "package.json"), SCRATCH_MANIFEST, {
            flag: "wx",
        });
    }
    await mkdir(testDirectory);
    return { root, testDirectory };
}
