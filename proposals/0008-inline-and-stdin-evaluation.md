---
id: proposal.0008
title: Inline and Stdin Source Evaluation
authors: [Mark]
status: draft
pull-request: https://github.com/bento-js/bento/pull/275
issues: []
supersedes: []
---

# Inline and Stdin Source Evaluation

## Summary

`bento run` gains Node 26's `-e`/`--eval`, `-p`/`--print`, `-` (standard input), and `--input-type`, so JavaScript, TypeScript, and TSX run without a source file or a `package.json`. `bento_runtime` gains `start_source`, which evaluates in-memory source from a host-chosen directory and reports its value as JSON.

## Motivation

Bento can only run code that is already a file inside a project. `bento run` requires a script path, and refuses a script with no `package.json` above it (`no package.json found in … or any parent directory`). `bento_runtime::start` canonicalizes its entry path on disk, so it fails for source that exists only in memory.

Two groups hit this:

- **People and agents running one-off code.** `node -e`, `node -p "require('./package.json').version"`, and piping generated code into `node -` are everyday Node idioms. Package scripts, READMEs, and CI steps use them, and coding agents use them constantly to probe a project: evaluate an expression, check what an import resolves to, try a snippet of TypeScript. None of these works under Bento. The tools that start Node again with its own command line, tracked in [#260](https://github.com/bento-js/bento/issues/260), include `-e` and `-p` in that command line.
- **Rust hosts evaluating code they hold in memory.** An agent harness or native application that wants to run a snippet must write it into a project directory first. `RunOutcome` then reports only `Completed` or `Exited(code)`, so a host that wants the snippet's result has to route it through a host module or parse the guest's standard output.

The workaround in both cases is a temporary file inside a project: write it, run it, delete it. It leaves files behind when the run is interrupted, restarts any `bento run --watch` watching that project, gives the code a misleading `__filename`, and, under [proposal.0003](https://github.com/bento-js/bento/pull/259)'s `--permission`, requires granting read access to the temporary path. Outside a project there is no workaround at all: the code needs a `package.json` that has nothing to do with it.

## Domain grounding

The behavior below was observed with Node 26.13.1 on macOS arm64 on 2026-11-27, in directories with and without a `package.json`, unless a source is cited. Deno 2.9.8 and Bun 1.4.5 were observed for comparison.

### Established context

- **Three forms of string input.** Besides a file, Node takes program text from `-e`/`--eval` ("evaluate the following argument as JavaScript"), `-p`/`--print` ("identical to `-e` but prints the result"), and standard input. `-` names standard input explicitly: "the script is read from stdin, and the rest of the options are passed to that script." Without a file name, Node also reads standard input when it is not a terminal; when it is a terminal, `node` and `node -` start the REPL. ([Node 26 command-line options](https://github.com/nodejs/node/blob/v26.13.1/doc/api/cli.md))
- **`--input-type`** accepts exactly `module`, `commonjs`, `module-typescript`, and `commonjs-typescript`. Any other value, `module-tsx` included, fails before anything runs with `--input-type must be "module","commonjs", "module-typescript" or "commonjs-typescript"` and status 9. With a file entry it fails with `ERR_INPUT_TYPE_NOT_ALLOWED: --input-type can only be used with string input via --eval, --print, or STDIN` and status 1.
- **Detection without `--input-type`.** Node's implementation ([`eval_string.js`](https://github.com/nodejs/node/blob/v26.13.1/lib/internal/main/eval_string.js), [`eval_stdin.js`](https://github.com/nodejs/node/blob/v26.13.1/lib/internal/main/eval_stdin.js), [`execution.js`](https://github.com/nodejs/node/blob/v26.13.1/lib/internal/process/execution.js)) evaluates string input as an ES module when it contains ES module syntax, and as CommonJS otherwise. In either case it first compiles the text as JavaScript and strips TypeScript types only when that fails, so `const a = 1, b = 2, c = 3; console.log(a < b > (c))` prints `false` instead of being read as a generic call. The [syntax detection rules](https://github.com/nodejs/node/blob/v26.13.1/doc/api/packages.md) count `import` and `export` declarations (not `import()` expressions), `import.meta`, and top-level `await`. Observed: type-only declarations count too (`import type { X } from "y"` runs as a module), while `const require = 1` stays CommonJS for string input. A `package.json` `type` field never affects string input.
- **CommonJS string input is a script, not a module.** Node compiles it as a classic script named `[eval]` (or `[stdin]`) and runs it in the global scope after setting `module`, `exports`, `require`, `__dirname`, and `__filename` as globals. Observed: a top-level `var x` becomes `globalThis.x`; `this === globalThis`; `__filename` is `[eval]` and `__dirname` is `.`; `module.id` is `[eval]` and `module.filename` is `<cwd>/[eval]`; `module.paths` lists `node_modules` from the working directory upward; `require.main` is `undefined`; stack frames read `at [eval]:1:13`.
- **Module string input is an ES module** at `file://<cwd>/[eval1]`, for `-e` and standard input alike. `import.meta.filename` is `<cwd>/[eval1]`, `import.meta.dirname` is the working directory, `require` is undefined, and top-level `await` works. `import.meta.main` is `true` under `--input-type=module` or `module-typescript` but `false` when detection chose the module system.
- **Resolution.** Relative `require`, `import`, `import()`, and package `imports` (`#util`) resolve from the working directory; packages resolve from `node_modules` in the working directory and every ancestor. No `package.json` is needed anywhere, and Node never loads `.env` files unless asked to with `--env-file`.
- **Arguments.** With `-e` or `-p`, `process.argv` is `[execPath, ...arguments]`: there is no script element. With `-` it is `[execPath, '-', ...arguments]`. Positional words after the code, a file name or `-` included, are arguments, not programs. Options between the code and the first argument are still Node's; a `--` before the first argument is consumed; after the first argument every word passes unchanged.
- **Printing.** `-p` logs the completion value once with `console.log`, from a `beforeExit` listener registered after evaluation, or from an `exit` listener if the process exits first. `-p 'setTimeout(() => console.log("timer"), 10); 5'` prints `timer` and then `5`; `-p 'process.exit(2); 5'` prints nothing and exits 2; `-p '(async () => 42)()'` prints `Promise { 42 }`, unawaited. ECMAScript gives a script a completion value ([ScriptEvaluation](https://tc39.es/ecma262/#sec-runtime-semantics-scriptevaluation)) and a module none ([module Evaluate](https://tc39.es/ecma262/#sec-moduleevaluation) returns a promise), so `-p` with module input, explicit or detected, fails before evaluating with `ERR_EVAL_ESM_CANNOT_PRINT: --print cannot be used with ESM input` and status 1.
- **Built-in globals.** `-e` and `-p`, but not standard input, define the built-in modules as globals, as the REPL does: `fs`, `path`, `os`, `child_process`, `readline`, and every other built-in requirable without `node:` whose name has no `/` and no leading `_` (`sqlite` and `test`, which require the `node:` prefix, are absent). Each loads on first access and is replaced by assignment. This holds in both module systems. Separately, when `-e` or `-p` code is not given as `--input-type=module` and its text contains `crypto`, `crypto` names `node:crypto` rather than Web Crypto, so `-p 'typeof crypto.createHash'` prints `function` while `globalThis.crypto` stays Web Crypto. Node does this by wrapping `-e` code in a function, `(crypto=>{{CODE}})(require('node:crypto'))`. Observed side effects: such code's top-level `var` declarations no longer become globals, and code that also has module syntax fails with `'import', and 'export' cannot be used outside of module code`.
- **TypeScript.** String input uses Node's strip-only mode ([Node 26 TypeScript](https://github.com/nodejs/node/blob/v26.13.1/doc/api/typescript.md)): enums, namespaces with values, and parameter properties fail with `ERR_UNSUPPORTED_TYPESCRIPT_SYNTAX`, and JSX is unsupported in any form.
- **Permissions.** Under `--permission`, `-e`, `-p`, and `-` evaluate with no `--allow-fs-read`, and `process.permission.has('fs.read', process.cwd())` is `false`. Every module the code loads from disk needs read permission: `import './local.mjs'` fails with `ERR_ACCESS_DENIED`, permission `FileSystemRead`, resource `<cwd>/local.mjs`; `require('dep')` is denied reading `node_modules/dep/package.json`. Built-in modules load.
- **Edges.** `-e` without a value fails with `-e requires an argument` (status 9); `-e ''` runs nothing and exits 0, and `-p ''` prints `undefined`. `-e` with `--watch` fails with `either --watch or --eval can be used, not both`. When both `-e` and `-p` appear, Node evaluates the last code given and prints if `-p` appeared anywhere. `-p` without code prints the result of standard input. A code string beginning with `-` is passed as `--eval=-42`. Standard input is decoded as UTF-8 with invalid bytes replaced by U+FFFD; after Node reads it, the guest's `process.stdin` yields nothing. With `-e`, standard input is left for the code to read.
- **Prior art.** Deno's `deno eval` evaluates its argument as an ES module named `$deno$eval.mts`, TypeScript by default, and implements `--print` by wrapping the code in `console.log(…)`: `deno eval -p 'await Promise.resolve(42)'` prints `42`, but `-p '1; 2'` is a syntax error. Bun's `-e` and `-p` name the code `[eval]`, accept TypeScript and JSX, and `-p` prints the completion value even after `await`. Neither has `--input-type`.
- **REPL evaluation is different.** Node's REPL and V8's REPL mode, which browser developer tools use, report the value of the last expression even in code that uses top-level `await`, by rewriting the code or compiling it in a special mode. One-shot string input in Node does neither.

### Relevant constraints and principles

- The vision's "Embedding is the default" says "a feature available only through a CLI is a limitation to correct". Every behavior the CLI needs, printing and built-in globals included, is therefore a library capability the CLI composes.
- "Compatibility is behavior, not a list of names." Flags spelled like Node's must behave like Node's, or the difference must be stated.
- "Treat TypeScript as the working language." Inline TypeScript and TSX use the same transform and settings as files.
- "Make host authority explicit." The host supplies inline source; the guest does not read it, so reading it is not a permission the guest needs.
- The architecture's embedding contract lists "asynchronous evaluation, preloaded or virtual modules, structured result exchange", and says cross-thread callers "use message passing and serializable results rather than exchanging engine-native handles". The evaluated value crosses to the host as JSON, never as an engine handle.
- [proposal.0003](https://github.com/bento-js/bento/pull/259), approved for implementation, makes launches unrestricted unless they pass `--permission`, implies read access only to the file `bento run` executes, and implies nothing for embeddings. This proposal uses its vocabulary and assumes it lands first.
- proposal.0004 ([PR #268](https://github.com/bento-js/bento/pull/268), draft) would make every `bento run` target a script path. This proposal does not depend on it.
- [policy.0001](../policies/0001-prefer-voidzero-crates.md) requires Oxc for parsing and transformation, which covers syntax detection and the TypeScript transform here.

### Quality bar

- For every command line in this design that Node 26.13.1 also accepts, `bento run` gives Node's result: which module system and syntax ran, standard output including `--print` output, `process.argv`, `__filename`, `__dirname`, `module.id`, `module.filename`, `import.meta.filename` and `dirname`, what relative specifiers and packages resolve to, which built-in globals exist, the `code`, `permission`, and `resource` of a denial, and the exit status. The differences listed under [Where Bento differs from Node](#where-bento-differs-from-node) are the only exceptions. Differential tests run both.
- Inline TypeScript and TSX run with the transform, `tsconfig.json` settings, and source-mapped error positions that a `.ts` or `.tsx` file in the same project gets.
- A Rust host evaluates source it holds in memory without creating any file, and receives the value as JSON.

### Remaining uncertainty

- Bento resolves ES module imports with `oxc_resolver`, which searches every ancestor's `node_modules`, but CommonJS `require` goes through Deno's npm resolver, configured to stop at the project root. Whether a project-less launch's `require` finds a package installed above the invocation directory, as Node's does, or needs that configuration changed, is unverified. The required behavior is stated below either way.
- The run loop in `deno_lib` 0.77 dispatches Node's `beforeExit` and `exit` events, and Deno's `process.exit` emits `exit` in JavaScript before ending execution, so printing and capturing the value at either moment appear feasible. Capturing after `process.exit` without adding a guest-visible listener is unproven.

## Existing baseline

- **`bento_runtime::start(config, entry: &Path)`** canonicalizes the project root, working directory, and entry, so all three must exist. The loader reads every module from disk after a read-permission check, transforms it by file extension through `Sources::transform`, and records it as a watch input. The project's `tsconfig.json` is loaded once from `RuntimeConfig.project_root`, whose doc comment already describes a directory rather than requiring a `package.json`: the library does not need one, only the CLI does.
- **The loader already serves source from memory.** `bento:compat`, `bento:subprocess`, and every registered `host:` module are returned as in-memory text without touching the file system. This is the precedent for an inline entry.
- **Module-system detection already uses Oxc.** The CommonJS export analyzer parses with `SourceType::unambiguous()` and treats a program with no module declaration as a script.
- **Values already cross the host boundary as JSON.** Host-module functions receive and return `serde_json::Value`.
- **`RunOutcome`** is `Completed`, `Exited(i32)`, `Failed(RuntimeError)`, or `Shutdown(ShutdownReport)`. It carries no value. `RuntimeError::guest` prefixes messages with the entry path, and `SourceLocation` names a file.
- **The CLI.** `bento run` requires a target, chooses a script or task with `select_target`, and finds a script's project with `Project::for_script`, which fails without a `package.json`. `Launch::prepare` resolves permissions and loads `.env`, `.env.<mode>`, and `.env.local` from the project root. The error reporter quotes source lines by reading the file at the error's location. `bento run -e` is an unknown option today.

Worth keeping: launch preparation, environment loading, signal forwarding, exit codes, error kinds, source-mapped locations, and code-frame reports all apply to inline code unchanged. Incidental, and changed here: the assumptions that an entry is a file on disk, and that a launch needs a `package.json`.

## Proposed solution

Bento accepts Node's forms of string input on `bento run` and gives them Node's meaning. The code needs no file and no project, TypeScript works in it as it does in files, and TSX is one `--input-type` away.

```text
bento run -e 'console.log(process.argv.slice(1))' a b               → [ 'a', 'b' ]
bento run -p 'const answer: number = 41; answer + 1'                → 42
bento run -p 'os.platform()'                                        → darwin
echo 'const n: number = await Promise.resolve(7); console.log(n)' | bento run -   → 7
cd /tmp && bento run -p 'path.join("a", "b")'                       → a/b (no package.json anywhere)
bento run --permission -e 'import "./seed.ts"'                      → ERR_ACCESS_DENIED (FileSystemRead) for …/seed.ts
bento run -p 'export const x = 1'                                   → fails: --print cannot be used with ESM input
bento run --input-type=module-tsx -e 'console.log(<p>hi</p>)'       → runs with the project's JSX settings
```

The CLI is built on a library entry point that any Rust host can use:

```rust
let source = InlineSource::new("export default await Promise.resolve({ ok: true });", &root);
let evaluation = bento_runtime::start_source(config, source)?.wait_for_evaluation();
assert!(matches!(evaluation.outcome, RunOutcome::Completed));
assert_eq!(evaluation.value, Some(EvaluatedValue::Json(json!({ "default": { "ok": true } }))));
```

## Detailed design

### Vocabulary

- An **inline source** is program text given to an instance directly, by `-e`, `-p`, standard input, or a Rust host, instead of read from an entry file. Node calls it string input. An instance runs exactly one entry: an entry file or an inline source.
- An inline source is placed at a virtual location: a **name**, such as `[eval]` or `[stdin]`, inside an existing **directory**. The location is its identity in stack traces, `__filename`, and `import.meta.url`, and the place its relative specifiers and packages resolve from. Nothing is read from or written to it.
- Its **module system** is whether it runs as an ES module or as a CommonJS script. Its **syntax** is JavaScript, TypeScript, or TSX. Node's `--input-type` names both at once; the command line keeps Node's spelling, and the Rust API keeps them apart because they vary independently.
- Its **evaluated value** is what it produced: a script's completion value, or a module's namespace.

### Command line

```text
bento run [PERMISSIONS] [--watch] <script-path> [args...]
bento run [PERMISSIONS] [--input-type=TYPE] -e <code> [args...]
bento run [PERMISSIONS] [--input-type=TYPE] -p <code> [args...]
bento run [PERMISSIONS] [--input-type=TYPE] - [args...]
```

`PERMISSIONS` are proposal.0003's `--permission` and `--allow-*` flags.

- `-e CODE`, `--eval CODE`, and `--eval=CODE` evaluate `CODE`. `-p` and `--print` take code the same way and print its value, as [Printing the value](#printing-the-value) describes. Empty code evaluates nothing: `-e ''` exits 0, and `-p ''` prints `undefined`. A code string beginning with `-` is given as `--eval=-42`, as in Node.
- `-` in place of a script path reads the code from standard input, as [Standard input](#standard-input) describes. `-` always means standard input; a file named `-` is run as `./-`.
- With `-e` or `-p`, every positional word is an argument for the code. `bento run -e CODE server.ts` passes `server.ts` to the code and runs no file, as in Node.
- Bento's options go before the first argument. A `--` before the first argument ends them and is not passed on. From the first argument on, every word is passed unchanged, `--` included: under `bento run -e CODE a -- b`, `process.argv.slice(1)` is `['a', '--', 'b']`.
- `--input-type=TYPE` or `--input-type TYPE` selects the module system and syntax. It accepts Node's four values and Bento's two TSX values, as [Module system and syntax](#module-system-and-syntax) lists.
- These fail before anything runs, with status 1 and a message:
    - `-e` or `-p` without a value: `-e requires an argument` or `-p requires an argument`.
    - `-e` and `-p` together, naming both.
    - Neither `-e`, `-p`, a script path, nor `-`: Bento's existing missing-target usage error.
    - An `--input-type` value outside the six, listing the six.
    - `--input-type` with a script path: `--input-type can only be used with string input via --eval, --print, or STDIN`.
    - `--watch` with `-e`, `-p`, or `-`: `--watch restarts a script when its file changes, so it cannot run --eval, --print, or standard input`.
    - Code that is not valid UTF-8 in `-e` or `-p`, as for any argument.
- `bento run --help` lists `-e`, `-p`, `-`, and `--input-type`.

### Module system and syntax

| `--input-type`        | Module system | Syntax     |
| --------------------- | ------------- | ---------- |
| not given             | detected      | detected   |
| `module`              | ES module     | JavaScript |
| `commonjs`            | CommonJS      | JavaScript |
| `module-typescript`   | ES module     | TypeScript |
| `commonjs-typescript` | CommonJS      | TypeScript |
| `module-tsx`          | ES module     | TSX        |
| `commonjs-tsx`        | CommonJS      | TSX        |

When the syntax is detected, the code is JavaScript if it parses as JavaScript, and otherwise TypeScript if it parses as TypeScript. It parses under the module system when that is given, and as either a script or a module when that is detected too. Valid JavaScript is never reread as TypeScript, so `a < b > (c)` compares. TSX is never detected. Code that parses as neither fails before running with the JavaScript parser's error at its position, followed by the TypeScript parser's message.

When the module system is detected, the code is an ES module if it contains module syntax, and CommonJS otherwise. Module syntax is an `import` or `export` declaration of any kind, type-only declarations included; `import.meta`; or `await` outside any function. An `import()` expression is not module syntax, nor is a declaration named `require`. Neither a `package.json` `type` field nor any file extension plays a part.

- JavaScript is evaluated as written. It may not contain JSX.
- TypeScript is transformed exactly as a `.mts` file (ES module) or `.cts` file (CommonJS) in the same project would be, with the project's `tsconfig.json` settings. It may not contain JSX.
- TSX is transformed as a `.tsx` file would be. JSX compiles according to the project's `jsx` settings; without a usable one, code containing JSX fails as a `.tsx` file does, naming the setting to add.
- The inline source is always a project source for these settings, never a dependency.
- Module syntax is kept as written. An `import` declaration in CommonJS input remains and fails as Node's does, with `Cannot use import statement outside a module`.

### What inline code sees

|  | CommonJS | ES module |
| --- | --- | --- |
| Evaluation | A classic script in the global scope, as Node evaluates string input: top-level `var` and function declarations become globals, and `this` is `globalThis` | An ES module, which may use top-level `await` |
| `module`, `exports`, `require` | Globals while it runs. `module.id` is the name, `module.filename` is `<directory>/<name>`, and `module.paths` lists `node_modules` from the directory upward | `require` is undefined; `createRequire(import.meta.url)` works |
| `__filename`, `__dirname` | The name, such as `[eval]`, and `.` | Undefined |
| `require.main` | `undefined` | Not applicable |
| `import.meta.url` | Not applicable | The `file:` URL of `<directory>/<name>` |
| `import.meta.filename`, `dirname` | Not applicable | `<directory>/<name>` and `<directory>` |
| `import.meta.main` | Not applicable | `true` |
| Stack frames | `[eval]:<line>:<column>` | The `file:` URL with line and column |

In both module systems:

- Relative specifiers, `import()`, `require`, `require.resolve`, `import.meta.resolve`, `createRequire`, and worker script URLs resolve from the directory. Packages resolve from `node_modules` in the directory and each of its ancestors, and package `imports` of the package scope enclosing the directory apply, as for a file in that directory.
- The CLI places the code in the invocation directory, named `[eval]` for `-e` and `-p` and `[stdin]` for `-`.
- `process.argv` is `[process.execPath, ...arguments]` for `-e` and `-p`, and `[process.execPath, '-', ...arguments]` for `-`. `process.cwd()` is the invocation directory.
- For `-e` and `-p`, every built-in module that can be required without `node:` and whose name has no `/` and no leading `_` is also a global of that name, in both module systems. It loads on first access, and assigning to the global replaces it. Code from standard input gets no built-in globals.
- When `-e` or `-p` code is CommonJS and its text contains the word `crypto`, the name `crypto` in it refers to `node:crypto`. `globalThis.crypto` stays Web Crypto, and the code's own declarations keep their usual meaning, including a top-level `var` and a `crypto` of its own. In ES module code, `crypto` is Web Crypto.
- Execution, permissions, the environment, signals, and lifecycle are those of a script run, including workers and subprocesses the code starts.

### Printing the value

- With `-p`, once the code has been evaluated, Bento adds a `beforeExit` listener that runs once and an `exit` listener, as Node does. Whichever runs first logs the completion value with the guest's `console.log` and removes the other. The value is printed as it is at that moment, unawaited: a promise is shown as `console.log` shows a promise.
- Nothing is printed when evaluation throws, or when the code exits before its evaluation finishes.
- Printing changes neither the exit status nor anything else about the run.
- When the module system, given or detected, is ES module, `-p` fails before evaluating anything with `--print cannot be used with ESM input`, Node's message for `ERR_EVAL_ESM_CANNOT_PRINT`, and status 1.

### Standard input

- With `-`, Bento reads standard input to its end before starting the instance and decodes it as UTF-8, replacing invalid sequences with U+FFFD. When standard input is a terminal, Bento reads until end of input (Ctrl-D); there is no REPL.
- Empty input runs nothing and exits 0.
- A failure to read fails the launch with `cannot read the code from standard input: <reason>` and status 1.
- The code's `process.stdin` is already at its end. With `-e` and `-p`, Bento leaves standard input unread for the code.

### Project, configuration, and environment

- An inline launch's project is found from the invocation directory: the nearest directory upward from it, itself included, that holds a `package.json`. A malformed `package.json` there fails the launch, as it fails a script's.
- When no directory holds one, the invocation directory serves as the project root.
- The project root plays the roles it plays for a script: its `tsconfig.json`, when present, supplies the compiler settings, and TypeScript's defaults apply otherwise; its `.env`, `.env.<mode>`, and `.env.local` are loaded as for a `bento run` without `--watch`; configuration diagnostics are printed as for a script.
- The code's directory is always the invocation directory, not the project root, as in Node: run from `app/src`, `import "./util.ts"` loads `app/src/util.ts`.

### Permissions

- Inline source needs no read permission. In a restricted launch, `-e`, `-p`, and `-` evaluate without any `--allow-fs-read`, and no path is implied readable: not the directory, the invocation directory, or the project.
- Everything the code loads from disk is checked like any other module load. Under `--permission` without a covering `--allow-fs-read`, `import "./local.ts"` fails with `ERR_ACCESS_DENIED`, permission `FileSystemRead`, resource the absolute path of `local.ts`, and requiring an installed package is denied reading its `package.json`. Built-in modules load without permission.
- Reading standard input is the CLI's act as host, not the guest's, and no permission governs it.
- If this proposal is implemented before proposal.0003, the same rule applies to the current grants: inline source needs no `--allow-read`.

### Failures and exit status

- Uncaught exceptions, permission denials, exit codes, signals, the shutdown deadline, and incomplete disposal behave as they do for a script, with the same statuses.
- A syntax error or transform failure in the inline source stops the launch before any of its code runs. It is `ErrorKind::Transform` when TypeScript or TSX syntax was given, and `ErrorKind::Startup` otherwise, including detected syntax that parses as neither language.
- Reports name the inline source by its name. The CLI's code frame quotes the line of the inline source, labelled `[eval]:<line>:<column>` or `[stdin]:<line>:<column>`, and shows the TypeScript line for TypeScript input. A runtime error message begins with the name where a script's begins with its path.

### Where Bento differs from Node

These are the only intended differences from Node 26.13.1:

- **TypeScript is transformed, not only stripped.** Enums, namespaces, parameter properties, and legacy decorators run, and `tsconfig.json` settings apply, as for Bento's TypeScript files. Node rejects them with `ERR_UNSUPPORTED_TYPESCRIPT_SYNTAX`.
- **`--input-type` also accepts `module-tsx` and `commonjs-tsx`.** Node supports no JSX.
- **Module input keeps its name.** Bento names ES module input `[eval]` or `[stdin]`, like CommonJS input; Node names every module input `[eval1]`.
- **`import.meta.main` is always `true` for module input.** Node reports `false` when it detected the module system, contrary to its [documented meaning](https://github.com/nodejs/node/blob/v26.13.1/doc/api/esm.md): "`true` when the current module is the entry point of the current process".
- **Syntax error messages are the parser's own** and may be worded differently from V8's; the position is the same.
- **`crypto` in `-e` code changes no scoping.** Node wraps CommonJS `-e` code that mentions `crypto` in a function, so its top-level `var` declarations stop being globals, and detected module code that mentions `crypto` fails to parse. Bento binds `crypto` to `node:crypto` in CommonJS code without changing its scoping, and runs module code that mentions `crypto` with Web Crypto.
- **`-e` and `-p` together are rejected.** Node runs the last code given and prints when `-p` appeared.
- **`-p` requires code.** Node's `-p` without code prints the result of standard input.
- **Standard input needs `-`.** `bento run` without a target is a usage error even when standard input is not a terminal; Node reads it.
- **`-` on a terminal reads to end of input.** Node starts its REPL.
- **`--watch` with `-` is rejected.** Node watches a file named `-`.
- **Usage errors exit with status 1,** as all of Bento's do. Node exits with 9.
- **The project's environment files load,** as for every Bento launch. Node loads none without `--env-file`.
- **`process.execArgv` does not list `-e` and its code.** It holds none of Bento's own options; accepting Node's command line is [#260](https://github.com/bento-js/bento/issues/260).

### Rust embedding API

```rust
/// Program text a host gives an instance to run in place of an entry file.
#[derive(Clone, Debug)]
pub struct InlineSource {
    /// The text to evaluate.
    pub code: String,
    /// Where the source is placed: relative specifiers and installed packages resolve from
    /// here. Taken relative to `RuntimeConfig::cwd` unless absolute, and canonicalized.
    pub directory: PathBuf,
    /// The file name the source has inside `directory`. No file of this name is read.
    pub name: String,
    /// `None` detects the module system.
    pub module_system: Option<ModuleSystem>,
    /// `None` detects the syntax: JavaScript, then TypeScript.
    pub syntax: Option<Syntax>,
    /// Defines Node's built-in modules as globals, and `crypto` as `node:crypto` in
    /// CommonJS code that mentions it, as `node --eval` does.
    pub builtin_globals: bool,
    /// Logs the completion value as `node --print` does.
    pub print: bool,
}

impl InlineSource {
    /// `code` named `[eval]` in `directory`, with its module system and syntax detected,
    /// without built-in globals, and without printing.
    pub fn new(code: impl Into<String>, directory: impl Into<PathBuf>) -> Self;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModuleSystem { EsModule, CommonJs }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Syntax { JavaScript, TypeScript, Tsx }

/// Launches `source` on a new runtime thread, as `start` launches an entry file.
pub fn start_source(config: RuntimeConfig, source: InlineSource) -> Result<RunningRuntime, StartError>;

impl RunningRuntime {
    /// Like `wait`, and also reports the value an inline source evaluated to.
    pub fn wait_for_evaluation(self) -> Evaluation;
}

/// How a run ended, with what its inline source evaluated to.
#[derive(Debug)]
pub struct Evaluation {
    pub outcome: RunOutcome,
    pub value: Option<EvaluatedValue>,
}

/// A script's completion value or a module's namespace, as the host receives it.
#[derive(Clone, Debug, PartialEq)]
pub enum EvaluatedValue {
    /// The value as `JSON.stringify` represents it.
    Json(serde_json::Value),
    /// The value has no JSON representation; says why.
    NotJson { reason: String },
}
```

- `start_source` canonicalizes `config.project_root`, `config.cwd`, and `source.directory`, which must each exist; the directory must be a directory. It applies the module system, syntax, and transform rules above and returns before any guest code runs.
- `start_source` returns `ErrorKind::Startup` when the directory cannot be resolved or is not a directory; when `name` is empty, `.` or `..`, or contains `/` or NUL; when `print` is set and the module system is ES module, with `--print cannot be used with ESM input`; when the source has a JavaScript syntax error or its detected syntax parses as neither language; and for the host-module and thread failures `start` reports. It returns `ErrorKind::Transform` when `tsconfig.json` cannot be loaded, as `start` does, and when TypeScript or TSX source cannot be parsed or transformed. Each failure placed in the source has a `SourceLocation` whose path is `<directory>/<name>`.
- The instance sees the inline source as [What inline code sees](#what-inline-code-sees) describes, with `directory` and `name` in place of the CLI's. `process.argv` is `[process.execPath, ...config.argv]`, with no entry element. `builtin_globals` and `print` have the meaning they have for `-e` and `-p`.
- Inline source needs no read permission in any `Permissions` configuration, and nothing is implied readable.
- `diagnostics()`, `handle()`, `is_finished()`, and `wait()` behave as for `start`. `watch_inputs()` never includes the inline source; modules it loads are recorded as loaded by a project source.
- `wait_for_evaluation()` returns the outcome `wait()` would return, and a value:
    - `None` for an instance started with `start`, and when evaluation did not finish: the source threw, its module evaluation rejected, the instance failed before evaluating it, or the guest exited or was shut down during evaluation.
    - Otherwise the value is captured when the event loop first runs out of work, after the guest's `beforeExit` listeners, or when the guest exits, after its `exit` listeners, whichever comes first: the moment `print` prints. A run that reaches neither moment, such as one interrupted at the shutdown deadline, reports `None`.
    - For a script, the value is its completion value. When that is a promise, it is the promise's fulfillment value if the promise has fulfilled by then; a pending or rejected promise has no JSON representation. For a module, the value is its namespace object.
    - The value is converted inside the guest with `JSON.stringify` and parsed into `serde_json::Value`. When `JSON.stringify` returns `undefined`, because the value is `undefined`, a function, or a symbol, or throws, as for a `BigInt`, a cycle, or a throwing `toJSON`, the result is `NotJson` with a reason: `undefined`, `function`, `symbol`, `pending promise`, `rejected promise`, or the thrown error's message. A conversion failure never fails the run.
    - Capturing adds no listener, global, or other state the guest can observe. Converting does run the guest's `toJSON` methods.
- `RuntimeConfig::argv`'s documentation says it follows the executable and, for an entry file, the entry path.
- `crates/bento_runtime/examples/evaluate.rs` evaluates its first argument with `start_source` in the current directory and prints the outcome and value, for previewing and as a worked example.

### Documentation

[Running with Bento](../guides/running-with-bento.md), [Embedding Bento](../guides/embedding-bento.md), [TypeScript and JSX](../guides/typescript-and-jsx.md), and [Node Compatibility](../guides/node-compatibility.md) are extended from this proposal: running code without a file, the module system and syntax table, inline TypeScript and TSX, the Rust API, and the differences from Node.

## Compatibility

This change adds behavior; existing commands and Rust callers keep working.

- `bento run -` now reads standard input. A file named `-` that `bento run -` used to run must be given as `./-`.
- `bento run <script-path>` is unchanged, including its requirement of a `package.json` above the script.
- `start`, `RuntimeConfig`, `RunOutcome`, and `wait()` are unchanged. `start_source`, `InlineSource`, `ModuleSystem`, `Syntax`, `wait_for_evaluation`, `Evaluation`, and `EvaluatedValue` are new.
- If proposal.0003 has not landed, the permission rule applies to the current grants.

## Implications on adoption

No implications on adoption. Nothing is installed or configured, and the change can be reverted without migration.

## Scope

- `-e`/`--eval`, `-p`/`--print`, `-`, and `--input-type` on `bento run`, with the errors above.
- Syntax and module-system detection with Oxc, and the TypeScript and TSX transform of inline source.
- Node's string-input semantics: the script and module evaluation forms, identity values, `process.argv`, built-in globals, and printing.
- Project discovery from the invocation directory, with the invocation directory as project root when no `package.json` is found.
- `start_source`, `InlineSource`, `wait_for_evaluation`, and `EvaluatedValue` in `bento_runtime`, and the `evaluate` example.
- Differential tests against Node 26.13.1 for every shared behavior, tests for each listed difference, and tests of the Rust API's values and failures.
- The guide updates above, and a changeset.

### Out of scope

- **Persistent sessions and REPL evaluation.** Keeping state between evaluations, and reporting the last expression's value from code that uses top-level `await`, belong to proposal.0012 ([PR #271](https://github.com/bento-js/bento/pull/271)). This proposal evaluates one source per instance.
- **Node's whole command line** ([#260](https://github.com/bento-js/bento/issues/260)): `node` as `argv0`, `NODE_OPTIONS`, `--import`, `-i`, `--check`, `process.execArgv`, and Node's handling of `-e` with `-p` and of `-p` without code. A Node-compatible command line would map `node -e` onto the same `start_source` this proposal adds.
- **Script files outside any project.** `bento run <script-path>` keeps requiring a `package.json`, pending the open question below.
- **The agent CLI registry and `--dry-run`** of proposal.0005 ([PR #269](https://github.com/bento-js/bento/pull/269)). The new options are ordinary `bento run` options for its registry to describe; how `--dry-run` previews an inline launch is that proposal's decision.
- **Structured output** of proposal.0007: per-instance standard-stream capture, `--output json` receipts, structured errors, and events. A receipt for an inline launch would identify its entry by name and directory, and may carry the evaluated value this proposal defines; that is proposal.0007's decision.
- **Timeouts and heap limits** of proposal.0009 ([PR #272](https://github.com/bento-js/bento/pull/272)), which apply to inline launches as to any other.
- **`node:vfs`** of proposal.0010 ([PR #270](https://github.com/bento-js/bento/pull/270)), and any virtual module beyond the one inline entry.
- **Asynchronous `wait` and `shutdown`** of proposal.0011. An asynchronous counterpart of `wait_for_evaluation` belongs there.
- **Evaluated values for entry files.** `wait_for_evaluation` reports `None` for `start`.
- **Worker string input**, such as `new Worker(code, { eval: true })`, which is unchanged.

## Preview

- Artifact: the macOS arm64 `bento` archive that CI publishes for this pull request (`bento-macos27-arm64-prompt`), and `cargo run -p bento_runtime --example evaluate` built from the branch.
- Reason: the change is the CLI's and the library's behavior. The reviewer runs the same `-e`, `-p`, `-`, `--input-type`, and `--permission` commands under `bento` and Node 26.13.1 side by side, inside the guest-book fixture and in a directory with no `package.json`, and runs the example to see a JSON value reach Rust. The example stays as a worked example; nothing needs cleaning up.

## Policies and decisions checked

- policy.0001 — Syntax and module-system detection and the TypeScript and TSX transform use Oxc through `bento_compiler`, as files do. Node's strip-only TypeScript (Amaro, built on SWC) is not adopted, and no Deno or SWC crate is added.
- `decisions/` holds no decisions.
- `VISION.md` — The capability is a library API that the CLI composes ("embedding is the default"), inline TypeScript uses the same transform and settings as files ("treat TypeScript as the working language"), flags spelled like Node's behave like Node's or their differences are listed ("compatibility is behavior"), and the host, not the guest, supplies the source ("make host authority explicit"). The vision's revision history records the change when it ships.
- proposal.0003 — Uses its permission vocabulary and restricted-launch rules. Its implied read covers only an entry file, and an inline launch has none, so nothing is implied.

## Future directions

- Running script files outside any project with the same rule, if the open question below is answered no for this proposal.
- Reporting the evaluated value of entry files, as the namespace of the entry module.
- Host-chosen virtual modules beyond the entry, for example a host-supplied module graph.
- A JSX-only syntax, if TSX's rejection of TypeScript-incompatible JavaScript ever matters in practice.

## Alternatives considered

- **Write the code to a temporary file and run that.** Under `--permission` the temporary path needs a read grant, interrupted runs leave files behind, a project's watcher restarts, and `__filename`, `process.argv`, and `-p` cannot match Node.
- **A CLI-only feature.** The vision makes CLI-only capability a limitation to correct, and embedders are half the motivation.
- **A separate `bento eval` subcommand, as Deno has.** #260 aims at Node's command line, where these are options of the command that runs code.
- **Evaluate everything as an ES module, as Deno does.** Node's `-e` is CommonJS unless the code has module syntax, `-p` needs a completion value, and `require` and `__filename` would disappear.
- **Detect TSX after TypeScript fails.** Error messages would come from whichever parser ran last, a few constructs mean different things in TypeScript and TSX, and Node has no such fallback.
- **Deno's `--ext` instead of extending `--input-type`.** Node's flag already names the input's kind; two flags for one choice would conflict.
- **One `InputType` enum in Rust, mirroring Node's flag.** Module system and syntax vary independently, and a host may give one and detect the other.
- **No value for module input, as Node.** A host could then get a value only from CommonJS code without top-level `await`. The namespace is what `import()` of the module resolves to, so it is ESM's own notion of a module's result.
- **The module's default export as its value.** That invents a convention the namespace already covers.
- **Report the last expression's value even with top-level `await`.** That is REPL evaluation, which rewrites the code or compiles it in a special mode; it belongs with the sessions of proposal.0012.
- **Await a promise completion value by attaching a handler.** Handling the rejection would change Node's unhandled-rejection behavior; reading the promise's state when the run ends changes nothing.
- **Structured cloning, or `util.inspect` text, as the value.** Host modules already exchange JSON, guests control JSON through `toJSON`, and the inspected text is what `print` already shows.
- **No project, rather than the invocation directory, when no `package.json` is found.** The CLI would then ignore a `tsconfig.json` or `.env` in the directory it runs in, and the library would need a new way to say "no project root". One rule, that the invocation directory serves as the root, covers both.
- **Read standard input whenever no target is given, as Node does.** Explicit `-` keeps `bento run` without a target a usage error, so a command missing its target cannot hang waiting for input.

## Open questions

- [NEEDS CLARIFICATION: Should `bento run <script-path>` also run a script with no `package.json` above it, using the same rule that the script's own directory then serves as the project root? Node runs files anywhere, and without it `bento run - < /tmp/a.ts` works where `bento run /tmp/a.ts` fails. It changes an existing command's behavior, so this proposal leaves it out unless you want it in.]
