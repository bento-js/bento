import type { Environment, RuntimeName } from "./results-model.ts";
import type { RuntimeAvailability } from "./runtimes.ts";

import { capture } from "./executables.ts";

const BYTES_PER_GIBIBYTE = 1024 ** 3;
const FIXTURE_PATH = "fixtures/remix-guest-book";

export interface EnvironmentInputs {
    availability: Record<RuntimeName, RuntimeAvailability>;
    remixVersion: string;
    /** A prepared fixture; its devEngines select the pnpm version that installed it. */
    fixtureDirectory: string | undefined;
    ohaExecutable: string | undefined;
    repositoryRoot: string;
}

/** What a reader needs to reproduce a result: the machine, versions, and source revisions. */
export async function collectEnvironment(inputs: EnvironmentInputs): Promise<Environment> {
    let { repositoryRoot } = inputs;
    return {
        hardware: await describeHardware(),
        os: await describeOperatingSystem(),
        runtimes: {
            node: describeRuntime(inputs.availability.node),
            bun: describeRuntime(inputs.availability.bun),
            bento: describeRuntime(inputs.availability.bento),
        },
        tools: {
            pnpm:
                inputs.fixtureDirectory === undefined
                    ? "not used"
                    : await capture("pnpm", ["--version"], inputs.fixtureDirectory),
            oha:
                inputs.ohaExecutable === undefined
                    ? "not installed"
                    : (await capture(inputs.ohaExecutable, ["--version"])).replace(/^oha\s+/, ""),
        },
        revisions: {
            bento: await revision(repositoryRoot, []),
            fixture: await revision(repositoryRoot, [FIXTURE_PATH]),
        },
        remixVersion: inputs.remixVersion,
    };
}

function describeRuntime(availability: RuntimeAvailability): string {
    return availability.available ? availability.version : `unavailable (${availability.reason})`;
}

async function describeHardware(): Promise<string> {
    let model = await capture("sysctl", ["-n", "hw.model"]);
    let chip = await capture("sysctl", ["-n", "machdep.cpu.brand_string"]);
    let cores = await capture("sysctl", ["-n", "hw.ncpu"]);
    let memory = Number(await capture("sysctl", ["-n", "hw.memsize"])) / BYTES_PER_GIBIBYTE;
    return `${model}, ${chip}, ${cores} cores, ${memory} GiB`;
}

async function describeOperatingSystem(): Promise<string> {
    let name = await capture("sw_vers", ["-productName"]);
    let version = await capture("sw_vers", ["-productVersion"]);
    let architecture = await capture("uname", ["-m"]);
    return `${name} ${version} (${architecture})`;
}

/** The short commit that last touched `paths` (or HEAD), marked when the tree has local edits. */
async function revision(root: string, paths: string[]): Promise<string> {
    let scope = paths.length > 0 ? ["--", ...paths] : [];
    let commit = await capture("git", ["log", "-1", "--format=%h", ...scope], root);
    let changes = await capture("git", ["status", "--porcelain", ...scope], root);
    return changes === "" ? commit : `${commit}-dirty`;
}
