import { copyFile, mkdir, mkdtemp, symlink, writeFile } from "node:fs/promises";
import path from "node:path";

/** The native packages the asset-operations probe loads, by the operation each one performs. */
export const PROBE_PACKAGES = {
    parse: "oxc-parser",
    resolve: "oxc-resolver",
    transform: "oxc-transform",
    minify: "oxc-minify",
    css: "lightningcss",
} as const;
export type ProbeOperation = keyof typeof PROBE_PACKAGES;

const PROBE_SOURCE = path.join(import.meta.dirname, "../asset-operations/probe.ts");
const REPOSITORY_PARSER = path.resolve(import.meta.dirname, "../../../node_modules/oxc-parser");

// Stand-ins that succeed the way the real packages do, source maps included. The resolver, like
// oxc-resolver, finds files and answers "Cannot find module" for anything else, such as `node:path`.
const SUCCEEDING_STAND_INS: Record<Exclude<ProbeOperation, "parse">, string> = {
    resolve: `
        import { existsSync } from "node:fs";
        import path from "node:path";
        export class ResolverFactory {
            async resolveFileAsync(file, request) {
                let target = path.resolve(path.dirname(file), request);
                return existsSync(target) ? { path: target } : { error: \`Cannot find module '\${request}'\` };
            }
        }`,
    transform: `export async function transform(file, source) {
        return { code: source, map: { mappings: "" }, errors: [] };
    }`,
    minify: `export async function minify(file, source) {
        return { code: source, map: { mappings: "" }, errors: [] };
    }`,
    css: "export function transform({ code }) { return { code, map: code, warnings: [] }; }",
};

/** Sources that every operation handles without a diagnostic. */
export const CLEAN_SOURCES: Record<string, string> = {
    "app/entry.ts": 'import { greeting } from "./greeting.ts";\nexport const message = greeting;\n',
    "app/greeting.ts": 'export const greeting = "hello";\n',
    "app/styles.css": "p { color: red; }\n",
    "app/view.tsx": "export function View() {\n    return <p>hello</p>;\n}\n",
};

/**
 * A fixture laid out the way the harness prepares one: the real probe in `.bench/` beside its
 * packages, a database directory with a seed, and the `remix/node-tsx` loader Node is launched with.
 * Parsing uses the repository's real oxc-parser; every other operation uses a stand-in. `standIns`
 * replaces the package behind an operation, the parser included, with the given module source.
 */
export async function createProbeFixture(
    parent: string,
    options: {
        sources: Record<string, string>;
        standIns?: Partial<Record<ProbeOperation, string>>;
    },
): Promise<string> {
    let directory = await mkdtemp(path.join(parent, "fixture-"));
    for (let [file, source] of Object.entries(options.sources)) {
        await writeSource(path.join(directory, file), source);
    }
    await mkdir(path.join(directory, "db"));
    await mkdir(path.join(directory, ".bench/seed"), { recursive: true });
    await mkdir(path.join(directory, ".bench/node_modules"));
    await copyFile(PROBE_SOURCE, path.join(directory, ".bench/probe.ts"));
    await writePackage(path.join(directory, "node_modules/remix"), "./node-tsx", "");

    let standIns = { ...SUCCEEDING_STAND_INS, ...options.standIns };
    for (let [operation, name] of Object.entries(PROBE_PACKAGES)) {
        let packageDirectory = path.join(directory, ".bench/node_modules", name);
        let standIn = standIns[operation as ProbeOperation];
        if (standIn === undefined) {
            await symlink(REPOSITORY_PARSER, packageDirectory);
        } else {
            await writePackage(packageDirectory, ".", standIn);
        }
    }
    return directory;
}

async function writePackage(directory: string, entry: string, source: string): Promise<void> {
    let manifest = {
        name: path.basename(directory),
        type: "module",
        exports: { [entry]: "./index.js" },
    };
    await writeSource(path.join(directory, "package.json"), JSON.stringify(manifest));
    await writeSource(path.join(directory, "index.js"), source);
}

async function writeSource(file: string, source: string): Promise<void> {
    await mkdir(path.dirname(file), { recursive: true });
    await writeFile(file, source);
}
