import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";

import type { InventoryResult, RunReport, RuntimeIdentity } from "../lib/model.ts";
import type { ProcessResult } from "./process.ts";

import { SetupError } from "./errors.ts";
import { runProcess } from "./process.ts";
import { SCRATCH_MANIFEST } from "./scratch.ts";
import { NODE_COMMIT, NODE_TAG, NODE_VERSION } from "./upstream.ts";

export const INVENTORY_SUITE: RunReport["suite"] = {
    version: NODE_VERSION,
    revision: NODE_COMMIT,
    source: `nodejs/node ${NODE_TAG}`,
    scope: "globalThis and public node: built-in module exports; names and typeof only, one process per module",
};
export const GLOBALS_MODULE = "globalThis";
const MODULE_ENTRY = "*";
const MARKER = "__BENTO_COMPAT_INVENTORY__";
const PROBE_STDOUT_BYTES = 1024 * 1024;

export type ProbeOutcome =
    | { ok: true; symbols: [string, string][]; errors: [string, string][] }
    | { ok: false; missingModule: boolean; message: string };

export interface ProbeOptions {
    directory: string;
    environment: NodeJS.ProcessEnv;
    timeoutMs: number;
    signal: AbortSignal;
}

export interface InventoryContext {
    node: RuntimeIdentity;
    bento: RuntimeIdentity;
    environment: NodeJS.ProcessEnv;
    timeoutMs: number;
    signal: AbortSignal;
}

export async function collectInventory(
    report: RunReport,
    context: InventoryContext,
    progress: (line: string) => void,
): Promise<void> {
    let directory = await mkdtemp(path.join(tmpdir(), "bento-compat-inventory-"));
    try {
        await writeFile(path.join(directory, "package.json"), SCRATCH_MANIFEST);
        let options: ProbeOptions = { directory, ...context };
        let nodeCommand = [context.node.executable, ...context.node.arguments];
        let bentoCommand = [context.bento.executable, ...context.bento.arguments];
        let targets = [null, ...(await baselineModules(nodeCommand, options))];
        let baseline: [string | null, ProbeOutcome][] = [];
        for (let specifier of targets) {
            let outcome = await probe(nodeCommand, specifier, options);
            if (outcome === null) return;
            baseline.push([specifier, outcome]);
        }
        let expected = baseline.reduce(
            (total, [, outcome]) =>
                total + (outcome.ok ? outcome.symbols.length + outcome.errors.length : 1),
            0,
        );
        report.selection = { filter: "", total: expected, selected: expected };
        progress(`Node baseline: ${baseline.length} targets, ${expected} entries`);
        for (let [index, [specifier, node]] of baseline.entries()) {
            let bento = node.ok ? await probe(bentoCommand, specifier, options) : null;
            if (node.ok && bento === null) return;
            let entries = compareModule(specifier ?? GLOBALS_MODULE, node, bento);
            report.inventory.push(...entries);
            let present = entries.filter(entry => entry.status === "present").length;
            progress(
                `[${index + 1}/${baseline.length}] ${specifier ?? GLOBALS_MODULE}: ${present}/${entries.length} present`,
            );
        }
    } finally {
        await rm(directory, { recursive: true, force: true });
    }
}

export async function baselineModules(command: string[], options: ProbeOptions): Promise<string[]> {
    let file = "./list-builtins.mjs";
    await writeFile(
        path.join(options.directory, file),
        `import { builtinModules } from "node:module";\nconsole.log(${JSON.stringify(MARKER)} + JSON.stringify(builtinModules));\n`,
    );
    let result = await run(command, file, options);
    if (result.interrupted) return [];
    let modules = markedValue(result);
    if (
        !Array.isArray(modules) ||
        !modules.every((name): name is string => typeof name === "string")
    ) {
        throw new SetupError(
            `Could not list the Node control's built-in modules: ${failure(result)}`,
        );
    }
    return modules
        .map(name => (name.startsWith("node:") ? name : `node:${name}`))
        .filter(name => !name.startsWith("node:_"))
        .sort();
}

export async function probe(
    command: string[],
    specifier: string | null,
    options: ProbeOptions,
): Promise<ProbeOutcome | null> {
    let file = `./probe-${(specifier ?? GLOBALS_MODULE).replace(/[^a-zA-Z0-9]+/g, "-")}.mjs`;
    await writeFile(path.join(options.directory, file), probeSource(specifier));
    let result = await run(command, file, options);
    return result.interrupted ? null : readProbe(result);
}

