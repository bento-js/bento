// Runs inside the disposable fixture under each runtime, through that runtime's own direct-source
// launch path. It times the native calls behind `remix/assets` on the fixture's own files with the
// options `remix/assets` uses, and prints one JSON report on stdout. If any call reports that it
// failed, it prints that failure on stderr instead and exits 1: there is no timing of failed work.
import { createHash } from "node:crypto";
import { readdirSync, readFileSync } from "node:fs";
import path from "node:path";

// Results are typed as possibly absent because a runtime's Node-API layer can fail to return one.
interface Diagnostics {
    errors: Array<{ message: string }>;
}
interface Parser {
    parseSync(
        filename: string,
        source: string,
        options: { lang: string; sourceType: string },
    ):
        | (Diagnostics & { module: { staticImports: Array<{ moduleRequest: { value: string } }> } })
        | undefined;
}
interface Resolver {
    ResolverFactory: new (options: object) => {
        resolveFileAsync(
            file: string,
            request: string,
        ): Promise<{ path?: string; error?: string } | undefined>;
    };
}
interface Compiler {
    transform(
        filename: string,
        source: string,
        options: object,
    ): Promise<(Diagnostics & { code: string; map?: unknown }) | undefined>;
    minify(
        filename: string,
        source: string,
        options: object,
    ): Promise<(Diagnostics & { code: string; map?: unknown }) | undefined>;
}
interface StyleCompiler {
    transform(options: object): { code: Uint8Array; map?: Uint8Array } | undefined;
}

interface Input {
    file: string;
    /** Relative to the fixture root, for failure messages. */
    relativePath: string;
    source: string;
    lang: "css" | "ts" | "tsx";
}
interface ImportRequest {
    file: string;
    specifier: string;
    /** The import and its importer, for failure messages. */
    subject: string;
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

const NO_RESULT = "returned no result";

// The probe resolves server modules too. A `node:` built-in or a `host:` module such as
// `host:guestbook` is provided by the runtime rather than found as a file, so the file resolver's
// not-found answer for it is that import's expected, recorded outcome. Any other scheme, such as
// `https:`, is a remote import, which execution never performs: its resolver error is a failure.
const RUNTIME_MODULE = /^(?:node|host):/;

/** A native call reported that it could not do its work, so the pass has nothing valid to time. */
class OperationFailure extends Error {
    constructor(operation: Operation, subject: string, diagnostic: string) {
        super(`${operation} failed on ${subject}: ${diagnostic}`);
    }
}

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
try {
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
} catch (error) {
    if (!(error instanceof OperationFailure)) throw error;
    console.error(error.message);
    process.exitCode = 1;
}

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
            if (lang === undefined) return [];
            let relativePath = path.relative(root, file);
            return [{ file, relativePath, source: readFileSync(file, "utf8"), lang }];
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
        scripts.map(({ file, relativePath, source, lang }) => {
            let result = attempt("parse", relativePath, () =>
                parser.parseSync(file, source, { lang, sourceType: "module" }),
            );
            return withoutErrors("parse", relativePath, result);
        }),
    );
    let requests: ImportRequest[] = scripts.flatMap((input, index) =>
        parsed.value[index]!.module.staticImports.map(({ moduleRequest }) => ({
            file: input.file,
            specifier: moduleRequest.value,
            subject: `${JSON.stringify(moduleRequest.value)} from ${input.relativePath}`,
        })),
    );
    let resolved = await measure(async () => {
        let outcomes: string[] = [];
        for (let request of requests) {
            let result = await attemptAsync("resolve", request.subject, () =>
                resolver.resolveFileAsync(request.file, request.specifier),
            );
            outcomes.push(resolution(request, result));
        }
        return outcomes;
    });
    let transformed = await measure(async () => {
        let outputs: string[] = [];
        for (let { file, relativePath, source, lang } of scripts) {
            let jsx = lang === "tsx" ? { jsx: JSX_OPTIONS } : {};
            let result = await attemptAsync("transform", relativePath, () =>
                compiler.transform(file, source, {
                    lang,
                    sourceType: "module",
                    sourcemap: true,
                    ...jsx,
                }),
            );
            let { code, map } = withoutErrors("transform", relativePath, result);
            outputs.push(code, JSON.stringify(map));
        }
        return outputs;
    });
    let minified = await measure(async () => {
        let outputs: string[] = [];
        for (let [index, { file, relativePath }] of scripts.entries()) {
            let result = await attemptAsync("minify", relativePath, () =>
                compiler.minify(file, transformed.value[index * 2]!, MINIFY_OPTIONS),
            );
            let { code, map } = withoutErrors("minify", relativePath, result);
            outputs.push(code, JSON.stringify(map));
        }
        return outputs;
    });
    let css = await measure(async () =>
        stylesheets.flatMap(({ file, relativePath, source }) => {
            // Lightning CSS reports an error by throwing; its warnings are not failures.
            let result = attempt("css", relativePath, () =>
                styles.transform({
                    analyzeDependencies: { preserveImports: true },
                    code: new TextEncoder().encode(source),
                    filename: file,
                    minify: true,
                    sourceMap: true,
                }),
            );
            if (result?.code === undefined)
                throw new OperationFailure("css", relativePath, NO_RESULT);
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

/** Runs a synchronous native call, attributing anything it throws to that operation. */
function attempt<Value>(operation: Operation, subject: string, call: () => Value): Value {
    try {
        return call();
    } catch (error) {
        let message = error instanceof Error ? error.message : String(error);
        throw new OperationFailure(operation, subject, message);
    }
}

/** Awaits an asynchronous native call, attributing a throw or rejection to that operation. */
async function attemptAsync<Value>(
    operation: Operation,
    subject: string,
    call: () => Promise<Value>,
): Promise<Value> {
    try {
        return await call();
    } catch (error) {
        let message = error instanceof Error ? error.message : String(error);
        throw new OperationFailure(operation, subject, message);
    }
}

/** An Oxc result fails on any entry in its `errors` array, as `remix/assets` treats it. */
function withoutErrors<Result extends Diagnostics>(
    operation: Operation,
    subject: string,
    result: Result | undefined,
): Result {
    if (result?.errors === undefined) throw new OperationFailure(operation, subject, NO_RESULT);
    let [first] = result.errors;
    if (first !== undefined) throw new OperationFailure(operation, subject, first.message);
    return result;
}

function resolution(
    request: ImportRequest,
    result: { path?: string; error?: string } | undefined,
): string {
    if (result?.path) return path.relative(root, result.path);
    if (result?.error !== undefined && RUNTIME_MODULE.test(request.specifier)) return result.error;
    throw new OperationFailure("resolve", request.subject, result?.error ?? NO_RESULT);
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
