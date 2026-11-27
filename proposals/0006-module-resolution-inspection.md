---
id: proposal.0006
title: Module Resolution Inspection
authors: [Mark]
status: draft
pull-request: https://github.com/bento-js/bento/pull/274
issues: []
supersedes: []
---

# Module Resolution Inspection

## Summary

Add a read-only way to ask Bento how it resolves one import: a `bento_runtime` API and a `bento resolve <specifier>` command that report the resolved module, its format, package, and CommonJS named exports, or a structured reason it fails, through the runtime's own resolution code and without running guest code.

## Motivation

When an import fails under Bento, or loads something other than what its author expected, the only way to learn what Bento decided is to run code and infer the decision from what it prints. People debugging an application, coding agents working in one, and Rust hosts embedding Bento all hit this.

[#264](https://github.com/bento-js/bento/issues/264) shows the cost. To demonstrate that `import * as t from "@babel/types"` exposes 63 names under Bento instead of Node's 1,359, its reporter had to save a `probe.mjs` inside `node_modules/.pnpm/babel-dead-code-elimination@1.0.12/node_modules/babel-dead-code-elimination/`, so the probe would resolve the same installed copy of `@babel/types` that the failing package does. They then ran it under both Node and Bento and compared the printed counts. That meant writing into an installed package and executing code, and they still learned nothing about which file Bento analyzed or which re-exports it missed.

Resolution failures are prose. The runtime reports every one as `Cannot resolve "<specifier>" from <referrer>: <resolver message>.`, where the resolver message is `oxc_resolver`'s display text, and appends `For missing installed packages, run bento install.` to every bare specifier. It does so even for a package that is installed but whose `exports` do not include the requested subpath. To tell a missing package from an `exports` mismatch, a missing `#imports` entry, a relative import without its extension, or a `tsconfig.json` `paths` alias that Bento deliberately ignores, an agent has to parse that sentence.

Bento's answers are also not the ones an agent's training predicts. Relative ESM imports must name the complete file, `paths` aliases never apply at runtime, and while grounding this proposal four places turned up where Bento resolves or classifies a module differently from Node 26.13.1 (Beads `bento-i3z`, `bento-a84`, `bento-61m`, `bento-pcq`). An agent that reasons from Node's documentation will mispredict exactly these cases. It needs to observe Bento's answer.

Embedders cannot ask at all. `ARCHITECTURE.md` says Bento "should expose … module-resolution traces … to the host", and the vision says a capability available only through the CLI is a limitation to correct. Today a Rust host can only start an instance and watch it fail.

## Domain grounding

### Established context

- Node separates resolution from loading. ESM resolution returns "the full resolved URL for a module load, along with its suggested module format"; whether the URL can be loaded is decided later, during loading. ([Node 26 ESM resolution algorithm](https://github.com/nodejs/node/blob/v26.13.1/doc/api/esm.md#resolution-algorithm))
- Node implements the export conditions `node-addons`, `node`, `import`, `require`, `module-sync`, and `default`. Key order in `exports` decides priority, `import` and `require` are mutually exclusive, `module-sync` matches for `import`, `import()`, and `require()` alike, and `default` always matches. ([Node 26 conditional exports](https://github.com/nodejs/node/blob/v26.13.1/doc/api/packages.md#conditional-exports))
- A file's format is `module`, `commonjs`, `json`, or `addon`. Extension decides first; for a `.js` file, the nearest `package.json` `type` decides; a `.js` file whose package has no `type` is "ambiguous" and is treated as an ES module when it contains ES module syntax: `import` or `export` statements, `import.meta`, top-level `await`, or redeclaring the CommonJS wrapper variables. Syntax detection has been on by default since v22.7.0. ([Node 26 syntax detection](https://github.com/nodejs/node/blob/v26.13.1/doc/api/packages.md#syntax-detection))
- `package.json` plays two roles that are easy to conflate. The _package scope_ of a file is its nearest `package.json`, whose `type` affects the file's format. The _package root_ is the `package.json` at the top of the package a bare specifier names. `module.findPackageJSON` returns the root for a bare specifier and the nearest manifest for a relative one, and warns that `type` is the least definitive input to a module's format. ([Node 26 `module.findPackageJSON`](https://github.com/nodejs/node/blob/v26.13.1/doc/api/module.md#modulefindpackagejsonspecifier-base))
- When ESM imports CommonJS, Node builds a namespace before the CommonJS module runs. It always contains `default` and `'module.exports'`, plus the names a heuristic static analysis finds on `module.exports`. Detection "does not always correctly detect named exports", and later changes to `module.exports` are not reflected. ([Node 26 CommonJS namespaces](https://github.com/nodejs/node/blob/v26.13.1/doc/api/esm.md#commonjs-namespaces))
- Node answers resolution questions only from running code: `import.meta.resolve()`, `require.resolve()`, `require.resolve.paths()` (the directories searched, or `null` for a built-in), and `module.findPackageJSON()`. None reports a module's format or its CommonJS named exports. ([Node 26 modules](https://github.com/nodejs/node/blob/v26.13.1/doc/api/modules.md#requireresolvepathsrequest))
- Other toolchains inspect resolution without executing the program: TypeScript's [`traceResolution`](https://www.typescriptlang.org/tsconfig/#traceResolution) prints each step of its module resolution, and [`deno info`](https://docs.deno.com/runtime/reference/cli/info/) prints a module's resolved dependency graph, with `--json` for programs.

### Relevant constraints and principles

- The answer is worth having only if it is the runtime's answer. A second resolver that agrees with the runtime most of the time misleads in the edge cases people inspect, and Bento already has three resolution paths that disagree with each other (see Existing baseline). Inspection must call the code the runtime calls.
- Vision: "Embedding is the default." The capability belongs in `bento_runtime`, with the CLI as one host of it. "Give tools one coherent view of the project": a resolution answer must not depend on which tool asked.
- Architecture: "Bento should expose … module-resolution traces … to the host." Architecture also plans a `bento_resolve` crate shared by runtime and tooling; this proposal does not create it (see Alternatives considered).
- [policy.0001](../policies/0001-prefer-voidzero-crates.md): ESM resolution uses `oxc_resolver`, CommonJS export analysis uses Oxc, and `node_resolver` appears only where `deno_node` requires it. Inspection reuses those paths and adds no Deno or SWC crate.
- Proposal 0005 ([#269](https://github.com/bento-js/bento/pull/269)) is the single definition of Bento's command-line wire contract: its [output formats][0005-formats], [structured invocations][0005-structured], [rule for deciding the contract][0005-deciding], [failure documents][0005-failures], [error codes][0005-codes], and [command registry][0005-declarations]. Every command reports through it, and later proposals adopt it rather than restating it. Mark decided on 2026-11-27 that this proposal does so.
- Inspecting must not run the application: no project module is evaluated, nothing is written, and nothing is fetched.

### Quality bar

- For the same instance configuration, referrer, specifier, and kind of request, every answer equals what a running instance does. Differential tests that compare the two prove it.
- A failure says which of the cases a person would fix differently it is, without parsing prose.
- The default output is one module's description, small enough to read in an agent's context; detail such as named exports is opt-in.
- Text that names a Node concept uses Node's term: specifier, conditions, package scope, `module`/`commonjs` formats.

### Remaining uncertainty

- Bento's `require()` of an ambiguous `.js` file that contains ES module syntax may not load it the way Node's `require(esm)` does. The reported `format` is the file's format as Bento's ESM loader classifies it; the differential tests will show whether `require` agrees, and a disagreement is a runtime defect, not an inspection one.
- Hooks a program registers at runtime with `module.registerHooks()` cannot be seen, because inspection runs no program. Node's `module.findPackageJSON` has the same limit.
- The `require` kind needs a V8 isolate. In a local check on 2026-11-27, a debug build's warm `bento run` of a one-line CommonJS script took about 30 ms on macOS arm64; the inspector's own cost is not yet measured.
- `oxc_resolver`'s record of the paths it consulted exists for watch invalidation, not as an ordered trace. Whether it is enough to explain real failures will show once it is used on them.

## Existing baseline

There is no inspection surface. The closest code is the runtime's own resolution and loading, which this proposal reuses rather than imitates. Each observation below comes from origin/main at `d9b00e0`.

`Loader::resolve` in `crates/bento_runtime/src/loader.rs` decides, in this order:

1. `bento:compat` and `bento:subprocess` resolve to themselves.
2. A `host:` specifier resolves to itself when the instance registered that module, and otherwise fails with "the host did not register this module in this instance".
3. A built-in name, with or without `node:`, resolves to `node:<name>` according to `deno_node::is_builtin_node_module`, except that `child_process` resolves to `bento:subprocess` unless `bento:subprocess` is the referrer.
4. A specifier that parses as a URL is accepted as written when its scheme is `file` (nothing checks that the file exists) and otherwise fails with "remote and unregistered module schemes are unavailable".
5. Anything else goes to `oxc_resolver`, from the referrer's directory or, without a file referrer, the instance's working directory. Its options are `condition_names: [node, import, default]`, `fully_specified: true`, `node_path: false`, and `builtin_modules: true`. The answer is the canonical file path.

Watch-input recording is interleaved with that decision. Failures go through `Sources::resolution_error`, which builds the prose message, attaches the import's source location, and records the failure for the host.

`Loader::load` turns a resolved file into a module. A `.json` file becomes a JSON module. Any other file is read with the instance's read permission and, for `.ts`, `.mts`, `.cts`, `.tsx`, and `.jsx`, transformed by `bento_compiler`: with the project's `tsconfig.json` settings for project sources, and with default settings for dependencies, which are classified by `node_modules` path components as `sources.rs` defines. The text then goes through `node_resolver`'s `NodeCodeTranslator`. Bento's `Analyzer` (`crates/bento_runtime/src/cjs/mod.rs`) parses it with Oxc and asks `deno_resolver`'s `CjsTracker` (mode `ImplicitTypeCommonJs`) whether it is CommonJS. It counts only `import` and `export` statements as ES module syntax. For CommonJS, the hand-written visitor in `cjs/exports.rs` collects export names, `require` specifiers re-exported wholesale, and `module.exports = require(X).member` shapes. `node_resolver`'s `CjsModuleExportAnalyzer` resolves the re-exports and analyzes them recursively. The facade's namespace is those names plus `default` and `module.exports`.

Guest `require()` is `deno_node`'s JavaScript `Module._resolveFilename` (`polyfills/01_require.js`), backed by Rust operations; `node_resolver` handles `exports`, `imports`, and self-reference. `NodeServices` (`node.rs`) configures it for the project's own `node_modules`. `op_require_init_paths` returns no global folders, so `NODE_PATH` and the home-directory folders Node searches never apply.

The three paths do not share conditions:

| Bento resolves… | through | with conditions |
| --- | --- | --- |
| `import`, `import()`, `import.meta.resolve` | `oxc_resolver` 11.24.3 in `Loader::resolve` | `node`, `import`, `default` |
| `require()`, `require.resolve()` | `deno_node` 0.197.0 `Module._resolveFilename`, with `node_resolver` 0.97.0 | `require`, `node`, `module-sync`, `default` |
| re-exports of a CommonJS module that ESM imports | `node_resolver` 0.97.0 `CjsModuleExportAnalyzer` | `deno`, `node`, `require`, `module-sync`, `default` |

Probes run on 2026-11-27 against debug builds of the proposal 0003 branch, whose resolution and CommonJS analysis code matches `d9b00e0`, confirmed four divergences from Node 26.13.1, each now a Beads bug outside this proposal's scope:

- `bento-i3z`: an `exports` entry under `module-sync` is ignored by ESM imports, which take `default` instead.
- `bento-a84`: a facade's named exports follow a re-export's `deno` condition while `module.exports` follows `require`, so the namespace lists names the module does not have.
- `bento-61m`: `"imports": { "#fs": "fs" }` fails ESM resolution with "Builtin module node:fs. For missing installed packages, run bento install."
- `bento-pcq`: an ambiguous `.js` file that uses `import.meta` but has no `import` or `export` statement is classified as CommonJS and fails to load.

Preserve the order and outcome of `Loader::resolve`, the dependency classification, the compiler-settings rule, and the facade rule: they are the behavior being reported. The interleaving of watch recording with the decision, the prose-only failure, and the install suggestion attached to every bare specifier are incidental. The benchmark harness (`bench/src/asset-operations`) classifies imports with the npm `oxc-resolver` package; that is a separate resolver for timing work, not a baseline to reuse.

## Proposed solution

`bento_runtime` gains `inspect_resolution`, which answers "what would an instance with this configuration do with this import?". For `import`, it calls the function `Loader::resolve` now delegates its decision to, then prepares the resolved file the way `Loader::load` does, up to but not including evaluation. For `require`, it asks `deno_node`'s own `require.resolve` inside an isolate built the way `start` builds one, loading no project module. The report says what supplies the module, where it is, its format, the package it belongs to, and, on request, the named exports an ES module importing it receives. A failure carries a closed reason code and the facts behind it. `bento resolve` is a command over that API, declared in proposal 0005's registry, that reports through 0005's wire contract.

```text
bento resolve @babel/types --from node_modules/babel-dead-code-elimination --named-exports
→ exit 0; stdout: the resolved lib/index.js, format commonjs, dependency true, package @babel/types@7.29.15,
  and namedExports listing the 63 names plus the per-file analysis that produced them

bento resolve @app/ui --from src/server.ts
→ exit 1; stderr: a failure document with code RESOLUTION_FAILED, reason package-not-found,
  the import's position in src/server.ts as its location, and a suggestion to declare the
  tsconfig.json alias @app/* under package.json imports, since Bento does not apply paths

bento resolve lodash --kind require
→ exit 0; the file deno_node's require() loads, with the require conditions
```

## Detailed design

### Vocabulary

| Term | Meaning |
| --- | --- |
| specifier | The string an `import` or `require` names, exactly as written. |
| referrer | The module the specifier is written in, or a directory to resolve from as if from a module in it. It is given with `--from` (`ResolutionQuery::from` in Rust) and reported as `referrer`, its `file:` URL, as proposal 0007 reports a run's resolution failures. A directory's URL ends with `/`. |
| kind | `import` covers `import` declarations, `import()`, and `import.meta.resolve`; `require` covers `require()`, `require.resolve()`, and `createRequire`. Kind follows the syntax that makes the request, not the referrer's format: a CommonJS module's `import()` is an `import`. TypeScript calls this the resolution mode; Bento says kind because it already uses "mode" for `.env.<mode>` files. |
| module identity | The URL an instance keys the module by: `file:`, `node:`, `host:`, or `bento:`. |
| provider | What supplies the module: `file` (the file system), `builtin` (Node's built-ins), `host` (a module the embedding host registered), or `bento` (a module Bento itself provides). |
| format | How the file is loaded, in Node's terms: `module`, `commonjs`, `json`, or `addon`. Only files have one. |
| package scope | The nearest `package.json` to a file; its `type` is one input to the format. |
| package | The package a file belongs to, identified by its root `package.json`. It differs from the package scope when a package nests a manifest such as `dist/esm/package.json` that only sets `type`. |
| dependency | A file Bento treats as a dependency rather than project source, by the rule in `guides/node-compatibility.md`. |
| conditions | The export condition names a resolver matches in `exports` and `imports` objects, in its configured order, always ending with `default`. |
| named exports | The keys of the namespace an ES module receives when it imports a CommonJS module, which Bento builds before that module runs. The term avoids "exports", which in resolution means the `package.json` field. |
| resolution failure | The resolver found no module for the specifier. |
| load failure | The specifier resolved, but preparing the module for evaluation as the loader does fails, so the import still fails at runtime. |

### Rust API

The API lives in a new `resolution` module of `bento_runtime` and is re-exported at the crate root. Names and fields below are normative. The command's result is the `serde` serialization of `ResolutionReport`, so there is one schema rather than a library model and a CLI model; its keys and enumerated values follow 0005's [structured invocations][0005-structured]. The report types also derive `schemars::JsonSchema` behind a `schema` feature that `bento_cli` enables, so the registry's result schema is generated from them.

```rust
pub fn inspect_resolution(
    config: &RuntimeConfig,
    query: &ResolutionQuery,
) -> Result<ResolutionReport, RuntimeError>;

pub struct ResolutionQuery {
    pub specifier: String,
    /// A file or directory; `None` resolves from `config.cwd` as a directory.
    pub from: Option<PathBuf>,
    pub kind: RequestKind,
}

pub enum RequestKind {
    /// `named_exports` adds the namespace an importing ES module receives.
    Import { named_exports: bool },
    Require,
}

pub struct ResolutionReport {
    pub specifier: String,
    pub kind: RequestKind,
    /// The referrer's `file:` URL.
    pub referrer: String,
    pub project: PathBuf,
    pub conditions: Vec<String>,
    pub outcome: ResolutionOutcome,
}

pub enum ResolutionOutcome {
    Resolved { module: Module, named_exports: Option<NamedExports> },
    ResolutionFailed(ResolutionFailure),
    LoadFailed { module: Module, failure: LoadFailure },
}
```

- `config` is the configuration of the instance being asked about, the same `RuntimeConfig` that `start` takes: `project_root` decides the `tsconfig.json`, the dependency classification, and the project's `node_modules`; `cwd` is the default referrer; `host_modules` decides which `host:` specifiers resolve; the instance's read authority governs the reads `start` would check. Fields that only matter to a running guest, such as `argv` and the shutdown deadline, are ignored. When proposal 0003 replaces `grants` with its permission type, inspection reads through whichever field holds read authority.
- `named_exports` sits inside `RequestKind::Import` because only an ES module import builds a namespace: `require()` returns `module.exports`, whose properties exist only once the module runs.
- `project` and every path in the report are canonical, as the runtime identifies modules, and `referrer` is the URL of the canonical referrer. A directory referrer resolves as if from a module inside it.
- Serialized, `kind` is `import` or `require`, and the outcome is flattened into the report: `Resolved` adds `module` and, when requested, `namedExports`; `ResolutionFailed` adds `failure`; `LoadFailed` adds `module` and `loadFailure`.
- `inspect_resolution` blocks until the report is ready. Its work runs on a thread it starts, so it may be called from any thread, including inside an async runtime. Calls share no state with running instances or with each other, so each reflects the file system at the time of the call.
- It never evaluates a project module, writes no file, loads no `.env` file, and makes no network request. Bento resolves without environment variables, because ESM resolution sets `node_path: false` and `require` searches no global folders.
- It returns `Err(RuntimeError)` only when it cannot answer: `from`, `project_root`, or `cwd` does not exist or cannot be canonicalized (`ErrorKind::Startup`); the project's `tsconfig.json` cannot be read or parsed, which would stop `start` too (`ErrorKind::Transform`); or the isolate for a `require` query cannot be built (`ErrorKind::Startup`). A failed resolution or load is an answer, not an error.

### Resolving an import

- `Loader::resolve`'s decision moves into one crate-private function that both the loader and `inspect_resolution` call. It takes the specifier, the referrer (a file, a directory, or none), the instance's working directory, its registered host specifiers, and the `oxc_resolver` instance, and returns either a module identity or a structured `ResolutionFailure`. It keeps the order and outcomes listed in Existing baseline. The loader keeps its side effects, recording watch inputs and the referrers whose resolutions it tracked, outside that function, and still produces every message it produces today from the structured failure. Existing tests, plus the differential tests below, show that no resolution result or message changes.
- The `oxc_resolver` options, and the condition list they contain, become one definition used by the loader and reported as `conditions`.
- For a query, the function runs with an `oxc_resolver` `ResolveContext`, so a failure can report the paths the resolver read (`found`) and the paths it looked for and did not find (`missing`), each sorted.
- A resolved file is then prepared as `Loader::load` prepares it: a `.json` file is a JSON module; any other file is read as UTF-8 text with the instance's read authority, transformed when its extension is `.ts`, `.mts`, `.cts`, `.tsx`, or `.jsx`, and passed to `CjsModuleExportAnalyzer::analyze_all_exports` with the same `Analyzer`, `CjsTracker`, and `node_resolver` instances `NodeServices` builds for a running instance. This happens whether or not named exports were requested, because a module that cannot be prepared fails its import at runtime, and the report must say so. A `.node` addon therefore fails as `unreadable`, as an `import` of one fails today with "stream did not contain valid UTF-8"; addons load through `require`.

### Resolving a require

- `inspect_resolution` builds the Node environment for `config` the way `start` does — the same worker factory, loader factory, `NodeServices`, and read authority — without loading any project module. In a Bento-internal module, it evaluates `createRequire(from).resolve(specifier)` and `createRequire(from).resolve.paths(specifier)`, passing a directory `from` with a trailing `/`. The answer is therefore `deno_node`'s `Module._resolveFilename`, the code a guest's `require()` runs.
- A returned built-in name, with or without `node:`, has provider `builtin` and identity `node:<name>`. A returned path has provider `file`.
- `conditions` is the `require` condition list of the `NodeServices` resolver, followed by `default`.
- A resolved file's format is decided as for an import, by reading, transforming, parsing, and asking the `CjsTracker`, except that `.json` is `json` and `.node` is `addon` without reading. A file that cannot be read, transformed, or parsed is a load failure, as `require()` of it would be. Named exports do not apply.
- A JavaScript error from `resolve` becomes a `ResolutionFailure` by its `code`, as specified under Resolution failures.

### Describing a resolved module

Every `Module` has `provider` and `url`:

| Provider | Identity | Extra fields |
| --- | --- | --- |
| `file` | `file:` URL of the canonical path | `path`, `format`, `compilerSettings`, `dependency`, `scope`, `package` |
| `builtin` | `node:<name>` | none |
| `host` | `host:<name>` | none |
| `bento` | `bento:<name>` | `replaces`, the built-in it stands in for: `node:child_process` for `bento:subprocess`; absent for `bento:compat` |

For a file:

- `path`: the canonical path. A `file:` URL specifier is taken as written, as the loader takes it, so its path is not canonicalized and may not exist; preparing it then reports a load failure.
- `format`: `json` for `.json`; `addon` for `.node` (reported only for `require`, since an import of an addon is a load failure); otherwise `module` or `commonjs`, as the `CjsTracker` decides from the parse during preparation. In a load failure that happens before the format is decided, such as a syntax error in an ambiguous `.js` file, `format` is `null`.
- `compilerSettings`: `project` when Bento transforms the file with the project's `tsconfig.json` settings, `default` when it transforms a dependency with default settings, and `null` when it does not transform the file.
- `dependency`: `true` when the file has a `node_modules` component in its path relative to the project root, or anywhere in its absolute path when it lies outside the project root.
- `scope`: the nearest `package.json` at or above the file's directory, as `PackageJsonResolver::get_closest_package_json` finds it for the `CjsTracker`, given as `{ "manifest": <path>, "type": "module" | "commonjs" | null }`. It is `null` when there is none.
- `package`: `{ "name", "version", "directory", "manifest" }`, where `name` and `version` are read from the manifest and may be `null`. For a file inside a `node_modules` directory, the manifest is the outermost `package.json` between the file and the nearest `node_modules` directory, or between the file and `node_modules/@scope` for a scoped package. For any other file it is the nearest `package.json`. It is `null` when there is none. This is the rule `oxc_resolver` uses to find a resolution's package manifest; for an import, the inspector reads it from the resolution itself.

### Named exports

- Named exports are computed only for an import whose module is a file, and reported only when `named_exports` is `true`:
    - For a `commonjs` file, `names` is the sorted union of the names `analyze_all_exports` returned and `default` and `module.exports`. Those are the keys of the facade namespace that `NodeCodeTranslator` builds from the same analysis.
    - For a `json` file, `names` is `["default"]`.
    - For a `module` file, `namedExports` is `null`: an ES module's exports are its own declarations, and Bento builds no namespace for it.
- `reexportConditions` lists the conditions `CjsModuleExportAnalyzer` uses to resolve re-exports: `deno`, `node`, `require`, `module-sync`, `default`. `node_resolver` 0.97.0 hard-codes this list, so the inspector reports it as a constant, and a differential test fails if the facade stops following it.
- `analyzed` lists every file the analysis parsed, as Bento's `Analyzer` records them while `node_resolver` calls it. Each entry is `{ "path", "names", "reexports", "memberReexports" }`: the names found in that file, the specifiers it re-exports wholesale, and its `module.exports = require(X).member` shapes as `{ "specifier", "member" }`. The resolved module comes first and the rest follow sorted by path. Recording happens only for an inspection; a running instance's analyzer records nothing.

### Resolution failures

Every `ResolutionFailure` has `reason`, one of the codes below, and `message`, the text the runtime raises for the same failure: for `import`, the message `Sources::resolution_error` builds today; for `require`, the JavaScript error's message. The library reports this text so a host can match it against what a run prints; the command reports the failure through [Error codes](#error-codes) instead.

| `reason` | When | Additional fields |
| --- | --- | --- |
| `file-not-found` | A relative or absolute specifier, or a path inside a found package, names no file. A `file:` URL is not checked here: the loader accepts it as written, so a missing one is a load failure | `path` (relative and absolute specifiers: the absolute path looked for), `candidates` |
| `package-not-found` | No directory for the bare specifier's package exists in any `node_modules` directory searched | `package`, `declaredIn` |
| `subpath-not-exported` | The package was found, and its `exports` does not export the subpath under `conditions` | `package`, `subpath`, `manifest` |
| `import-not-defined` | A `#` specifier matches no entry in the referrer's package scope's `imports` | `manifest` (`null` when the referrer has no package scope) |
| `invalid-package-target` | An `exports` or `imports` entry maps to a target Node does not allow | `manifest`, `key`, `target` |
| `invalid-package-config` | A `package.json` cannot be parsed, or its `exports` mixes subpath and condition keys | `manifest` |
| `invalid-specifier` | The specifier is empty, or not a valid URL, package name, or subpath | none |
| `unknown-builtin` | A `node:` specifier names no built-in | none |
| `unsupported-scheme` | A URL specifier with any other scheme Bento does not load, such as `https:`, `data:`, `npm:`, or `jsr:` | `scheme` |
| `host-module-not-registered` | A `host:` specifier the instance did not register | none |
| `permission-denied` | A read `require` resolution needed was denied. ESM resolution reads without permission checks, as the loader's does today | none |
| `resolver-error` | Any other resolver failure, including `bento-61m`'s `#imports` target naming a built-in | `nodeCode` when the error had one |

- For an import, `oxc_resolver` errors map as follows: `PackagePathNotExported` → `subpath-not-exported`; `PackageImportNotDefined` → `import-not-defined`; `InvalidPackageTarget` → `invalid-package-target`; `InvalidPackageConfig`, `InvalidPackageConfigDefault`, `InvalidPackageConfigDirectory`, and `Json` → `invalid-package-config`; `InvalidModuleSpecifier` and `Specifier` → `invalid-specifier`; `NotFound` → `package-not-found` or `file-not-found` by the rule below; anything else → `resolver-error`. A URL with scheme `node:` whose name is not a built-in is `unknown-builtin`; any other non-`file:` scheme is `unsupported-scheme`.
- For a `require`, error codes map as follows: `ERR_PACKAGE_PATH_NOT_EXPORTED` → `subpath-not-exported`; `ERR_PACKAGE_IMPORT_NOT_DEFINED` → `import-not-defined`; `ERR_INVALID_PACKAGE_TARGET` → `invalid-package-target`; `ERR_INVALID_PACKAGE_CONFIG` → `invalid-package-config`; `ERR_INVALID_MODULE_SPECIFIER` → `invalid-specifier`; a permission error → `permission-denied`; `MODULE_NOT_FOUND` → `unknown-builtin` when the specifier starts with `node:`, and otherwise `package-not-found` or `file-not-found` by the rule below; anything else → `resolver-error`. A `require` failure carries `nodeCode` and the fields the specifier and lookup paths determine (`package`, `declaredIn`, `lookupPaths`), not fields that only the error message would reveal.
- A not-found bare specifier is `package-not-found` when no `node_modules/<package>` directory exists in any directory searched, and `file-not-found` otherwise. For an import, the directories searched are the referrer's directory and each of its ancestors; for a `require`, they are `lookupPaths`. `<package>` is the specifier's first path segment, or its first two when it starts with `@`.
- `declaredIn` (`package-not-found` only) is `{ "manifest", "field" }` when the referrer's package scope lists the package in `dependencies`, `devDependencies`, `optionalDependencies`, or `peerDependencies`, checked in that order, and `null` otherwise. Declared means `bento install` is the fix; undeclared means the package must be added first.
- `candidates` (`file-not-found` for a relative or absolute import only) lists specifiers, written in the same relative or absolute form, that name existing files: the specifier with `.ts`, `.tsx`, `.mts`, `.cts`, `.js`, `.jsx`, `.mjs`, `.cjs`, or `.json` appended; the specifier followed by `/index` and one of those extensions; and the specifier with a final `.js`, `.jsx`, `.mjs`, or `.cjs` replaced by `.ts`, `.tsx`, `.mts`, or `.cts` respectively. They are candidates, not resolution: Bento still requires the complete file name. `require` probes extensions itself and gets none.
- `tsconfigPaths`, attached to a failure of either kind, is `{ "pattern", "targets" }` when the specifier matches a `compilerOptions.paths` key in the project's effective `tsconfig.json`, after `extends`, either exactly or through its single `*`. Keys starting with `#` are skipped, as Bento's `tsconfig.json` diagnostics skip them. It tells the reader the alias is not applied at runtime and that runtime aliases belong in `package.json` `imports`.
- `consulted` (import only) is `{ "found": [...], "missing": [...] }` from the resolver's context. `lookupPaths` (require only) is the value of `require.resolve.paths(specifier)`, or `null`.

### Load failures

A `LoadFailure` has `reason`, `message`, and `location` (`{ "path", "line", "column" }`, one-based, or `null`). The `message` and `location` are those the runtime reports for the same failure.

| `reason` | When |
| --- | --- |
| `unreadable` | The resolved file cannot be read as UTF-8 text: it is missing (a `file:` URL to nothing), the operating system refuses the read, or it is binary, such as an addon imported with `import` |
| `permission-denied` | The instance's read authority does not cover the file |
| `transform-failed` | `bento_compiler` rejects the file, for example JSX under `"jsx": "preserve"` |
| `syntax-error` | Oxc cannot parse the transformed text of the resolved file, or of a file its CommonJS re-exports name |
| `reexport-unresolved` | A re-export named by the CommonJS module, or by a module it re-exports, does not resolve |

### The `bento resolve` command

```text
bento resolve <SPECIFIER> [--from <PATH>] [--kind <import|require>] [--named-exports] [--output <json|text>]
```

- `<SPECIFIER>` is required, and there is exactly one per call.
- `--from` names the referrer, a file or directory, relative to the current directory. Without it, the referrer is the current directory.
- `--kind` defaults to `import`. `--named-exports` is valid only with `--kind import`.
- `--output` accepts `json`, the default, and `text`. Because the default is `json`, every invocation of `bento resolve` is structured under 0005's [rule for deciding the contract][0005-deciding], and it reports as 0005's [structured invocations][0005-structured] say.
- The project is the one the current directory belongs to, found as `bento install` finds it (`Project::for_directory`). The command builds `RuntimeConfig::new(project_root)` with `cwd` set to the current directory, no host modules, and unrestricted read authority: it inspects with the invoking user's own access, like `bento install`, and takes no permission flags. Its `tsconfig.json` diagnostics are not printed; `bento context show` reports them.
- A `Resolved` outcome is the command's result: the serialized `ResolutionReport`, with `module` and, when requested, `namedExports`, on stdout, with exit status 0.
- A `ResolutionFailed` or `LoadFailed` outcome is a failure: a [failure document][0005-failures] on stderr and exit status 1, built as [Error codes](#error-codes) says. So is every failure to answer.
- With `--output text`, a result is rendered for people: the specifier and the resolved path, then one line each for kind and conditions, format and package scope type, package name and version, dependency, compiler settings, and, when requested, the named exports. Failures are still failure documents.

`bento resolve` is declared in 0005's [command registry][0005-declarations]:

| Field | Value |
| --- | --- |
| `id`, `path` | `resolve` |
| `summary` | Show how Bento resolves an import or require specifier |
| `description` | What the command answers, the referrer and project rules above, that it runs no guest code, and that its answer is what `bento run` does from that referrer |
| `arguments` | `specifier` (positional `string`, required); `--from` (option, `path`); `--kind` (option, `choice` of `import` and `require`, default `import`); `--named-exports` (flag); `--output` |
| `output` | `json` |
| `effect` | `reads` |
| `requires` | `project` |
| `dryRun` | no |
| `idempotent` | yes |
| `errors` | `NOT_FOUND`, `PRECONDITION_FAILED`, `INVALID_CONFIGURATION`, `RESOLUTION_FAILED`, `MODULE_LOAD_FAILED`, `SYNTAX_ERROR`, `TRANSFORM_FAILED` |
| `exitStatuses` | `0` when the specifier resolves and its module can be prepared; `1` on failure, with a failure document |
| `examples` | `resolve zod --from src/server.ts`; `resolve lodash --kind require`; `resolve @babel/types --from node_modules/babel-dead-code-elimination --named-exports` |
| `synonyms` | module not found, cannot find package, which file does this import load, package exports, CommonJS named exports, `require.resolve`, `import.meta.resolve` |
| `results` | One `json` result, the schema of `ResolutionReport`'s resolved form |

A resolved import, with named exports, in a project whose dependency `ms` sets `module.exports` to a function:

```json
{
    "specifier": "ms",
    "kind": "import",
    "referrer": "file:///work/app/src/server.ts",
    "project": "/work/app",
    "conditions": ["node", "import", "default"],
    "module": {
        "provider": "file",
        "url": "file:///work/app/node_modules/.pnpm/ms@2.1.3/node_modules/ms/index.js",
        "path": "/work/app/node_modules/.pnpm/ms@2.1.3/node_modules/ms/index.js",
        "format": "commonjs",
        "compilerSettings": null,
        "dependency": true,
        "scope": {
            "manifest": "/work/app/node_modules/.pnpm/ms@2.1.3/node_modules/ms/package.json",
            "type": null
        },
        "package": {
            "name": "ms",
            "version": "2.1.3",
            "directory": "/work/app/node_modules/.pnpm/ms@2.1.3/node_modules/ms",
            "manifest": "/work/app/node_modules/.pnpm/ms@2.1.3/node_modules/ms/package.json"
        }
    },
    "namedExports": {
        "names": ["default", "module.exports"],
        "reexportConditions": ["deno", "node", "require", "module-sync", "default"],
        "analyzed": [
            {
                "path": "/work/app/node_modules/.pnpm/ms@2.1.3/node_modules/ms/index.js",
                "names": [],
                "reexports": [],
                "memberReexports": []
            }
        ]
    }
}
```

### Error codes

0005's [error codes][0005-codes] are the one table of codes. This proposal adds two rows to it; every other failure `bento resolve` reports uses a code 0005 defines.

| Code | Meaning | `retryable` | `details` | `suggestions` |
| --- | --- | --- | --- | --- |
| `RESOLUTION_FAILED` | A module specifier could not be resolved. | `false` | `specifier` and `referrer`, as proposal 0007 defines them. From `bento resolve`, also `kind`, `project`, `conditions`, `reason`, the reason's fields from [Resolution failures](#resolution-failures), and `consulted` or `lookupPaths`. | Those the reporter can determine. From `bento resolve`: when `tsconfigPaths` is set, only declaring the alias under the project's `package.json` `imports`, because Bento does not apply `paths`. Otherwise, in this order: `bento install` when `declaredIn` is set; adding the package to the `dependencies` of the referrer's package scope when the reason is `package-not-found` and `declaredIn` is null; and importing each of `candidates`. |
| `MODULE_LOAD_FAILED` | A module specifier resolved, but the module it names cannot be loaded. | `false` | `specifier`, `referrer`, `kind`, `project`, `conditions`, `module`, and `reason`: `unreadable` or `reexport-unresolved`. For `reexport-unresolved`, `analyzed`, the files the analysis parsed before the failure, as [Named exports](#named-exports) describes them. | For an addon imported with `import`, loading it with `require()` through `createRequire(import.meta.url)`. Otherwise none. |

- 0005 names `RESOLUTION_FAILED` with that meaning, and proposals 0006 and 0007 both use it; whichever is implemented first adds the row. Its base `details`, `specifier` and `referrer`, are 0007's, so either implementation produces the same row, and the fields `bento resolve` adds are compatible additions under 0005's rule for [structured invocations][0005-structured].
- `location` is a failure body's own field, never part of `details`. For `RESOLUTION_FAILED` from `bento resolve`, it is the position of the specifier's literal in the referrer when the referrer is a file that imports or requires it, found as the runtime finds an import's position, and `null` otherwise. For `MODULE_LOAD_FAILED`, it is `null`.
- The `message` is one sentence naming the specifier, the referrer, and what failed, such as `"@app/ui" cannot be resolved from /work/app/src/server.ts: no installed package is named @app/ui.`

Every other failure uses 0005's codes and `details`:

| Failure | Code | `details` and `location` |
| --- | --- | --- |
| The resolved module, or a file its CommonJS re-exports name, cannot be parsed (`syntax-error`) | `SYNTAX_ERROR` | `location` is the syntax error |
| `bento_compiler` rejects the resolved module (`transform-failed`) | `TRANSFORM_FAILED` | `location` is the syntax |
| `--from` names nothing; `--named-exports` with `--kind require`; an unknown `--kind`; `--output ndjson` | `INVALID_ARGUMENT` | `command`, and `argument`: `from`, `named-exports`, `kind`, or `output` |
| No `package.json` in the current directory or above | `NOT_FOUND` | `missing: "package.json"` and `searchedFrom` |
| The project's `package.json` or `tsconfig.json`, or a file it extends, is malformed | `INVALID_CONFIGURATION` | `path`; `location` where the file is not valid JSON |
| A file the command needs cannot be read, or the current directory cannot be determined | `PRECONDITION_FAILED` | `path` when a file is at fault |
| The isolate for a `require` query cannot be built, or any other defect | `INTERNAL` | as 0005 defines |

The command never reports `PERMISSION_DENIED`, because it reads with the invoking user's access. A Rust host that inspects through a restricted configuration receives `permission-denied` reasons in the report instead.

A failed import of a `tsconfig.json` alias, from a referrer whose third line is `import { Button } from "@app/ui";`, with the `consulted` lists shortened and illustrative:

```json
{
    "error": {
        "code": "RESOLUTION_FAILED",
        "message": "\"@app/ui\" cannot be resolved from /work/app/src/server.ts: no installed package is named @app/ui.",
        "retryable": false,
        "details": {
            "specifier": "@app/ui",
            "referrer": "file:///work/app/src/server.ts",
            "kind": "import",
            "project": "/work/app",
            "conditions": ["node", "import", "default"],
            "reason": "package-not-found",
            "package": "@app/ui",
            "declaredIn": null,
            "tsconfigPaths": { "pattern": "@app/*", "targets": ["./src/*"] },
            "consulted": {
                "found": [],
                "missing": ["/work/app/node_modules/@app/ui", "/work/node_modules/@app/ui"]
            }
        },
        "suggestions": [
            "Declare the alias @app/* under \"imports\" in /work/app/package.json, for example \"#app/*\": \"./src/*\", and import \"#app/ui\": tsconfig.json paths are not applied at runtime."
        ],
        "location": { "path": "/work/app/src/server.ts", "line": 3, "column": 24 }
    }
}
```

### Agreement with the runtime

The differential tests are the contract that keeps inspection honest. A fixture project covers relative imports with and without extensions, explicit TypeScript extensions, bare packages under each Bento condition list, unexported subpaths, `#imports`, built-ins with and without `node:`, unknown `node:` names, `child_process`, `https:` URLs, pnpm's symlinked layout, files inside and outside `node_modules`, ambiguous `.js` files, CommonJS facades with wholesale and member re-exports, and a re-export whose package exports differ under `deno`. For each case, a test compares `inspect_resolution` with what a running instance with the same `RuntimeConfig` does from a module at the same referrer: the `import.meta.resolve` result or thrown message, the `require.resolve` result or thrown code, and `Object.keys` of the imported namespace. Where Bento differs from Node, as in the four Beads bugs, the tests pin agreement with Bento, not with Node; fixing one of those bugs changes both sides together.

## Compatibility

No compatibility impact. Runtime resolution results and error messages are unchanged; the loader's refactor is behavior-preserving, and existing tests plus the differential tests guard it. `bento resolve`, `inspect_resolution`, and the `RESOLUTION_FAILED` and `MODULE_LOAD_FAILED` rows are additions, which 0005's contract treats as compatible.

## Implications on adoption

`bento resolve` depends on proposal 0005: it reports through 0005's wire contract and is declared in 0005's registry, so its implementation follows 0005's. `inspect_resolution` has no such dependency, but ships with the command. Users need no configuration and no project changes, and the API is opt-in for embedders. Removing the feature later would remove a command, a function, and two code rows, with no stored state to migrate.

## Scope

- Move `Loader::resolve`'s decision into a shared function returning structured failures, and share the ESM resolver options, without changing any resolution result or message.
- `inspect_resolution` and its types in `bento_runtime`, for both kinds, with module description, named exports and the analysis record, resolution failures with their facts, and load failures, plus the `schema` feature that derives their JSON Schema.
- The `bento resolve` command: its registry declaration, its result, and its failure documents, built on 0005's implemented contract. It adds the `MODULE_LOAD_FAILED` row, and the `RESOLUTION_FAILED` row unless proposal 0007's implementation added it first.
- The differential tests above, plus tests of each failure reason, each code mapping, and each suggestion.
- A guide to inspecting module resolution with `bento resolve`, a pointer to it from the Module resolution section of `guides/node-compatibility.md`, and the API in `guides/embedding-bento.md`.

### Out of scope

- Changing what Bento resolves or how it classifies modules, including the four Beads bugs and [#264](https://github.com/bento-js/bento/issues/264). Inspection reports Bento's behavior; fixes belong to those items, and the differential tests carry them into inspection automatically.
- The wire contract, the registry, and the discovery commands (`bento cli search`, `bento schema`, `bento docs`, `bento context show`) and `bento run --dry-run`, which proposal 0005 defines and implements.
- Listing the exports of ES modules. Their exports are their own declarations, and `export *` chains are resolved by V8 when it links modules; a static listing would be a second linker.
- Resolving every import of a file or a whole module graph, as `deno info` does. One specifier per call keeps output bounded.
- Reporting which `exports` or `imports` key and condition matched. Neither `oxc_resolver` nor `deno_node` exposes it, and re-matching in Bento would be a second resolver.
- Type resolution (`types` conditions, `typesVersions`), which belongs to the type checker.
- Structured runtime errors and events (proposal 0007); inline evaluation (proposal 0008); `node:vfs` (proposal 0010).
- Asking whether a scoped instance may read a resolved file without inspecting through that instance's configuration; the CLI has no permission flags for `resolve`.

## Preview

- Artifact: the installable `bento` executable that the macOS CI job uploads for the pull request (`bento-macos27-arm64-prompt`).
- Reason: the change is a CLI command over a library function, and the executable is the cheapest way to exercise both: the command calls `inspect_resolution` directly, and the crates publish nothing that could serve as a prerelease package. The reviewer scaffolds React Router 8.5.3 as #264 describes and runs `bento resolve @babel/types --from node_modules/babel-dead-code-elimination --named-exports` to see the 63-name namespace and the analysis of `lib/index.js` without writing a probe file. They run the same query with `--kind require`, the Beads bugs' probe trees to watch inspection report Bento's answers and conditions, and the failure cases piped to `jq` and on a terminal, with and without `--output text`. They find the command with `bento cli search "which file does this import load"` and read `bento schema resolve`.

## Policies and decisions checked

- [policy.0001](../policies/0001-prefer-voidzero-crates.md) — ESM inspection uses `oxc_resolver` through the loader's own options, CommonJS analysis uses Bento's Oxc visitor, and `node_resolver` is reached only through `deno_node`'s `require` and `NodeCodeTranslator`'s analyzer, as the policy allows. `bento_runtime` gains `schemars` as an optional dependency, which fills none of the policy's roles.
- `decisions/` — holds no decisions yet.
- [Vision](../VISION.md) — the capability is a library function first and a CLI command second ("Embedding is the default"), and it reports the runtime's own resolution rather than a tool-specific one ("Give tools one coherent view of the project").
- `ARCHITECTURE.md` — satisfies "module-resolution traces" for the host. It does not create `bento_resolve`.
- Proposal 0001 — the resolution behavior its guides describe is unchanged and becomes observable.
- Proposal 0003 ([#259](https://github.com/bento-js/bento/pull/259)) — `bento resolve` takes no permission flags and reads with the invoking user's access, consistent with an unrestricted CLI default; the API reads through the instance's own permission configuration, whatever its type.
- Proposal 0004 ([#268](https://github.com/bento-js/bento/pull/268)) — unaffected: removing tasks does not change resolution or project discovery for `bento install`.
- Proposal 0005 ([#269](https://github.com/bento-js/bento/pull/269)) — on 2026-11-27 Mark decided that 0005 owns the output formats, the failure document, and the code table, and that this proposal adopts them and waits for 0005's implementation. `bento resolve` cites 0005's sections rather than restating them, is declared in its registry, and adds only the rows in [Error codes](#error-codes).
- Proposal 0007 ([#277](https://github.com/bento-js/bento/pull/277)) — shares `RESOLUTION_FAILED`. This proposal uses 0007's `details` fields `specifier` and `referrer`, and its `referrer` vocabulary, so the row is the same whichever proposal adds it.

## Future directions

- Reporting the matched `exports` or `imports` key and condition, if `oxc_resolver` and `node_resolver` expose them.
- Inspecting an entry's whole import graph, as `deno info` does, built from the same shared decision.
- A run's `RESOLUTION_FAILED` carrying the facts `bento resolve` reports, such as `reason` and `declaredIn`, once the runtime records the structured failure; proposal 0007's structured runtime errors are where that would land.
- A reusable inspector that keeps caches across queries, for embedders and editors that ask many questions of one project.
- Extracting the resolution decision into the `bento_resolve` crate the architecture plans, once the compiler or type checker needs resolution without the runtime.

## Alternatives considered

- **Resolve `require` in Rust with `node_resolver::NodeResolver::resolve` and `ResolutionMode::Require`.** It avoids an isolate, but a guest's `require()` runs `deno_node`'s JavaScript `Module._resolveFilename` and `Module._findPath`; `node_resolver`'s Rust `require` mode is separate code for extension probing, `main` fields, directory indexes, and self-reference. It would be a second resolver that disagrees in exactly the edge cases people inspect.
- **Take the format from `oxc_resolver`'s `module_type`.** That option implements `ESM_FILE_FORMAT` without syntax detection, while the loader decides through the `CjsTracker` after parsing, so the two disagree on ambiguous files.
- **Generate a probe script and run it with `bento run`.** That is #264's workaround made automatic. It needs a module at the referrer's location, `import.meta.resolve` reports neither format nor package, and observing named exports means evaluating the module, which runs its code and side effects.
- **Trace resolution during a real run, like `tsc --traceResolution`.** It answers a different question, everything a run resolved, and needs the application to execute. It could reuse the shared decision later.
- **Ship `bento resolve` with its own JSON output and failure envelope, and adopt proposal 0005's registry later.** It would let this proposal land first, but contracts defined one proposal at a time drift. Mark decided on 2026-11-27 that 0005 owns the wire contract and that this proposal waits for it.
- **Create `bento_resolve` now.** `require` inspection and the facade analysis depend on `NodeServices`, the `Analyzer`, and `deno_node`, all inside `bento_runtime`; moving them is a refactor with its own design questions.
- **`--exports` and an `exports` field.** In resolution, "exports" means the `package.json` field, which this report also discusses. "Named exports" is Node's own phrase for the namespace's names.
- **`importer` or `from` as the report's name for the referrer.** The CLI keeps `--from` because it reads naturally, but `RESOLUTION_FAILED` is shared with proposal 0007, which calls the field `referrer`, as `deno_core`'s module loader and Bento's own loader do.
- **`bento modules resolve`, in resource-operation grammar.** Bento's commands are top-level verbs (`run`, `install`), and "resolve" is Node's word for this operation in `require.resolve` and `import.meta.resolve`.

## Open questions

- [NEEDS CLARIFICATION: Should a failed resolution be a failure, as drafted: a failure document on stderr with exit status 1 under 0005's structured contract? Or should it be a result: a report on stdout with a `failure` outcome and exit status 0? The draft makes the exit status the answer to "does this resolve?"; the alternative keeps every answer on stdout for `jq` and reserves failure documents for failing to answer.]
- [NEEDS CLARIFICATION: Which project does the CLI inspect for? The draft uses the current directory's project, as `bento install` does, which suits running from an application's root and referrers inside its `node_modules`. The alternative is the project enclosing `--from`, skipping `node_modules`, which suits monorepos but misjudges workspace sources another project loads. The choice decides the `tsconfig.json`, the dependency classification, and the project root `NodeServices` is configured with for `require`.]

[0005-formats]: https://github.com/bento-js/bento/blob/e88c817d339f2d5c6f56daa31bcc2f3412c2cd1b/proposals/0005-agent-first-cli-discovery.md#output-formats
[0005-structured]: https://github.com/bento-js/bento/blob/e88c817d339f2d5c6f56daa31bcc2f3412c2cd1b/proposals/0005-agent-first-cli-discovery.md#structured-invocations
[0005-deciding]: https://github.com/bento-js/bento/blob/e88c817d339f2d5c6f56daa31bcc2f3412c2cd1b/proposals/0005-agent-first-cli-discovery.md#deciding-the-contract
[0005-failures]: https://github.com/bento-js/bento/blob/e88c817d339f2d5c6f56daa31bcc2f3412c2cd1b/proposals/0005-agent-first-cli-discovery.md#failure-documents
[0005-codes]: https://github.com/bento-js/bento/blob/e88c817d339f2d5c6f56daa31bcc2f3412c2cd1b/proposals/0005-agent-first-cli-discovery.md#error-codes
[0005-declarations]: https://github.com/bento-js/bento/blob/e88c817d339f2d5c6f56daa31bcc2f3412c2cd1b/proposals/0005-agent-first-cli-discovery.md#declarations
