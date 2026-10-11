// Runs inside the disposable fixture under each runtime, through that runtime's own direct-source
// launch path. It times the native calls behind `remix/assets` on the fixture's own files with the
// options `remix/assets` uses, and prints one JSON report on stdout.
import { createHash } from "node:crypto";
import { readdirSync, readFileSync } from "node:fs";
import path from "node:path";

interface Diagnostics {
    errors: unknown[];
}
interface Parser {
    parseSync(
        filename: string,
        source: string,
        options: { lang: string; sourceType: string },
    ): Diagnostics & { module: { staticImports: Array<{ moduleRequest: { value: string } }> } };
}
interface Resolver {
    ResolverFactory: new (options: object) => {
        resolveFileAsync(file: string, request: string): Promise<{ path?: string; error?: string }>;
    };
}
interface Compiler {
    transform(
        filename: string,
        source: string,
        options: object,
    ): Promise<Diagnostics & { code: string; map?: unknown }>;
    minify(
        filename: string,
        source: string,
        options: object,
    ): Promise<Diagnostics & { code: string; map?: unknown }>;
}
interface StyleCompiler {
    transform(options: object): { code: Uint8Array; map?: Uint8Array };
}

interface Input {
    file: string;
    source: string;
    lang: "css" | "ts" | "tsx";
}
interface Measured {
    ms: number;
    digest: string;
}

const OPERATIONS = ["parse", "resolve", "transform", "minify", "css"] as const;
type Operation = (typeof OPERATIONS)[number];

const RESOLVER_OPTIONS = {
    aliasFields: [["browser"]],
    conditionNames: ["browser", "import", "module", "default"],
    extensionAlias: {
        ".js": [".js", ".ts", ".tsx", ".jsx"],
        ".jsx": [".jsx", ".tsx"],
        ".mjs": [".mjs", ".mts"],
    },
    extensions: [".ts", ".tsx", ".js", ".jsx", ".mts", ".mjs"],
    mainFields: ["browser", "module", "main"],
    tsconfig: "auto",
};
const JSX_OPTIONS = { development: false, importSource: "remix/component", runtime: "automatic" };
const MINIFY_OPTIONS = { compress: true, mangle: true, module: true, sourcemap: true };

let root = path.resolve(import.meta.dirname, "..");
let iterations = Number(process.argv[2]);
if (!Number.isInteger(iterations) || iterations < 1) {
    throw new Error("usage: probe.ts <iterations>");
}

let parser = await loadPackage<Parser>("oxc-parser");
let compiler = {
    ...(await loadPackage<Compiler>("oxc-transform")),
    ...(await loadPackage<Compiler>("oxc-minify")),
};
let styles = await loadPackage<StyleCompiler>("lightningcss");
let { ResolverFactory } = await loadPackage<Resolver>("oxc-resolver");
let resolver = new ResolverFactory(RESOLVER_OPTIONS);

let inputs = listInputs(path.join(root, "app"));
let passes: Array<Record<Operation, Measured>> = [];
for (let pass = 0; pass < iterations; pass++) passes.push(await runPass(inputs));

console.log(
    JSON.stringify({
        scripts: inputs.filter(input => input.lang !== "css").length,
        styles: inputs.filter(input => input.lang === "css").length,
        bytes: inputs.reduce((total, input) => total + Buffer.byteLength(input.source), 0),
        passes,
    }),
);

// These packages exist only in the installed fixture, so the repository's type check cannot resolve
// them statically. They are the locked packages `remix/assets` calls, shaped by the interfaces above.
async function loadPackage<Shape>(name: string): Promise<Shape> {
    return (await import(name)) as Shape;
}

function listInputs(directory: string): Input[] {
    return readdirSync(directory, { withFileTypes: true })
        .sort((left, right) => (left.name < right.name ? -1 : 1))
        .flatMap(entry => {
            let file = path.join(directory, entry.name);
            if (entry.isDirectory()) return listInputs(file);
            let lang = languageOf(entry.name);
            return lang === undefined ? [] : [{ file, source: readFileSync(file, "utf8"), lang }];
        });
}

function languageOf(name: string): Input["lang"] | undefined {
    if (name.includes(".test.")) return undefined;
    if (name.endsWith(".tsx")) return "tsx";
    if (name.endsWith(".ts")) return "ts";
    return name.endsWith(".css") ? "css" : undefined;
}

async function runPass(allInputs: Input[]): Promise<Record<Operation, Measured>> {
    let scripts = allInputs.filter(input => input.lang !== "css");
    let stylesheets = allInputs.filter(input => input.lang === "css");

    let parsed = await measure(async () =>
        scripts.map(input =>
            parser.parseSync(input.file, input.source, { lang: input.lang, sourceType: "module" }),
        ),
    );
    let requests = scripts.flatMap((input, index) =>
        parsed.value[index]!.module.staticImports.map(({ moduleRequest }) => ({
            file: input.file,
            specifier: moduleRequest.value,
        })),
    );
    let resolved = await measure(async () => {
        let outcomes: string[] = [];
        for (let { file, specifier } of requests) {
            let result = await resolver.resolveFileAsync(file, specifier);
            outcomes.push(result.path ? path.relative(root, result.path) : String(result.error));
        }
        return outcomes;
    });
    let transformed = await measure(async () => {
        let outputs: string[] = [];
        for (let { file, source, lang } of scripts) {
            let jsx = lang === "tsx" ? { jsx: JSX_OPTIONS } : {};
            let result = await compiler.transform(file, source, {
                lang,
                sourceType: "module",
                sourcemap: true,
                ...jsx,
            });
            outputs.push(result.code, JSON.stringify(result.map));
        }
        return outputs;
    });
    let minified = await measure(async () => {
        let outputs: string[] = [];
        for (let [index, { file }] of scripts.entries()) {
            let result = await compiler.minify(file, transformed.value[index * 2]!, MINIFY_OPTIONS);
            outputs.push(result.code, JSON.stringify(result.map));
        }
        return outputs;
    });
    let css = await measure(async () =>
        stylesheets.flatMap(({ file, source }) => {
            let result = styles.transform({
                analyzeDependencies: { preserveImports: true },
                code: new TextEncoder().encode(source),
                filename: file,
                minify: true,
                sourceMap: true,
            });
            return [
                Buffer.from(result.code).toString("utf8"),
                Buffer.from(result.map ?? []).toString("utf8"),
            ];
        }),
    );

    return {
        parse: summarize(
            parsed.ms,
            parsed.value.map(result => JSON.stringify(result.module.staticImports)),
        ),
        resolve: summarize(resolved.ms, resolved.value),
        transform: summarize(transformed.ms, transformed.value),
        minify: summarize(minified.ms, minified.value),
        css: summarize(css.ms, css.value),
    };
}

async function measure<Value>(work: () => Promise<Value>): Promise<{ ms: number; value: Value }> {
    let start = performance.now();
    let value = await work();
    return { ms: performance.now() - start, value };
}

// Source maps embed the fixture's absolute path (lightningcss drops its leading slash); the digest
// must not depend on where the fixture copy lives.
function summarize(ms: number, outputs: string[]): Measured {
    let hash = createHash("sha256");
    for (let output of outputs) {
        hash.update(output.replaceAll(root, "<fixture>").replaceAll(root.slice(1), "<fixture>"));
    }
    return { ms, digest: hash.digest("hex").slice(0, 16) };
}
