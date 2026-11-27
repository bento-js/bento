# Bento Node compatibility dashboard

A local, read-only dashboard for two different kinds of evidence: **behavioral results** from upstream Node test files, and **availability** of globals and built-in module exports. A present export is not proof that it behaves correctly.

The app follows the Remix guest-book fixture's architecture—Remix controllers, server-rendered components, asset serving, and SQLite through `remix/data-table`—using pinned **Remix 3.3.0**. Node hosts the dashboard and collection tools so a Bento regression cannot prevent you from inspecting its results. Mise owns toolchains and tasks; the app never uses Bento's task runner.

## Start locally

From the repository root:

```sh
mise run compat:setup
mise run compat:dev
```

Open <http://127.0.0.1:4318>. The server binds only to loopback. `PORT` changes the port. The first visit shows an empty state with collection commands; no example results are presented as measurements.

## Collect evidence

Use a Bento executable you want to measure. Build this checkout with `mise run build`, or pass the path to another build. From the repository root:

```sh
mise run compat:prepare
mise run compat:collect -- --bento ../../target/debug/bento --filter buffer
```

The executable path is relative to `apps/node-compat`. `--filter buffer` selects behavioral test paths containing `buffer`; the availability inventory still covers the full public baseline. Omit `--filter` to run the complete declared behavioral corpus. A full run can take a long time and executes trusted upstream test code with your user's authority. It is not a sandbox. Do not point the collector at untrusted executables or modified test sources.

Select one evidence kind when needed:

```sh
mise run compat:collect -- --kind behavior --bento ../../target/debug/bento --filter assert
mise run compat:collect -- --kind inventory --bento ../../target/debug/bento
```

`--timeout-ms` sets the per-process deadline. `--bento-arg=ARG` adds an explicit Bento run argument, recorded with the evidence. Builds implementing proposal 0003 need no permission arguments for an unrestricted run. The collector does not guess a permission model or translate flags.

Collection does not run through the browser. Refresh the dashboard after a run to see its results. Ordinary test failures are measurements, not collection failures; a missing executable, invalid selection, or failed corpus preparation is an error.

## Read the results

**Behavior** compares the same unmodified upstream files under Node and Bento. The corpus is Node **26.12.1**, commit `bc2e01dcf6acd5cf7d169b5f26cf89abaaeb41f1`, specifically `test/parallel` and `test/sequential` JavaScript test files and their upstream helpers and fixtures. Native-addon build suites, Web Platform Tests, and other Node test directories are outside this corpus. The dashboard shows both the corpus size and the selected size; a filtered run is not a measurement of the entire suite.

Bento requires a project manifest. The harness adds `{"private":true}` as `test/parallel/package.json` and `test/sequential/package.json` in both scratch copies, under Node and Bento alike. It records this adaptation in the corpus scope. Upstream test files and helpers remain unchanged, and the cached upstream source is never patched.

Both runtimes run with `NODE_TEST_KNOWN_GLOBALS=0`, following Deno's harness: additional runtime globals are measured by the availability inventory instead of failing every behavioral file. This disables the shared harness's global-leak check, including its ability to catch globals leaked by individual tests. API assertions and `mustCall` checks remain active. The setting is recorded in the corpus scope, so runs from a different harness profile cannot be compared as regressions.

A behavioral pass requires both a passing Node control and a passing Bento execution. Failures of the Node control remain visible and are not credited as Bento compatibility. Skipped tests, unsupported flags, timeouts, crashes, and unrun cases stay distinct. No expected-failure configuration converts failures into passes. Node-style scripts can legitimately complete without TAP output; successful file execution is not a count of individual assertions. Recognized TAP skips and empty test plans are not passes.

Bento does not implement every Node command-line flag. Tests requiring unsupported flags are marked **unsupported**, not silently run without their requirements. Each case shows the original flags, exact invocation, exit code or signal, duration, stdout, and stderr. Logs are bounded and explicitly marked when truncated.

**Availability** compares globals and public built-in module exports against Node, including prefix-only built-ins. Internal underscore-prefixed modules are excluded. It reports present, missing, type mismatch, or inspection error; it does not call every API or infer behavioral support. When Bento's `node:module.isBuiltin()` reports a built-in absent, every Node export from that module is missing. Other import failures are inspection errors. One failed module import does not erase the remaining inventory.

**History** preserves runs in SQLite. A regression is a previously passing case that no longer passes under comparable conditions. Comparisons require complete behavior runs with the same corpus revision, selection, platform, Node control, launch arguments, and execution deadline. Changing the Bento build is allowed; changing the measurement conditions is not silently treated as a regression. Interrupted or failed collection remains incomplete rather than becoming a smaller, inflated pass rate.

## Local data and checks

The app stores results in `apps/node-compat/.data/results.sqlite` and caches upstream source beneath `.data`. These files are ignored by git. Collection uses a scratch copy rather than running mutable tests inside the cached source. Keep `.data` to retain history. The UI has no delete, upload, or arbitrary-command endpoint.

From `apps/node-compat`:

```sh
mise run setup
mise run prepare
mise run collect -- --bento /absolute/path/to/bento --filter buffer
mise run dev
mise run check
```

The root `mise run check` also checks this app. Tests use the fixture's `remix/test`; formatting, linting, and types use Oxfmt, Oxlint, and TypeScript. No Rust crate changes are required.

## Methodology references

- [Node's upstream tests](https://github.com/nodejs/node/tree/v26.12.1/test) supply the behavioral corpus.
- [Deno's Node compatibility harness](https://github.com/denoland/deno/tree/main/tests/node_compat) and [daily viewer](https://github.com/denoland/node_test_viewer) informed the separation of full measurements, exclusions, and history.
- [Bun's Node tests](https://github.com/oven-sh/bun/tree/main/test/js/node) demonstrate upstream-test reuse; this dashboard stores executed results rather than inferring success from included files.
- [Pooya Parsa's platform-node-compat](https://github.com/pi0/platform-node-compat) informed the separate availability view. Its application code is not copied.

MIT. See the repository's `LICENSE` file.
