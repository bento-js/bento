import { readdir, readFile } from "node:fs/promises";
import { builtinModules } from "node:module";
import path from "node:path";

import { SetupError } from "./errors.ts";

export const CORPUS_DIRECTORIES = ["parallel", "sequential"] as const;

export interface CorpusCase {
    id: string;
    path: string;
    module: string;
    flags: string[];
    env: Record<string, string>;
}

const TEST_FILE = /^test-.*\.(?:m?js|cjs)$/;
const FLAGS_PATTERN = /\/\/\s+Flags:(.*)/;
const ENV_PATTERN = /\/\/\s+Env:(.*)/;

const MODULE_PREFIXES = builtinModules
    .map(name => name.replace(/^node:/, ""))
    .filter(name => !name.startsWith("_") && !name.includes("/"))
    .map(name => ({ name, prefix: name.replaceAll("_", "-") }))
    .sort((left, right) => right.prefix.length - left.prefix.length);

export function parseMetadata(source: string, file: string): Pick<CorpusCase, "flags" | "env"> {
    let flags = FLAGS_PATTERN.exec(source)?.[1]?.trim().split(/\s+/).filter(Boolean) ?? [];
    let env: Record<string, string> = {};
    for (let pair of ENV_PATTERN.exec(source)?.[1]?.trim().split(/\s+/).filter(Boolean) ?? []) {
        let separator = pair.indexOf("=");
        if (separator <= 0) throw new SetupError(`${file} has a malformed // Env: entry: ${pair}`);
        env[pair.slice(0, separator)] = pair.slice(separator + 1);
    }
    return { flags, env };
}

export function moduleOf(fileName: string): string {
    let stem = fileName.replace(/^test-/, "").replace(/\.[^.]+$/, "");
    let match = MODULE_PREFIXES.find(
        ({ prefix }) => stem === prefix || stem.startsWith(`${prefix}-`),
    );
    return match?.name ?? stem.split("-")[0]!;
}

export async function listCorpus(root: string): Promise<CorpusCase[]> {
    let cases: CorpusCase[] = [];
    for (let directory of CORPUS_DIRECTORIES) {
        let entries = await readdir(path.join(root, "test", directory), { withFileTypes: true });
        let files = entries
            .filter(entry => entry.isFile() && TEST_FILE.test(entry.name))
            .map(entry => entry.name);
        for (let file of files.sort()) {
            let relative = path.posix.join("test", directory, file);
            let source = await readFile(path.join(root, relative), "utf8");
            cases.push({
                id: `${directory}/${file}`,
                path: relative,
                module: moduleOf(file),
                ...parseMetadata(source, relative),
            });
        }
    }
    return cases;
}

export function selectCases(cases: CorpusCase[], filter: string | null): CorpusCase[] {
    if (filter === null) return cases;
    let selected = cases.filter(testCase => testCase.id.includes(filter));
    if (selected.length === 0) {
        throw new SetupError(
            `--filter ${JSON.stringify(filter)} matches none of the ${cases.length} behavioral test files`,
        );
    }
    return selected;
}
