---
id: proposal.0005
title: Agent-First CLI Registry, Discovery, and Dry-Run
authors: [Mark]
status: draft
pull-request: https://github.com/bento-js/bento/pull/269
issues: []
supersedes: []
---

# Agent-First CLI Registry, Discovery, and Dry-Run

## Summary

Make the `bento` executable explain itself to coding agents, and define Bento's one CLI wire contract. A Rust registry generates the parser, help, `bento cli search`, `bento schema`, and the CLI reference. `bento docs` serves built-in guides. `bento context show` and `bento run --dry-run` preview a launch without running it.

## Motivation

A coding agent asked to run a project with Bento has nothing about Bento in its training data, and the `bento` executable tells it almost nothing. `bento --help` prints one sentence and two commands:

```text
Run TypeScript applications and project tasks

Usage: bento <COMMAND>

Commands:
  run      Run a script, or a task from the `tasks` field of package.json
  install  Install dependencies and devDependencies with the project's pnpm
  help     Print this message or the help of the given subcommand(s)
```

Everything else lives in guides on the documentation site: which flags exist and what they accept, how `.env` files layer, which `tsconfig.json` options apply. The site tracks `main`, not the binary the agent is holding, and the command line is changing. Proposal 0003 ([PR #259](https://github.com/bento-js/bento/pull/259)) replaces every permission flag with Node's, and proposal 0004 ([PR #268](https://github.com/bento-js/bento/pull/268)) removes task targets. An agent that learned from older material types `--allow-read`, a flag proposal 0003 removes.

This costs agents in three ways:

- **Discovery by guessing.** The agent guesses flags from Node or Deno, reads a 28 KB guide, or reads the Rust source. Nothing on the command line answers "which command does this" or "what does this flag accept".
- **No preview.** The only way to learn which project root, environment files, `tsconfig.json` chain, and permissions a launch will use is to run it, and running executes guest code: servers bind ports and migrations write databases. Under proposal 0003 a bare `bento run` is unrestricted, so trying a command to see what happens runs the code with the user's full authority.
- **Prose output.** Bento's own failures are `error:` lines or source excerpts, so an agent tells a missing script from a malformed `tsconfig.json` only by parsing English. Launches print `tsconfig.json` diagnostics as `warning:` and `error:` lines even when the run succeeds ([#265](https://github.com/bento-js/bento/issues/265)), which an agent reads as failure.

Today's workaround is to read the guides from a checkout, read the source, or run the script and watch. None works from an installed binary alone, and the last one has side effects.

## Domain grounding

### Established context

- Cloudflare built its `cf` CLI for agents that have never used it. Its launch post says Wrangler's years of documentation were absorbed into model training, so changing Wrangler "goes against learned behavior"; `cf` instead makes JSON the default output and tells the agent about `cf cli search` the first time it runs `--help`. ([Introducing cf](https://blog.cloudflare.com/cloudflare-cf-cli-launch/))
- `cf`'s agent documentation: commands "return JSON and describe their own inputs". `cf cli search` runs locally without credentials and returns up to five matches, best first. `cf schema` describes a command. `--dry-run` previews a change. Results go to standard output as JSON, and messages and errors go to standard error, so when standard output is not a terminal it holds only the result. An unknown command lists the closest matches. `cf docs search` searches the live documentation site and needs a network connection. ([Use cf with coding agents](https://developers.cloudflare.com/cf/agents/))
- The Command Line Interface Guidelines: primary and machine-readable output goes to stdout, messages and errors to stderr; exit zero on success and non-zero on failure; decide whether a person is reading by checking whether each stream is a terminal, separately. ([clig.dev](https://clig.dev/))
- JSON Lines, also called NDJSON, puts one UTF-8 JSON value on each line and ends each line with `\n`, so a consumer can process a stream line by line. ([JSON Lines](https://jsonlines.org/))
- A runtime leaves the program's output to the program. Node writes a script's output to its own stdout and stderr and exits with the script's exit code. ([Node `process`](https://nodejs.org/api/process.html))
- A preview is only as faithful as the validation it shares with the real operation. Kubernetes added server-side dry run because `kubectl apply --dry-run` ran locally, "doesn't have server validation and doesn't go through validating admission controllers", and so could approve requests the server would reject. ([APIServer dry-run and kubectl diff](https://kubernetes.io/blog/2019/01/14/apiserver-dry-run-and-kubectl-diff/))
- Established tools validate or inspect without running. `node --check` parses a script without executing it ([Node command-line API](https://nodejs.org/api/cli.html)). `tsc --showConfig` prints the final configuration after `extends` instead of building ([tsc options](https://www.typescriptlang.org/docs/handbook/compiler-options.html)).

Observed in this repository on `main` (`d9b00e0`):

- `bento --help` prints the text above. `bento` alone prints it and exits with status 1. `bento frobnicate` prints clap's `error: unrecognized subcommand` text and exits with status 1. No command produces machine-readable output.
- `crates/bento_cli/src/arguments.rs` and `grant_flags.rs` declare the command line with clap's derive macros. Their doc comments are the only summaries; the guides restate them by hand.
- `bento_workspace::LaunchEnvironment` records which variables came from environment files (`file_keys`) but not from which file, and an inherited variable that overrides a file value replaces it.
- `bento_compiler::CompilerSettings::load` returns the execution settings and the diagnostics for `tsconfig.json`. `oxc_resolver` 11.24.3 returns merged compiler options and the root file's raw `extends` field, not the resolved chain. `watch/configuration.rs` already records the files that `tsconfig.json` resolution reads, so watch mode restarts when one changes.
- An environment file error names the file and line and never prints the value ([Tasks and Environment](../guides/tasks-and-environment.md)).

### Relevant constraints and principles

- The vision says "embedding is the default" and "a feature available only through a CLI is a limitation to correct". What a preview computes belongs in the libraries; the CLI composes it.
- "Compatibility is behavior, not a list of names." `bento run server.ts | jq` must see only the program's output, as under Node, so running a program cannot become a JSON command.
- "The host decides what code may do." A preview reports the permissions a launch would hold, and where they came from.
- [policy.0001](../policies/0001-prefer-voidzero-crates.md): `tsconfig.json` loading and TypeScript transformation go through Oxc and `oxc_resolver`. A dry run reuses `bento_compiler` rather than adding a parser.
- Bento options go before the script; every word after it belongs to the script (proposal 0004). `bento run server.ts --dry-run` therefore runs `server.ts` with `--dry-run` in its arguments, as `node server.js --dry-run` would.
- Environment files hold secrets. No output may contain a variable's value.

### Quality bar

- An agent that has never seen Bento gets from `bento --help` to a correct, previewed `bento run` command using only the installed binary.
- Nothing advertised can drift. Every flag, default, accepted value, and example in help, `bento schema`, `bento cli search`, and the CLI reference comes from one declaration, and tests fail when the parser or the runtime defaults disagree with it.
- A structured invocation writes only what its format promises: one JSON document with `json`, one per line with `ndjson`, or on failure one failure document on stderr. Nothing else.
- Every command, including those later proposals add, reports through the same `--output` formats, failure document, and error codes.
- A launch by `bento run` or `bento install` without `--dry-run` writes exactly the bytes it writes today and exits with the same status. Only help and usage-error text change.
- A dry run executes no guest code, starts no JavaScript engine, subprocess, or network connection, and writes nothing. Every failure it reports is one the real launch would hit before guest code runs, and it states what it does not check.
- No environment variable's value appears in any output, except the environment mode Bento derives from `NODE_ENV`.

### Remaining uncertainty

- `oxc_resolver` does not expose the resolved `extends` chain. Bento derives it either by recording the files the resolver reads, as watch mode does, or by resolving each `extends` entry itself. The result described below does not depend on which.
- Probing the Foundation Models model's availability in `bento context show` initializes Apple's framework on a blocking thread. Its latency has not been measured.
- Proposals 0003 and 0004 are not merged and may change in review. The registry declares `bento run` as they finally define it.
- Whether word-and-stem ranking, which works for `mcp`'s vocabulary, ranks Bento's commands correctly is settled by the ranking table in [`bento cli search`](#bento-cli-search), which the tests enforce.

## Existing baseline

**Bento's CLI.** `main.rs` parses with clap's derive API and dispatches `run` and `install`. `Launch::prepare` reads the inherited environment, takes `BENTO_GRANT_CEILING`, resolves the grant flags against it, and loads the environment files; after proposal 0004 it does this before touching the script. `bento_runtime::start` loads `tsconfig.json` and transforms modules as they load. `CliError::report` prints a source excerpt or one `error:` line.

Worth keeping: the parse semantics of `bento run` (options before the script, every later word passed through verbatim); the order in which a launch validates its flags, ceiling, project, and environment; the human error reports with source excerpts for running programs; the environment-file redaction; `LaunchEnvironment`'s layering; `CompilerSettings::load`'s diagnostics; and watch mode's recording of configuration files.

Incidental: argument declarations whose summaries are restated by hand in the guides; clap's `help` subcommand; and `bento` alone exiting with status 1.

**Mark's `mcp` CLI** (`~/Developer/CLIs/mcp/crates/mcp`) implements this pattern for MCP servers. `registry.rs` declares each operation once as a `CommandSpec`: path, summary, description, arguments, effect, requirements, dry-run support, idempotence, error codes, examples, synonyms, and an output schema. `parser.rs` builds the clap parser from the registry, and `reference.rs` renders `docs/cli.md`. `commands/cli.rs`, `schema.rs`, `docs.rs`, and `context.rs` implement discovery. `tests/cli/registry.rs` fails when the parser's commands, arguments, or defaults differ from the registry, or when an example does not parse, run, and match its schema. Results are JSON on stdout, compact when piped and indented on a terminal, with `--output text` for people. A failure is one `{ "error": { code, message, retryable, details, suggestions } }` document on stderr. Documentation is compiled in with `include_str!`.

Worth keeping: all of that. Different for Bento:

- Every `mcp` command owns stdout. Bento's main command must leave stdout to the program it runs, so Bento needs two output contracts.
- `mcp` gives each error code its own exit status. Bento's usage errors exit with 1 (proposal 0003), and a run's exit status belongs to the script.
- `mcp`'s `--output` is global, accepted anywhere. In `bento run`, a word after the script belongs to the script.
- `mcp` pages search results with cursors over a large catalogue of tools. Bento has seven commands.

## Proposed solution

Bento's command line becomes self-describing. Each command is declared once in Rust, and everything that describes the command line is generated from that declaration and tested against it. Commands that answer questions write JSON; commands that run programs leave the program's output alone, exactly as today. One wire contract covers both: `--output` means the same thing on every command, every failure has the same envelope, and every error code comes from one table.

```text
bento                                              → orientation: what Bento is, its commands, how to discover them
bento cli search "run a server with network access" → bento run, with --allow-net among its matching arguments
bento schema run                                   → every argument, default, effect, error code, exit status, and example of bento run
bento docs search "environment file precedence"    → the matching sections of the guides built into this binary
bento context show                                 → project root, package.json summary, tsconfig chain, .env names and layers, permissions
bento run --dry-run --permission --allow-net server.ts → what this launch would use, as JSON; nothing runs
bento run --permission --allow-net server.ts         → runs; the server's output passes through as it does today
```

An agent follows one loop: discover a command, inspect it, preview the launch, run it, and verify the result. A person gets the same answers with `--output text`.

## Detailed design

### Vocabulary

- A **command** is one operation the `bento` executable performs. Its **path** is the words after `bento` (`run`, `cli search`), and its **id** is the path joined with dots (`run`, `cli.search`).
- An **invocation** is one use of a command with its arguments.
- An invocation's **output format** is its `--output` value: `json`, `ndjson`, or `text`. Each command declares the formats it accepts and its default.
- An invocation's **output contract** is **structured** when Bento writes JSON documents for it, and **pass-through** when the program Bento runs owns stdout and stderr. [Deciding the contract](#deciding-the-contract) is the one rule that chooses.
- A pass-through invocation's **error format** is its `--error-format` value: whether Bento reports its own failures as human text or as failure documents.
- The **registry** is the set of command declarations compiled into `bento`.
- A **result** is what a structured invocation writes to stdout: one JSON document with `json`, or a stream of them with `ndjson`.
- A **failure document** is the JSON document `{ "error": body }` that reports one failure. Its `error` object is the **failure body**, and the body's `code` is an **error code** from the one table in [Error codes](#error-codes).
- A **guide** is a Markdown file from `guides/` compiled into the binary. A **section** is a heading and the text below it, up to the next heading of the same or a higher level.
- The **context** of a directory is what a `bento run` started there without flags would use: its project, configuration, environment files, and permissions.
- A **launch preview** is what one `bento run` command line would use, computed by `bento run --dry-run` without running anything.

### The wire contract

This section is Bento's whole command-line wire contract: the output formats, the two output contracts and the rule between them, the failure document, and the error codes. Every command reports through it, including the commands and options later proposals add; proposals 0006 ([PR #274](https://github.com/bento-js/bento/pull/274)) and 0007 ([PR #277](https://github.com/bento-js/bento/pull/277)) adopt it rather than restating it.

#### Output formats

`--output <format>` is one option with one meaning on every command: how Bento reports the command's result. Every command declares it, with the formats it accepts and its default. A format the command does not accept fails with `INVALID_ARGUMENT`. On `bento run`, like every Bento option, it goes before the script.

- `json`: the result is exactly one JSON document on stdout.
- `ndjson`: the result is a stream of JSON documents on stdout, one per line.
- `text`: the result is rendered for people. Text is not a contract and may change between versions. On a pass-through command, `text` means pass-through: the program Bento runs owns stdout and stderr.

The formats this proposal defines for each command:

| Command | Accepts | Default |
| --- | --- | --- |
| `bento cli search`, `bento schema`, `bento docs search`, `bento docs show`, `bento context show` | `json`, `text` | `json` |
| `bento run` | `text`; with `--dry-run`, `json` and `text` | `text` (pass-through); `json` with `--dry-run` |
| `bento install` | `text` | `text` (pass-through) |

- `bento run --dry-run --output text` renders the launch preview for people. A dry run never passes anything through.
- Proposal 0007 adds two results to `bento run` without `--dry-run`, under these rules: `--output json`, a receipt for the run, and `--output ndjson`, a stream of the run's events. `--output json` on `bento run` therefore always means a structured result instead of pass-through.
- Until proposal 0007 lifts it, `bento run --output json` without `--dry-run` fails with `INVALID_ARGUMENT` and the message `--output json requires --dry-run`. `--output ndjson` is not a format `bento run` accepts in this proposal, and a dry run never accepts it.

#### Structured invocations

- A JSON document is UTF-8. Keys are camelCase. Enumerated values are lowercase kebab-case strings. Paths are absolute, and canonical wherever Bento resolves them.
- Bento never colors or styles JSON. A control character inside a string, including the escape character that begins a terminal sequence, is written as a JSON escape such as `\u001b`, so a document never contains a raw control character.
- With `json`, stdout receives exactly one document followed by a newline: compact on one line when stdout is not a terminal, and indented with two spaces when it is.
- With `ndjson`, stdout receives one document per line: each compact, followed by a newline, and written whole, never interleaved with another, whether or not stdout is a terminal. A consumer can parse each line as it arrives.
- With `text`, stdout receives the rendering for people. Failures are still failure documents.
- On success, Bento writes nothing to stderr and exits with the status the command declares. Every structured result in this proposal exits with 0.
- A failure before the first document writes nothing to stdout, writes one failure document to stderr, and exits with status 1. A command that streams defines how a failure after its first document appears in its stream; proposal 0007's event stream reports it in the run's last event.
- Bento never prompts. Bento itself reads standard input only where a command declares it as an input, as the `stdin` field of an argument says; proposal 0008's `bento run -`, which reads the program from standard input, will be the first, and no command in this proposal reads it. A script's standard input passes through to the script, as in a pass-through run, so with proposal 0007's receipt, `bento run --output json transform.ts < data.json` gives the script `data.json`.
- When stdout's reader has gone away (`EPIPE`), Bento stops writing to stdout, ends the command as it declares for a reader that left (every command in this proposal simply ends), writes nothing to stderr, and exits with status 0: the reader chose to stop. Any other failure to write stdout is an `INTERNAL` failure.
- Adding a field, an enumerated value, a kind of stream document, or an error code is a compatible change; consumers ignore values they do not know. Removing or renaming any of them, or changing a field's type or meaning, is a breaking change and is recorded in a changeset. This rule covers results, failure documents, and the code table alike.

#### Pass-through invocations

- stdout and stderr belong to the script, or to pnpm. Bento's own messages that are not failures, such as `tsconfig.json` diagnostics and watch restarts, go to stderr as human text, as today. Exit statuses are unchanged.
- This proposal changes none of these bytes once a launch has parsed. Help and usage errors change as described below and in [Help](#help).
- `--error-format <format>`, declared on `bento run`, decides how Bento reports its own failures and usage errors on stderr:
    - `text`, the default: a human report, as today. A usage error's last line, in place of clap's `For more information, try '--help'.`, names the command's schema: for `bento run`, ``See `bento schema run` for its arguments.``
    - `json`: every failure report and usage error Bento writes is a failure document, compact on one line unless stderr is a terminal. That includes each failed load or reload under `--watch`, and a run whose work did not settle by the shutdown deadline. A run failure's document is written after its instance has ended, so it is the last thing Bento writes to stderr for that run. Messages that are not failures stay text. Exit statuses are unchanged.
- `--error-format` goes before the script. Bento reads it from the words before parsing, so a malformed command line is a failure document when `--error-format json` or `--error-format=json` appears before the script.
- On a structured invocation every failure is a failure document: `--error-format json` is accepted and changes nothing, and `--error-format text` fails with `INVALID_ARGUMENT`.
- `bento install` does not declare `--error-format`; its failure reports stay text.
- A run's failures after its instance starts, such as an uncaught exception, an unresolvable import, a failed startup, or incomplete disposal, take codes proposals 0006 and 0007 add. `--error-format` is therefore delivered with proposal 0007's codes: this proposal defines it, and proposal 0007's implementation adds it to `bento run`'s registry entry, so the registry never advertises an option that cannot report every failure.

#### Deciding the contract

This is the one rule every command uses. Later proposals rely on it rather than restating it. Bento applies it to the words before parsing, so a malformed command line is reported in its own contract:

- An invocation is **structured** when its words select a command whose default output format is `json` or `ndjson`, or when its options include `--dry-run` or an `--output` value other than `text`; for `bento run`, those options count only before the script. `bento run --dry-run …` and `bento run --output json …` are structured, and so is `bento install --output json`, which fails because `bento install` accepts only `text`.
- Words that select no command, such as `bento frobnicate` or `bento docs`, are structured.
- Every other invocation is **pass-through**. It reports its own failures as failure documents when `--error-format json` appears before the script.
- "Before the script" means before the first word that is neither an option nor an option's value. `--output json` and `--output=json` are read alike. After the script, every word belongs to the script.
- `--help` and `--version` print text in every contract.

#### Failure documents

```json
{
    "error": {
        "code": "SYNTAX_ERROR",
        "message": "/Users/you/app/server.ts:12:7: Expected `;` but found `:`",
        "retryable": false,
        "details": {},
        "suggestions": [],
        "location": { "path": "/Users/you/app/server.ts", "line": 12, "column": 7 }
    }
}
```

- `{ "error": body }` is the only envelope Bento uses for a failure, in every command and both contracts. Anywhere else a failure appears, such as the `error` of a proposal 0007 receipt, it is the same body.
- A failure document goes to stderr, indented when stderr is a terminal and compact otherwise. It stays JSON whatever `--output` asks for, so stderr always parses.
- `code` is an error code; consumers branch on it. `message` is one sentence for a person and not a contract. `retryable` says whether running the same command again, unchanged, can succeed. `details` holds the facts the code's row lists. `suggestions` lists commands or changes that recover, most useful first, and may be empty.
- `location` is `{ "path", "line", "column" }`, one-based, or null. Every failure that points at a place in a file sets it: a `package.json` or `tsconfig.json` that is not valid JSON, a malformed environment file, a syntax error, or syntax that cannot be transformed. `column` is null when Bento knows only the line, as for an environment file. `details` never carries `line` or `column`.
- Other proposals add fields to the body as compatible additions under the rule in [Structured invocations](#structured-invocations). Proposal 0007 adds `exception`, the JavaScript value an uncaught failure carried.
- No environment variable's value appears in `message`, `details`, `suggestions`, or `location`, except the environment mode.

#### Error codes

There is one code table, and it is defined here. The registry holds it as one `ErrorCode` enum: every failure Bento reports is built from a variant, so no command can report a code outside the table, and the CLI reference's code table and `bento schema`'s meanings are generated from the enum. A command's registry entry lists the codes it can report. One kind of failure has one code in every command, so a dry run and a real run report the same code for the same failure.

| Code | Meaning | `details` |
| --- | --- | --- |
| `INVALID_ARGUMENT` | The command line is malformed or inconsistent: an unknown command or option, a missing or extra value, a value outside its accepted values or bounds, a format the command does not accept, or an argument without one it requires. | `command` (such as `bento run`), and `argument` when one argument is at fault. For words that select no command, `commands`: every command path. |
| `NOT_FOUND` | A named command, guide, section, or script does not exist, or no `package.json` encloses the script. | One of `command`, `guide` (with `guides`), `section` (with `sections`), or `path`. For a missing project, `missing: "package.json"` and `searchedFrom`. |
| `PRECONDITION_FAILED` | A file Bento must read exists but cannot be read, the current directory cannot be determined, or something else the command needs from its environment is unavailable. | `path`, when a file is at fault. |
| `INVALID_CONFIGURATION` | Something Bento reads is malformed: `package.json`, `tsconfig.json` or a file it extends, an environment file, a `NODE_ENV` containing `/`, `BENTO_PERMISSION_CEILING`, or an inherited variable that is not UTF-8. | `path`, the file at fault, or `variable`, a name. |
| `PERMISSION_DENIED` | A permission flag asks for more than the inherited ceiling allows; the launch's permissions would not let it read its own script; or a running script was denied an operation and did not handle the denial. A denial is reported the same way in a dry run and in a real run. | For a flag beyond the ceiling, `argument` and `limit`. For a denied operation, Node's shape as proposal 0003 adopts it: `permission`, Node's name for the permission, such as `FileSystemRead`, and `resource`, the path or other resource when there is one. Proposal 0007 adds `nodeCode`, such as `ERR_ACCESS_DENIED`, for a denial during a run. |
| `SYNTAX_ERROR` | A module's source cannot be parsed. | None; `location` is the syntax error. |
| `TRANSFORM_FAILED` | A module parses but cannot be made executable: syntax the project's settings leave untransformed, such as JSX without a usable `jsx` setting or decorators without `experimentalDecorators`, or another error from the transformer. | None; `location` is the syntax. |
| `INTERNAL` | A defect in Bento, or stdout could not take the result. | `stream` when writing failed. |

- `retryable` is `false` for every code in this table: each failure repeats when the same command runs unchanged. A row that can succeed on retry says so.
- Every structured command can report `INVALID_ARGUMENT` and `INTERNAL`. An `INTERNAL` failure suggests reporting the defect, naming the Bento version and command id.
- For words that select no command, the message names the unknown word, `details.commands` lists every command path, and the suggestions are `bento cli search "<what you want to do>"` followed by up to three commands whose path is closest to the unknown words.
- A later proposal adds a row by giving its code, meaning, `retryable`, `details`, and suggestions. Adding a row is compatible; a row is never removed or renamed without a breaking change.

These rows are named here and defined by the proposals that add them:

| Code | Added by |
| --- | --- |
| `RESOLUTION_FAILED` | Proposals 0006 and 0007 both use it, and whichever is implemented first adds it: a module specifier could not be resolved. |
| `MODULE_LOAD_FAILED` | Proposal 0006. |
| `STARTUP_FAILED` | Proposal 0007. |
| `UNCAUGHT_EXCEPTION` | Proposal 0007. |
| `DISPOSAL_INCOMPLETE` | Proposal 0007. |

### The command registry

#### Declarations

Each command is declared once, in `crates/bento_cli`, with these fields:

| Field | Meaning |
| --- | --- |
| `id`, `path` | As in the vocabulary. |
| `summary` | One line, shown in help listings and search results. |
| `description` | Everything an agent needs to use the command correctly, in prose. |
| `arguments` | The command's arguments, below. |
| `output` | Its default output format. Every command accepts `text`; the other formats it accepts are those of its `results`. |
| `effect` | `reads` (reads files, changes nothing), `executes` (runs guest code, whose effects belong to the guest within its permissions), or `modifies` with what it modifies. |
| `requires` | What it needs besides `bento`: `project` (a `package.json` in the directory or an ancestor) and `pnpm` (on `PATH`, and declared by the project). |
| `dryRun` | Whether it accepts `--dry-run`. |
| `idempotent` | Whether running it twice with the same input leaves the same state as running it once. |
| `errors` | The `ErrorCode` variants a structured invocation can report, besides `INVALID_ARGUMENT` and `INTERNAL`. |
| `exitStatuses` | Each exit status and what it means. |
| `examples` | At least one: a summary and the words after `bento`. |
| `synonyms` | Words an agent might use for the command that its path and summary do not contain. |
| `results` | Each structured result it can write: its format (`json` or `ndjson`), whether it requires `--dry-run`, and the JSON Schema of the document, generated from the Rust result type (for `ndjson`, of one line). With `--dry-run`, the default format is the dry-run result's. `bento run` has one, the launch preview; `bento install` has none. |

Each argument declares:

| Field | Meaning |
| --- | --- |
| `name` | Kebab-case. A flag or option is written `--<name>`; a positional is written bare. |
| `kind` | `positional`, `option` (takes a value), `flag` (takes none), or `trailing` (every word after the positionals, passed through unparsed). |
| `type` | `string`, `path`, `integer`, `choice`, or `boolean`. |
| `required`, `multiple` | Whether it must be given, and whether it may be given more than once. |
| `default` | The value used when it is absent. The parser applies it, so it is the runtime default. |
| `acceptedValues`, `minimum`, `maximum` | For `choice` and `integer`. |
| `requires` | Other arguments it is valid only with. |
| `heading` | The help section that lists it, such as `Permissions`. |
| `stdin` | Whether the value `-` means standard input, which Bento then reads. No argument in this proposal sets it. |
| `summary` | One line. |

Validation that the parser cannot express, such as proposal 0003's messages for permission flags, the ceiling check, and a format accepted only with `--dry-run`, stays in Bento's code; `requires`, `results`, and `errors` declare it.

#### What the registry generates

- The clap parser, built with clap's builder API. The derive declarations in `arguments.rs` and `grant_flags.rs` are removed.
- `bento --help`, and each `bento <command> --help`.
- The `bento cli search` index and every `bento schema` answer.
- The CLI reference, `guides/cli-reference.md`, which `mise run cli:reference` regenerates, including its code table, which comes from the `ErrorCode` enum.
- `details.commands` and the closest-command suggestions in failure documents.

#### Parity

Tests in `bento_cli`, run by `mise run rust`, fail when any of these is false:

1. Every declaration is complete: its id is its path joined with dots and is unique; its summary and description are not empty; it has at least one example and declares its exit statuses; every argument name is kebab-case; every command whose default format is not `text` declares at least one result.
2. The parser's commands are exactly the registry's, and each command parses exactly the arguments it declares, besides `--help` (and `--version` at the root). Every command declares `--output`.
3. Every example parses as the command it belongs to.
4. Every declared default is the value the parser yields when the argument is absent. Without `--output`, an invocation uses the command's declared default format, and with `--dry-run` the dry-run result's format.
5. Every example of a structured command, and every `--dry-run` example of `bento run`, succeeds in a fixture project, and its stdout validates against the schema of the result it produces.
6. For every declared `requires`, giving the argument without the one it requires fails with `INVALID_ARGUMENT` in a structured invocation, and with status 1 in a pass-through one. Every format a command does not accept, and every format it accepts only with `--dry-run` given without it, fails with `INVALID_ARGUMENT`.
7. `bento --help` names every command.
8. `guides/cli-reference.md` is exactly what the registry renders, and its code table lists exactly the `ErrorCode` variants, each with its meaning and `details`.
9. Every Markdown file in `guides/` except `README.md` is compiled into the binary.

### Commands

| Command | Id | Summary | `--output` | Effect | Requires | `dryRun` |
| --- | --- | --- | --- | --- | --- | --- |
| `bento run` | `run` | Run a JavaScript or TypeScript source file | `text` (pass-through); with `--dry-run`, `json` (default) or `text` | `executes` | `project` | yes |
| `bento install` | `install` | Install dependencies and devDependencies with the project's pnpm | `text` (pass-through) | `modifies` `node_modules` and `pnpm-lock.yaml` | `project`, `pnpm` | no |
| `bento cli search` | `cli.search` | Find the bento command for a task described in plain words | `json` (default), `text` | `reads` | nothing | no |
| `bento schema` | `schema` | Show a command's arguments, effects, errors, exit statuses, examples, and result schemas | `json` (default), `text` | `reads` | nothing | no |
| `bento docs search` | `docs.search` | Search the guides built into this bento | `json` (default), `text` | `reads` | nothing | no |
| `bento docs show` | `docs.show` | Print one built-in guide, or one of its sections | `json` (default), `text` | `reads` | nothing | no |
| `bento context show` | `context.show` | Show the project, configuration, environment files, and permissions a launch from here would use | `json` (default), `text` | `reads` | nothing | no |

`bento run` is not idempotent; every other command is.

Arguments:

- `bento run`: `script` (positional path, required); `arguments` (trailing); `--watch` and `--dry-run` (flags); `--output` (choice, as above); `--error-format` (choice of `text` or `json`, default `text`, delivered with proposal 0007 as [Pass-through invocations](#pass-through-invocations) says); and under the heading `Permissions`, the flags proposal 0003 defines: `--permission`, `--allow-fs-read` and `--allow-fs-write` (repeatable path options), `--allow-env` (repeatable option), and the flags `--allow-net`, `--allow-child-process`, `--allow-worker`, `--allow-addons`, `--allow-wasi`, `--allow-ffi`, `--allow-inspector`, `--allow-openssl-store`, `--allow-fs-vfs`, and `--allow-language-model`. Every `--allow-*` argument requires `permission`.
- `bento install`: `--frozen-lockfile` (flag), `--output` (`text` only).
- `bento cli search` and `bento docs search`: `query` (positional string, required, multiple; the words are joined with spaces, so quoting is optional), `--limit` (integer from 1 to 20, default 5), `--output`.
- `bento schema`: `command` (positional string, optional, multiple), `--output`.
- `bento docs show`: `guide` (positional string, required), `--section` (option), `--output`.
- `bento context show`: `--output`.

Exit statuses:

- Structured results in this proposal: `0` on success; `1` on failure, with a failure document.
- `bento run`: `0` when the script finishes; the script's own code when it calls `process.exit(code)`; `130` and `143` when an interrupt or termination signal stops it; `1` for every Bento failure, which a script exiting with 1 also produces. With `--dry-run`, the structured statuses.
- `bento install`: `0` on success; pnpm's status when pnpm fails; `1` for every Bento failure.

`clap`'s `help` subcommand is removed; `bento <command> --help` replaces `bento help <command>`.

### Help

- `bento --help`, `bento -h`, and `bento` alone print the orientation to stdout and exit with status 0. It is at most 30 lines and contains: one sentence on what Bento is; the usage line; the commands in two groups, "Run" (`run`, `install`) and "Discover" (`cli search`, `schema`, `docs search`, `docs show`, `context show`), each with its summary; one sentence on `--output` and the failure document; and the discovery loop, ending with `bento docs show bento-for-agents`.
- `bento <command> --help` prints the command's summary and description, its usage, its arguments grouped by heading, its effect, and its examples, to stdout with status 0.
- Help is text in every invocation; it is usage, not a result.
- `bento --version` prints `bento <version>` as today.

### `bento cli search`

```text
bento cli search [--limit <1-20>] [--output json|text] <query>...
```

```json
{
    "items": [
        {
            "command": "bento run",
            "id": "run",
            "summary": "Run a JavaScript or TypeScript source file",
            "effect": "executes",
            "arguments": ["--allow-net"],
            "score": 0.81
        }
    ],
    "hasMore": false
}
```

- Ranks every command against the query and returns the best `--limit` matches, best first. Ties keep registry order. `hasMore` is true when more commands matched than were returned.
- `arguments` lists, in declaration order, the usage form of each argument whose name or summary matched a query word, so a query about network access returns `--allow-net`.
- Matching is deterministic and offline. Text is lowercased and split into ASCII letters and digits; a fixed list of stop words is ignored; a query word matches a word exactly at full weight, or by a shared stem at partial weight. A command's score combines, from highest weight to lowest, matches in its path, synonyms, summary, arguments, and description, averaged over the query words. Scores run from 0 to 1 and are rounded to three decimals.
- A query with no meaningful words, or no matches, returns no items with status 0.
- The tests require these rankings:

| Query                                  | First result                             |
| -------------------------------------- | ---------------------------------------- |
| `run a typescript file`                | `run`                                    |
| `preview a launch without running it`  | `run`                                    |
| `allow network access`                 | `run`, with `--allow-net` in `arguments` |
| `install dependencies`                 | `install`                                |
| `which tsconfig is used`               | `context.show`                           |
| `environment variables from env files` | `context.show`                           |
| `what flags does a command accept`     | `schema`                                 |
| `find a command`                       | `cli.search`                             |
| `search the documentation`             | `docs.search`                            |
| `read a guide`                         | `docs.show`                              |
| `zebra xylophone`                      | no items                                 |

### `bento schema`

```text
bento schema [--output json|text] [<command>...]
```

- Without a command, returns `{ "items": [...] }` with every command's `command`, `id`, `summary`, `effect`, and `output` (its default format), in registry order.
- With a command, returns its declaration: `id`, `command` (`bento` and the path), `summary`, `description`, `output`, `effect` (`kind`, and `what` for `modifies`), `requires`, `dryRun`, `idempotent`, `arguments` (each with `name`, `kind`, `usage` such as `--allow-fs-read <path>`, `type`, `required`, `multiple`, `default`, `acceptedValues`, `minimum`, `maximum`, `requires`, `heading`, `summary`), `results` (each with `format`, `dryRun`, and `schema`), `errors` (each with `code`, `retryable`, and `meaning`, including `INVALID_ARGUMENT` and `INTERNAL`), `exitStatuses` (each with `status` and `meaning`), and `examples` (each with `summary` and `command`, shell-quoted).
- `run`, `cli search`, `cli.search`, and `bento cli search` all name the same command, as separate words or one.
- An unknown command fails with `NOT_FOUND`. Its suggestions are `bento schema <path>` for up to three commands that `bento cli search` ranks highest for the given words, then `bento schema`.

### `bento docs search` and `bento docs show`

```text
bento docs search [--limit <1-20>] [--output json|text] <query>...
bento docs show [--section <heading>] [--output json|text] <guide>
```

The binary contains every Markdown file in `guides/` except `README.md`, embedded at build time by a build script, so the guides always describe the binary that serves them. A guide's name is its file name without `.md`.

`bento docs search`:

- Splits every guide into sections at its headings, outside fenced code blocks and after dropping frontmatter. It ranks the sections against the query by matches in the heading, in the guide's title (at half weight), and in the body, and returns `{ "items": [...], "hasMore": bool }`.
- Each item has `guide`, `title`, `section` (the heading text, or the title for text before the first heading), `excerpt` (the paragraph outside code blocks with the most query words, cut to 400 characters with an ellipsis), and `score`.
- Matching follows `bento cli search`'s rules. No matches returns no items.

`bento docs show`:

- Accepts a guide as `running-with-bento`, `running-with-bento.md`, `guides/running-with-bento.md`, or its record id `guide.running-with-bento`.
- Returns `{ "guide", "title", "describes", "section", "markdown" }`. `describes` is the guide's `describes` frontmatter. `markdown` is the guide without its frontmatter. `section` is null unless `--section` is given.
- `--section <heading>` returns only the first section whose heading matches, ignoring case, with the subsections below it; `section` then holds its heading.
- `--output text` prints the Markdown alone.
- An unknown guide fails with `NOT_FOUND`, with `details.guides` listing every guide and the suggestion `bento docs search "<topic>"`. An unknown section fails with `NOT_FOUND`, with `details.sections` listing the guide's headings.

### The built-in guides

Two new guides ship and are compiled in with the others:

- `guides/bento-for-agents.md` (`id: guide.bento-for-agents`, `title: Bento for Agents`, `describes: [proposal.0005]`) is the orientation an agent reads first, written in Phase 3 from this proposal and kept under 120 lines. It covers when to use `bento`; the loop of `bento cli search`, `bento schema`, `bento docs search`, `bento context show`, `bento run --dry-run`, running, and verifying; `--output`, the two output contracts, the failure document, error codes, and exit statuses; that a bare `bento run` is unrestricted and `--permission` restricts it; that `--dry-run` and `--output` must come before the script, because after it they belong to the script, which then runs; and what a dry run does not check. It names other guides rather than repeating them, and includes one line a project can put in its own `AGENTS.md`.
- `guides/cli-reference.md` (`id: guide.cli-reference`, `title: CLI Reference`, `describes: [proposal.0005]`) is generated from the registry. It states the wire contract: the output formats, the two output contracts and the rule between them, the failure document's JSON Schema, and the code table from the `ErrorCode` enum. Then it documents each command: summary, description, effect, requirements, dry-run support, idempotence, default output format, results and their schemas, errors, exit statuses, an argument table, and examples. It opens by saying it is generated and must not be edited by hand.

### Shared reports

`bento context show` and `bento run --dry-run` describe permissions, environment files, and `tsconfig.json` with the same three objects. A field marked "context only" is always null in a launch preview: where a context reports a problem, the dry run fails instead, because the launch would fail.

#### Permissions

```json
{
    "mode": "restricted",
    "source": "flags",
    "allow": {
        "fs-read": ["/Users/you/app"],
        "fs-write": ["/Users/you/app/db/*"],
        "env": ["PORT", "APP_*"],
        "net": true,
        "child-process": false,
        "worker": false,
        "addons": false,
        "wasi": false,
        "ffi": false,
        "inspector": false,
        "openssl-store": false,
        "fs-vfs": false,
        "language-model": false
    },
    "implicitRead": "/Users/you/app/server.ts",
    "ceiling": null,
    "problem": null
}
```

- `mode` is `unrestricted` or `restricted`, as proposal 0003 defines them.
- `source` is `default` (no `--permission` and no ceiling), `flags` (`--permission` was given), or `ceiling` (no `--permission`, so the launch holds exactly the inherited ceiling).
- `allow` is null for an unrestricted launch. Otherwise its keys are the permission flags without `--allow-`. `fs-read` and `fs-write` list the real paths and wildcard patterns the launch holds, resolved as proposal 0003 resolves them, with `["*"]` for every path. `env` lists names and prefix patterns. Every other key is a boolean.
- `implicitRead` is the script the launch may read without `--allow-fs-read`, or null. It is always null in a context.
- `ceiling` is the decoded `BENTO_PERMISSION_CEILING` as `{ "mode", "allow" }`, or null when none was inherited.
- `problem` (context only) says why `BENTO_PERMISSION_CEILING` cannot be read, or is null.

The report is built from the same permissions value the launch would pass to `RuntimeConfig::new`.

#### Environment

```json
{
    "kind": "run",
    "mode": "development",
    "modeSource": "inherited",
    "files": [
        {
            "layer": ".env",
            "path": "/Users/you/app/.env",
            "exists": true,
            "names": ["DATABASE_URL", "SESSION_SECRET"],
            "problem": null
        },
        {
            "layer": ".env.development",
            "path": "/Users/you/app/.env.development",
            "exists": true,
            "names": ["DATABASE_URL"],
            "problem": null
        },
        {
            "layer": ".env.local",
            "path": "/Users/you/app/.env.local",
            "exists": false,
            "names": [],
            "problem": null
        }
    ],
    "variables": [
        {
            "name": "DATABASE_URL",
            "layers": [".env", ".env.development"],
            "source": ".env.development"
        },
        { "name": "SESSION_SECRET", "layers": [".env", "inherited"], "source": "inherited" }
    ],
    "inherited": { "count": 58, "keptNames": null },
    "problem": null
}
```

- `kind` is `watch` for `bento run --watch` and `run` otherwise. A context reports `run`.
- `mode` is the environment mode Bento selected, or null. `modeSource` is `inherited`, `.env`, or `default` (the watch default), or null with no mode.
- `files` lists the environment files in load order: `.env`; `.env.<mode>` only when there is a mode; `.env.local`. Each has its `layer`, `path`, whether it `exists`, the sorted `names` it defines, and `problem` (context only) when it cannot be read or parsed. A problem message names the file and line and never the value.
- `variables` lists every variable an environment file or the watch default sets, sorted by name. `layers` names every layer that sets it, lowest precedence first, among `.env`, `.env.<mode>`, `.env.local`, `default`, and `inherited` (an exported variable). `source` is the layer whose value the script starts with, after proposal 0003 removes an inherited value a restricted launch does not allow; the file's value is then kept.
- `inherited.count` counts the variables `bento` inherited, without `BENTO_PERMISSION_CEILING`. `inherited.keptNames` is null for an unrestricted launch, whose script sees every inherited variable. For a restricted launch it lists, sorted, the inherited variables the script still sees: those `--allow-env` matches and those on Node's always-kept list.
- `problem` (context only) says why the environment cannot be prepared, such as a `NODE_ENV` containing `/` or a variable that is not UTF-8.
- No value of any variable appears anywhere in the report, except `mode`.

#### `tsconfig.json`

```json
{
    "path": "/Users/you/app/tsconfig.json",
    "chain": [
        { "path": "/Users/you/app/tsconfig.json", "extendedAs": null },
        {
            "path": "/Users/you/app/node_modules/@tsconfig/node22/tsconfig.json",
            "extendedAs": "@tsconfig/node22/tsconfig.json"
        }
    ],
    "executionOptions": {
        "jsx": { "mode": "automatic", "importSource": "remix/component", "development": false },
        "verbatimModuleSyntax": true,
        "experimentalDecorators": false,
        "emitDecoratorMetadata": false,
        "strictNullChecks": true,
        "useDefineForClassFields": true
    },
    "diagnostics": [
        {
            "severity": "warning",
            "path": "/Users/you/app/tsconfig.json",
            "option": "paths",
            "message": "\"paths\" aliases @/* are not applied at runtime; ..."
        }
    ],
    "problem": null
}
```

- `path` is the project's `tsconfig.json`, or null when it does not exist.
- `chain` lists the files that contribute options, starting with the project's own and following each `extends` entry depth-first in declaration order, as TypeScript reads them. `extendedAs` is the specifier as written in the extending file, or null for the project's own. It is empty without a `tsconfig.json`.
- `executionOptions` holds the settings Bento applies when it transforms the project's sources, as the [TypeScript and JSX](../guides/typescript-and-jsx.md) guide lists them, with TypeScript's defaults when there is no `tsconfig.json`. `jsx` is `{ "mode": "automatic", "importSource", "development" }`, `{ "mode": "classic", "factory", "fragmentFactory" }`, or `{ "mode": "unavailable", "configured" }`.
- `diagnostics` are the diagnostics `CompilerSettings::load` returns, with their severity as Bento assigns it. They are reported here, never printed.
- `problem` (context only) says why `tsconfig.json` or a file it extends cannot be read or parsed. `executionOptions` and `diagnostics` are then null.

### `bento context show`

```text
bento context show [--output json|text]
```

Reports what a `bento run` started from the current directory without flags would use, for a script in the enclosing project. It reads the same inherited environment such a launch would.

```json
{
    "bento": {
        "version": "0.1.0",
        "executable": "/Users/you/.cargo/bin/bento",
        "target": "aarch64-apple-darwin",
        "features": { "prompt-foundation-models": true },
        "languageModel": {
            "backend": "foundation-models",
            "sdkVersion": "27.2",
            "availability": "available"
        }
    },
    "invocationDirectory": "/Users/you/app/src",
    "project": {
        "root": "/Users/you/app",
        "package": {
            "path": "/Users/you/app/package.json",
            "name": "guest-book",
            "type": "module",
            "packageManager": { "name": "pnpm", "version": "12" },
            "imports": { "#app/*": "./app/*" },
            "exports": null,
            "problem": null
        }
    },
    "tsconfig": { "...": "the tsconfig report" },
    "environment": { "...": "the environment report" },
    "permissions": { "...": "the permissions report" }
}
```

- `bento.version` is the crate version; `executable` is the running binary's canonical path; `target` is the target triple it was built for; `features` maps each of `bento_cli`'s Cargo features to whether this build enabled it.
- `bento.languageModel.backend` is `foundation-models` when the native backend is compiled in, else null. `sdkVersion` is the macOS SDK it was built against (`bento_prompt::NATIVE_SDK_VERSION`), or null. `availability` is `available` or `unavailable`, from `bento_prompt::availability` with default options: the model's own readiness, regardless of whether a launch is permitted to use it. A build without a backend reports `unavailable`.
- `project` is null when no `package.json` encloses the current directory; `tsconfig` and `environment` are then null too.
- `project.package` summarizes the `package.json` Bento would use: `name` (null when absent or not a string), `type` (`module`, `commonjs`, or null), `packageManager` (from `devEngines.packageManager`, which `bento install` reads; the top-level `packageManager` field is not read), and `imports` and `exports` exactly as written, or null. It never contains `tasks` or `scripts`. `problem` says why the file cannot be read or does not have a shape Bento accepts; the other fields are then null.
- `permissions` describes a flagless launch: unrestricted with source `default`, or the inherited ceiling with source `ceiling`.
- Problems in project files never fail the command; they appear in the report, and the command exits with 0. The command fails only with `INVALID_ARGUMENT`, with `PRECONDITION_FAILED` when the current directory cannot be determined, or with `INTERNAL`.
- To see a launch with flags, `--watch`, or a script in another project, use `bento run --dry-run`.

### `bento run --dry-run`

```text
bento run --dry-run [--output json|text] [--watch] [--permission] [--allow-...]... <script> [<argument>...]
```

A dry run prepares the launch exactly as `bento run` would, by calling the same code, and stops before `bento_runtime::start`. It runs these steps in the launch's order and fails at the first that fails:

1. **Arguments.** Parse the command line and apply proposal 0003's rules for permission flags. A violation fails with `INVALID_ARGUMENT`; the message is the one the launch prints, and `details.argument` names the flag.
2. **Inherited environment and permissions.** Read the inherited environment and decode `BENTO_PERMISSION_CEILING`. A variable that is not UTF-8, or a ceiling that cannot be read, fails with `INVALID_CONFIGURATION` and `details.variable`. A flag asking for more than the ceiling fails with `PERMISSION_DENIED`, with `details.argument` and `details.limit`. A file system flag whose path cannot be resolved fails with `INVALID_ARGUMENT`.
3. **Script and project.** Resolve the script against the current directory and find its project. A missing script, or one that is not a regular file, fails with `NOT_FOUND` and `details.path`. No enclosing `package.json` fails with `NOT_FOUND`, `details.missing: "package.json"`, and `details.searchedFrom`. An unreadable `package.json` fails with `PRECONDITION_FAILED`; one that is not valid JSON, or has a field Bento rejects, fails with `INVALID_CONFIGURATION` and `details.path`, with `location` at the syntax error.
4. **Environment files.** Load them for `watch` with `--watch` and `run` otherwise. An unreadable file fails with `PRECONDITION_FAILED`; a malformed one fails with `INVALID_CONFIGURATION`, `details.path`, and `location` at its line, with `column` null; and a `NODE_ENV` containing `/` fails with `INVALID_CONFIGURATION` and `details.variable: "NODE_ENV"`. No message carries a value.
5. **`tsconfig.json`.** Load it with `CompilerSettings::load`. A file in the chain that cannot be read or parsed fails with `INVALID_CONFIGURATION` and `details.path`, with `location` when the parser reports a position. Diagnostics go into the preview.
6. **Reading the script.** When the launch's permissions would not let it read its own script, as when a restricted ceiling does not cover it, fail with `PERMISSION_DENIED`, `details.permission: "FileSystemRead"`, and `details.resource` set to the script's canonical path: the denial a real run would report when it loads the script.
7. **Transforming the script.** For a `.ts`, `.mts`, `.cts`, `.tsx`, or `.jsx` script, read it and transform it with `bento_compiler::transform` and the settings the runtime would use: the project's, or TypeScript's defaults for a dependency source as the [TypeScript and JSX](../guides/typescript-and-jsx.md) guide defines one. A source Oxc's parser or semantic checker rejects fails with `SYNTAX_ERROR`; syntax the settings leave untransformed, or any other transformer error, fails with `TRANSFORM_FAILED`. Both set `location`. These are the codes a real run reports for the same failure. An unreadable script fails with `PRECONDITION_FAILED`. Other scripts are evaluated as written, so they are not checked.

On success the result is the launch preview:

```json
{
    "status": "dry-run",
    "entry": {
        "path": "/Users/you/app/server.ts",
        "transform": "typescript",
        "settings": "project"
    },
    "projectRoot": "/Users/you/app",
    "cwd": "/Users/you/app",
    "argv": ["--port", "3000"],
    "watch": false,
    "permissions": { "...": "the permissions report" },
    "environment": { "...": "the environment report" },
    "tsconfig": { "...": "the tsconfig report" },
    "notChecked": ["imports", "execution"]
}
```

- `status` is always `dry-run`.
- `entry.path` is the script's canonical path. `entry.transform` is `typescript` (`.ts`, `.mts`, `.cts`), `tsx`, `jsx`, or `none`. `entry.settings` is `project`, `default` for a dependency source, or null when the script is not transformed.
- `projectRoot` is the project's root. `cwd` is the script's `process.cwd()`: the canonical current directory. `argv` is every word after the script, verbatim, as the script would see in `process.argv.slice(2)`. `watch` says whether `--watch` was given.
- `notChecked` names what the preview does not establish. It always contains `imports`, because the modules the script imports are not resolved, read, or transformed, so a missing package, an import the permissions do not allow reading, or a syntax error in another module is not detected; and `execution`, because the script never runs, so its runtime behavior and the operations its permissions deny are not predicted. It contains `entry-syntax` when `entry.transform` is `none`, because Bento hands such a script to the engine as written and the dry run does not parse it.
- A dry run reads only `package.json`, the files of the `tsconfig.json` chain and the package manifests resolving it reads, the environment files, and the script. It starts no JavaScript engine, subprocess, file watcher, or network connection, and writes no file. It prints no diagnostics to stderr.
- `--dry-run` and `--output` are Bento options and go before the script. After the script they belong to the script: `bento run server.ts --dry-run` runs `server.ts` with `--dry-run` in `process.argv`, as Node would. The `--dry-run` summary, `bento schema run`, and Bento for Agents say so.
- `--output json` and `--output text` are the dry run's formats, `json` by default. `bento run --output json` without `--dry-run` fails with `INVALID_ARGUMENT` until proposal 0007 adds the run receipt, as [Output formats](#output-formats) says.

### Library changes

The CLI composes these public additions, so a Rust host can build the same reports:

- `bento_workspace::LaunchEnvironment` gains `files()`, the environment files it considered with their layer, path, whether each exists, and the names each defines; and `provenance()`, for every variable a file or the launch default sets, the layers that set it in precedence order, including the inherited layer when an inherited variable overrides it. Neither carries values. It keeps the file value an inherited variable overrides, which proposal 0003's restricted launches also need.
- `bento_workspace::Manifest` gains `name`, `imports`, and `exports`, reported and not interpreted: resolution reads `imports` and `exports` through `oxc_resolver` as before, and a `name` that is not a string is null rather than an error.
- `bento_compiler` gains `tsconfig_chain(project_root)`, returning the chain described above or the `ConfigError` that `CompilerSettings::load` would return. `bento_compiler::TransformError` gains `kind`: `Syntax` for diagnostics from Oxc's parser and semantic checker, and `Transform` for everything else `transform` rejects, so the dry run and the runtime tell `SYNTAX_ERROR` from `TRANSFORM_FAILED` the same way.
- `bento_cli` adds `schemars` for result schemas, and `jsonschema` as a development dependency for the parity tests.

### Guides and repository

- `guides/bento-for-agents.md` and the generated `guides/cli-reference.md` are added.
- [Running with Bento](../guides/running-with-bento.md) gains sections, written from this proposal, on discovering commands, previewing a launch with `--dry-run`, and the wire contract: `--output`, the output contracts, failure documents, and error codes.
- The README's usage section names the discovery commands and `bento docs show bento-for-agents`.
- `mise.toml` gains `cli:reference`, which regenerates `guides/cli-reference.md` from the registry.

## Compatibility

- `bento run` and `bento install` without `--dry-run` keep their streams, messages, and exit statuses, and accept exactly the command lines proposals 0003 and 0004 define, plus `--output text`, which changes nothing. A usage error's last line now names `bento schema <command>`.
- `bento` alone prints the orientation to stdout and exits with 0 instead of 1.
- Words that select no command now produce a failure document on stderr, still with status 1, instead of clap's text.
- `bento help <command>` is removed; `bento <command> --help` prints the same help.
- All help text is regenerated and reworded.
- New: the five discovery commands, `--dry-run`, and `--output` on every command. `bento run --output json` without `--dry-run` fails with a failure document until proposal 0007 gives it a receipt.
- The library additions are additive. No stored data changes.

## Implications on adoption

Nothing new to install. The binary grows by the embedded guides, about 200 KB of Markdown. A project that wants agents to use Bento can add one line to its `AGENTS.md`, which Bento for Agents provides. Reverting the change restores today's CLI without migration.

## Scope

- The registry, the generated parser, help, and CLI reference, and the parity tests.
- The wire contract: the output formats, both output contracts and the rule between them, the failure document with `location`, and the `ErrorCode` enum and its table. `--error-format` is defined here and delivered with proposal 0007's codes, as [Pass-through invocations](#pass-through-invocations) says.
- `bento cli search`, `bento schema`, `bento docs search`, `bento docs show`, and `bento context show`.
- `bento run --dry-run`, with tests that it runs no guest code and writes nothing (a script whose top level writes a marker file leaves no marker), that every failure step reports its code, and that no environment value appears in any output (a `.env` secret never appears in context, preview, or failure output).
- The library additions, the guides, the README paragraph, and the `cli:reference` task.

### Out of scope

- `bento resolve` and module-resolution inspection: proposal 0006. A dry run checks only the script, and says so in `notChecked`.
- What proposal 0007 adds under this contract: structured runtime errors and the codes `STARTUP_FAILED`, `UNCAUGHT_EXCEPTION`, and `DISPOSAL_INCOMPLETE`; delivering `--error-format`; `bento run --output json` without `--dry-run`, a run receipt with captured standard streams; and `bento run --output ndjson`, a stream of runtime events. Pass-through invocations keep their human messages until then.
- Inline and standard-input evaluation (`-e`, `-p`, `-`): proposal 0008.
- `--process-timeout` and heap limits: proposal 0009.
- `node:vfs` and `--vfs-load`: proposal 0010.
- Accepting Node's command line ([#260](https://github.com/bento-js/bento/issues/260)), audit mode ([#261](https://github.com/bento-js/bento/issues/261)), and Node configuration files ([#262](https://github.com/bento-js/bento/issues/262)). [Future directions](#future-directions) notes how each fits the registry and the reports.
- `bento install --dry-run`. Bento delegates installation to pnpm and would need pnpm to compute a faithful plan; `install` declares `dryRun: false` until one exists.
- The severity of `"jsx": "preserve"` diagnostics ([#265](https://github.com/bento-js/bento/issues/265)). The reports show diagnostics as Bento classifies them.
- An MCP server and shell completions, which the registry could generate later.

## Preview

- Artifact: the macOS arm64 `bento` executable archives that CI publishes for this pull request (`bento-macos27-arm64-prompt` and `bento-macos26-arm64-backend-free`), and the documentation preview under `pr-preview/pr-269/`.
- Reason: the change is the executable's behavior, and installing it is the cheapest way to exercise it. In `fixtures/remix-guest-book` with a secret in `.env`, the reviewer runs `bento`, `bento cli search` and `bento schema run`, `bento docs search "permissions"` and `bento docs show bento-for-agents`, and `bento context show`, both piped to `jq` and on a terminal, and checks that the secret's value never appears. They run `bento run --dry-run --watch server.ts`, a dry run of a `.tsx` file with JSX but no `jsx` setting, and a dry run under a narrower `BENTO_PERMISSION_CEILING`, then run `bento run server.ts` and confirm its output is unchanged. The backend-free archive shows `languageModel.backend: null`. Optionally, a fresh agent session given only the installed binary is asked to run the guest book restricted to the network and its own directory.

## Policies and decisions checked

- [policy.0001](../policies/0001-prefer-voidzero-crates.md): honored. `tsconfig.json` loading and script transformation reuse `bento_compiler` on Oxc and `oxc_resolver`. The new dependencies, `schemars` and `jsonschema`, fill no parsing, transformation, resolution, source-map, minification, or bundling role.
- `decisions/`: holds no decisions.
- [VISION.md](../VISION.md): "Embedding is the default" is honored by putting provenance, the `tsconfig.json` chain, and the package summary in the libraries; "compatibility is behavior" by leaving `bento run`'s streams untouched; and "the host decides what code may do" by reporting a launch's permissions before it runs.
- [ARCHITECTURE.md](../ARCHITECTURE.md): `bento_cli` stays "a thin composition layer over the libraries". The registry describes the CLI and lives in it; the reports compose library functions.
- Proposal 0003 ([PR #259](https://github.com/bento-js/bento/pull/259), approved for implementation): the dry run reproduces its flag rules, messages, ceilings, implied script read, and environment removal by calling the same code, and the permissions report uses its vocabulary.
- Proposal 0004 ([PR #268](https://github.com/bento-js/bento/pull/268), draft): the registry declares `bento run <script>` with no task targets, and `context show` reports no `tasks`.
- Proposals 0006 ([PR #274](https://github.com/bento-js/bento/pull/274)) and 0007 ([PR #277](https://github.com/bento-js/bento/pull/277)), drafts: on 2026-11-27 Mark decided that this proposal owns the output formats, the failure document, and the code table, and that those proposals adopt them. The wire contract above is written for them to cite.

## Future directions

- An MCP server generated from the same registry, so tools and commands cannot disagree.
- Shell completions generated from the registry's parser.
- `bento install --dry-run`, if pnpm offers a preview that touches neither the store nor the lockfile.
- An agent-usability benchmark in `bench/` measuring discovery accuracy, invalid-command rate, and commands per task for an agent with no prior knowledge of Bento.
- `bento_runtime::check(config, entry)`, the embedding counterpart of a dry run.
- Accepting Node's command line (#260) would register its invocation form as another pass-through command in the registry. Audit mode (#261) would add a field to the permissions report. Node configuration files (#262) would add the file to the context and a `config-file` value to the permissions report's `source`.
- Proposals 0006 through 0010 declare their commands, options, results, and codes in the registry under this contract, which puts them in help, search, schema, and the reference without further work.
- Per-option provenance in the `tsconfig.json` report, naming the file that set each execution option.

## Alternatives considered

- **JSON for every command, including `bento run` and `bento install`, as `cf` does.** A runtime's output belongs to the program. Wrapping it would break `bento run script.ts | jq`, Node parity, and every tool that reads a child's stdout.
- **Human output by default with an opt-in `--json`, as the CLI Guidelines and `gh` do.** The agent must already know the flag. Cloudflare saw agents add `--json` and pipe through `jq` anyway, and chose JSON as the default.
- **Keep clap's derive declarations and attach a metadata table by command id.** Summaries and defaults would live in two places, and a table can lag the parser in ways a structural test does not see. One declaration that builds the parser cannot.
- **A separate `bento plan` or `bento check` command instead of `bento run --dry-run`.** A preview has to accept exactly the arguments of the launch it previews and validate them along the same path, or it drifts from the launch as Kubernetes's client-side dry run did. A mode of the same command shares both.
- **A dry run that exits 0 with a list of problems.** Exit status is the verdict scripts and agents check first. A failure document carries everything the problem list would.
- **A different exit status for each error code, as `mcp` does.** Bento's usage errors already exit with 1 (proposal 0003), and a run's other statuses belong to the script. Agents branch on `code`.
- **Search the online documentation site, as `cf docs search` does.** The site follows `main` rather than the installed binary, and Bento must work offline.
- **Embed proposals and policies as documentation.** They record intent and history, not what the binary does.
- **Report environment values masked, hashed, or by length.** Each still leaks something about a secret. Names and layers answer which file supplied a variable.
- **Resolve the script's whole module graph in a dry run.** That is module-resolution inspection, which proposal 0006 owns. The preview names the gap instead.
- **Each proposal defines its own output option, failure envelope, and error codes.** Proposal 0007's first draft defined `--error-format`, the failure body's `location`, and its run codes itself, and proposal 0006 asked whether to wait for this proposal. Contracts defined one proposal at a time drift: `--output` could mean different things on different commands, and code lists could collide. Mark decided on 2026-11-27 that this proposal owns the output formats, the failure document, and the code table, and that the others adopt them.

## Open questions

- [NEEDS CLARIFICATION: Sequencing. This design declares `bento run` as proposals 0003 and 0004 define it: Node's permission flags, and script targets only. Should implementation start only after both merge? If proposal 0004 is not accepted, should `bento run --dry-run <task-name>` fail with `INVALID_ARGUMENT`, or must the preview cover task targets too?]
- [NEEDS CLARIFICATION: Where the generated CLI reference lives. This proposal generates it into `guides/cli-reference.md`, so the documentation site, `bento docs`, and the reference are one file. The process says guides are written from proposals, not from code. Is a generated reference acceptable under `guides/`, or should it live elsewhere, such as `docs/cli-reference.md`, and stay out of `bento docs`?]

## Acknowledgments

Mark's `mcp` CLI is the working model for the registry, the discovery commands, the failure document, and the parity tests. Mark's guide to agent-first command-line interfaces and Cloudflare's `cf` shaped the discover, inspect, preview, run, and verify loop.