export function readProbe(result: ProcessResult): ProbeOutcome {
    let value = markedValue(result);
    if (value === undefined) return { ok: false, missingModule: false, message: failure(result) };
    if (typeof value !== "object" || value === null)
        return { ok: false, missingModule: false, message: "The probe printed a malformed result" };
    if ("error" in value && typeof value.error === "string") {
        let missingModule = "missingModule" in value && value.missingModule === true;
        return { ok: false, missingModule, message: value.error };
    }
    if (
        "symbols" in value &&
        "errors" in value &&
        isPairs(value.symbols) &&
        isPairs(value.errors)
    ) {
        return { ok: true, symbols: value.symbols, errors: value.errors };
    }
    return { ok: false, missingModule: false, message: "The probe printed a malformed result" };
}

export function compareModule(
    module: string,
    node: ProbeOutcome,
    bento: ProbeOutcome | null,
): InventoryResult[] {
    if (!node.ok) {
        let error = `Node baseline could not be inspected: ${node.message}`;
        return [{ module, name: MODULE_ENTRY, node: "error", bento: null, status: "error", error }];
    }
    if (bento === null) throw new TypeError(`${module} has a Node baseline but no Bento probe`);
    let entries = node.errors.map(([name, message]): InventoryResult => ({
        module,
        name,
        node: "error",
        bento: null,
        status: "error",
        error: `Node baseline could not read this export: ${message}`,
    }));
    let types = new Map(bento.ok ? bento.symbols : []);
    let readErrors = new Map(bento.ok ? bento.errors : []);
    for (let [name, type] of node.symbols) {
        let error = bentoError(module, bento, readErrors.get(name));
        if (error !== null) {
            entries.push({ module, name, node: type, bento: null, status: "error", error });
        } else {
            let actual = types.get(name) ?? null;
            entries.push({
                module,
                name,
                node: type,
                bento: actual,
                status: presence(type, actual),
                error: null,
            });
        }
    }
    return entries.sort(
        (left, right) => Number(left.name > right.name) - Number(left.name < right.name),
    );
}

function bentoError(
    module: string,
    bento: ProbeOutcome,
    readError: string | undefined,
): string | null {
    if (!bento.ok) {
        return bento.missingModule ? null : `Bento could not import ${module}: ${bento.message}`;
    }
    return readError === undefined ? null : `Bento could not read this export: ${readError}`;
}

function run(command: string[], file: string, options: ProbeOptions): Promise<ProcessResult> {
    return runProcess({
        command: [...command, file],
        cwd: options.directory,
        env: options.environment,
        timeoutMs: options.timeoutMs,
        signal: options.signal,
        stdoutHeadBytes: PROBE_STDOUT_BYTES,
    });
}

function markedValue(result: ProcessResult): unknown {
    if (result.exitCode !== 0) return undefined;
    let line = result.stdout.split("\n").findLast(candidate => candidate.startsWith(MARKER));
    if (line === undefined) return undefined;
    try {
        return JSON.parse(line.slice(MARKER.length));
    } catch {
        return undefined;
    }
}

function presence(expected: string, actual: string | null): InventoryResult["status"] {
    if (actual === null) return "missing";
    return actual === expected ? "present" : "type-mismatch";
}

function failure(result: ProcessResult): string {
    let output = (result.stderr.trim() || result.stdout.trim()).split("\n").slice(-5).join("\n");
    let outcome = describeExit(result);
    return output ? `The probe ${outcome}: ${output}` : `The probe ${outcome}`;
}

function describeExit(result: ProcessResult): string {
    if (result.spawnError !== null) return `could not start (${result.spawnError})`;
    if (result.timedOut) return "timed out";
    if (result.signal !== null) return `was terminated by ${result.signal}`;
    if (result.exitCode !== 0) return `exited with code ${result.exitCode}`;
    return "printed no readable result";
}

function isPairs(value: unknown): value is [string, string][] {
    return (
        Array.isArray(value) &&
        value.every(
            pair =>
                Array.isArray(pair) &&
                pair.length === 2 &&
                pair.every(part => typeof part === "string"),
        )
    );
}

function probeSource(specifier: string | null): string {
    return `const specifier = ${JSON.stringify(specifier)};
const describe = error => {
    try {
        return error instanceof Error ? \`\${error.name}: \${error.message}\` : String(error);
    } catch {
        return "unprintable error";
    }
};
let result;
try {
    const target = specifier === null ? globalThis : await import(specifier);
    const names = specifier === null ? Object.getOwnPropertyNames(globalThis) : Object.keys(target);
    const symbols = [];
    const errors = [];
    for (const name of names) {
        try {
            symbols.push([name, typeof target[name]]);
        } catch (error) {
            errors.push([name, describe(error)]);
        }
    }
    result = { symbols, errors };
} catch (error) {
    let missingModule = false;
    if (specifier !== null) {
        try {
            const { isBuiltin } = await import("node:module");
            missingModule = !isBuiltin(specifier);
        } catch {
            // Without node:module, the import failure remains an inspection error.
        }
    }
    result = { error: describe(error), missingModule };
}
console.log(${JSON.stringify(MARKER)} + JSON.stringify(result));
`;
}
