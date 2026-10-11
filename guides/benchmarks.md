---
id: guide.benchmarks
title: Benchmarks
describes: [proposal.0001]
---

# Benchmarks

## What it does

The benchmark harness measures one real application, the Remix 3 guest book in `fixtures/remix-guest-book`, under Node, Bun, and Bento, and records raw samples plus a summary you can read and re-run. Every runtime receives the same source, the same locked packages, the same seeded database, the same environment mode, and equivalent authority.

Correctness comes before timing. Before any workload measures a runtime, an acceptance checker proves that the runtime serves the actual guest book: the full document, every asset, a valid submission, and the refusal of invalid ones. A runtime that fails a gate is reported as `incompatible`, with the failing check and the response that failed it; one that cannot be launched, exits, or never listens is reported the same way at a `startup` check, with the operating system's error or its output. It never gets a timing row, because an empty page returned quickly is not a fast result.

The harness is tooling, written in TypeScript and run by Node. It does not execute the application itself; each runtime does that through its own launch path.

### What each workload answers

| Workload | What it answers |
| --- | --- |
| `cold-launch` | How long from spawning the process until the port accepts connections, and until the first response that passes the document gate. The two milestones are reported separately, because the second includes application initialization (imports, database, native asset compilation of the entry graph). Measured in development and production modes. |
| `assets` | How long the first request for each JavaScript, TSX, and CSS asset the page references takes, compared with repeat requests in the same process, which hit the fixture's in-memory cache. Measured in development and production modes. The fixture compiles the entry graph while the server starts, so first requests can already be warm for those assets; the cold cost shows up in `cold-launch`. |
| `asset-operations` | Where time goes inside the asset pipeline: parsing, resolving, transforming, minifying, and Lightning CSS processing, each timed on the fixture's own files through the locked npm packages with identical options. It runs without the application, so it is valid for a runtime whose full-app render is still incompatible. |
| `steady-state` | Throughput and latency percentiles for a rendered `GET /`, an asset `GET`, and a persisted valid `POST /` at fixed concurrency, after a warmup, in production mode. The database is reset for every trial, and persisted rows are counted afterwards so a response that was not really stored is caught. |
| `watch-reload` | How long from editing a file until the first response that shows the change, with each runtime's watch mode running: once for a source file the entry imports (a visible string in `app/actions/guest-book/page.tsx`) and once for the entry itself (`server.ts`, observed through a response header). It also confirms that the database contents and the listener survive each reload. Edited files are restored afterwards. |
| `memory` | Resident memory of the whole process tree: idle after startup, and the peak sampled from spawn through startup and the first request for every asset. This is the footprint of the usable application with its native dependencies, not an empty isolate. |

## How to use it

### Prerequisites

- Node 26.11.1 and `pnpm` 12 on `PATH`. The harness runs on Node, and each run installs the fixture with `pnpm install --frozen-lockfile`, which needs registry access the first time.
- `rsync`, which macOS provides.
- Bun 1.4.3 on `PATH` for the Bun runtime. A missing Bun is reported as unavailable.
- `oha` for the `steady-state` workload. Install it with `cargo install --locked oha`; the harness records the version it finds.
- Bento: build the executable and set `BENTO_BIN` to its path. When `BENTO_BIN` is unset, or the file is missing or not executable, the harness reports Bento as unavailable with that reason. It never skips Bento silently and never substitutes another runtime.

### Running

Run from the repository root, in a quiet window with no builds or tests running, and one workload at a time (the harness already does this):

```sh
node bench/src/main.ts [--runtimes node,bun,bento] [--workloads cold-launch,assets,...] [--trials N]
```

| Option | Meaning | Default |
| --- | --- | --- |
| `--runtimes` | Comma-separated runtimes to measure. | `node,bun,bento` |
| `--workloads` | Comma-separated workloads to run. | all six |
| `--trials N` | Trial count for every selected workload. | 15 cold launches; 10 for assets, asset operations, watch reloads, and memory; 5 for steady state |
| `--duration S` | Measured seconds per steady-state scenario. | 10 |
| `--warmup S` | Warmup seconds per steady-state scenario. | 3 |
| `--concurrency N` | Steady-state connections. | 16 |
| `--iterations N` | Passes per asset-operations trial (the first is the cold pass, the rest are warm) and repeat requests per asset in the `assets` workload. | 10 |
| `--output DIR` | Where results are written. | `bench/results/<UTC timestamp>/` |
| `--keep-fixtures` | Keep the disposable fixture copies under `/tmp/bento-bench/` for inspection. | remove them |

Each runtime gets a fresh copy of the fixture under `/tmp/bento-bench/`, installed with a frozen lockfile, migrated with `remix db migrate`, and seeded through the application's own `POST /` with the same fixed entries. Each trial starts from that seeded database. Servers listen on ports 17700 to 17799. Every process the harness starts runs in its own process group and is stopped when its measurement ends, when the run fails, and when the harness is interrupted.

Interrupting the run with `Ctrl-C` (SIGINT) or SIGTERM unwinds it instead of exiting on the spot: the harness stops every process it started, lets the trial in progress restore any fixture file it edited, removes the disposable fixture copies unless `--keep-fixtures` is set, writes no results, and exits with 130 or 143. A second signal ends it immediately.

The launch paths are the ones the proposal fixes:

| Runtime | Command run in the fixture copy |
| --- | --- |
| Node | `node --import remix/node-tsx server.ts` (adds `--watch` for watch reload) |
| Bun | `bun server.ts` (`bun --watch server.ts`) |
| Bento | `$BENTO_BIN run --allow-read --allow-write=./db --allow-net --allow-env=PORT,DATABASE_URL,NODE_ENV --allow-addons=./node_modules server.ts` (adds `--watch` before `server.ts`) |

`NODE_ENV` is pinned per workload to `development` or `production`.

### Correctness gates

The acceptance checker (`bench/src/acceptance.ts`) runs against a base URL and stops at the first failing gate:

1. `GET /` returns 200 with the whole document: the guest-book heading, the seeded entries in order, the sign form with name and message fields, the import map, the module script, and the stylesheet link. An empty or truncated body fails. This is how Bun's server-side rendering failure (issue #2) appears: Bun answers 200 with no body.
2. Every asset URL the page references returns 200 with a JavaScript or `text/css` content type: stylesheets, the module script, preloads, and every import-map target, including those under `scopes`. In production the URLs are fingerprinted and the scripts are minified.
3. A valid `POST /` returns 303 with `Location: /`, and the new entry then renders after the seeded ones.
4. A submission with whitespace-only fields returns 400 with a field error for each, keeping the submitted values in the form. A 281-character message returns 400 with the length error. Neither adds an entry.

The same checker is meant to be reused by the end-to-end tests of the Bento CLI and the Rust host.

### Reading the results

A run writes `bench/results/<UTC timestamp>/` with:

- `run.json`: when the run started, its options (including the executable chosen for each runtime), and the environment: hardware, OS, runtime versions (or why a runtime is unavailable), tool versions, the `remix` version, and source revisions.
- One raw JSON file per workload, with every sample, every trial failure, and each runtime's outcome in `series[].outcome`: `measured`, `failed`, `incompatible` (with the failing check and the response evidence of the gate), or `unavailable`. Each file repeats the run's options and environment so it can be read on its own.
- `summary.md`: one table per workload and mode, with the median, the range, the interquartile range, and the sample count for each metric. A `p90`, `p95`, or `p99` appears only when the sample count supports it (10, 20, and 100 samples). Below each table, every runtime that did not produce numbers is explained.

A runtime's column shows one of:

- numbers: the metric over the successful trials;
- `incompatible`: it failed a correctness gate or could not be launched. The check, the failures, and the response evidence (method, URL, status, content type, `Location`, body size) are listed under the table; a runtime that could not be launched has no responses, only the error;
- `failed`: every trial failed after the gates passed. Each trial's reason is listed.

Trials alternate runtime order, so no runtime always runs first. Compare medians and look at the spread before drawing a conclusion; a wide interquartile range means the run was noisy and should be repeated.

## Worked example

```sh
export BENTO_BIN=/path/to/bento
node bench/src/main.ts --runtimes node,bun,bento --workloads cold-launch --trials 15
```

→ `bench/results/2026-10-10T23-00-00Z/summary.md` shows, for `cold-launch` in production, a Node column with the median time to listening and to the first correct page, a Bun column reading `incompatible` with the empty-document evidence, and a Bento column with timings once `BENTO_BIN` points at a working build.

Validate the harness itself with tiny counts, writing outside the repository so no placeholder numbers are recorded:

```sh
node bench/src/main.ts --trials 1 --duration 1 --warmup 1 --iterations 2 --output /tmp/bento-bench/results
```

## Limits and edge cases

- The results make no speed promise. Bento is not expected to be faster than Node or Bun. Node's and Bun's asset compilers are native Node-API packages (Oxc and Lightning CSS) under every runtime, so "Rust instead of JavaScript" is not evidence of a speedup; compare numbers only after the gates pass.
- Bun currently fails gate 1 for the full application: Remix 3's server renderer passes a duck-typed signal that Bun's `EventTarget` rejects, and Bun returns an empty page. The `asset-operations` workload does not render the application and still runs for Bun.
- The fixture keeps its asset cache in process memory. There is no persistent-cache scenario, and cold state is only reset by starting a new process and a new database.
- Every helper command the harness runs to completion (copying the fixture, installing it, migrating, the asset-operations probe, and the load generator) has a deadline: ten minutes, except the load generator, which gets its own run's duration plus a minute. A command that outlives it is stopped, its output is reported, and the trial or setup step fails instead of blocking the run.
- Memory is sampled from the process table about every 20 ms, so the peak is a lower bound that can miss a shorter spike. Idle memory is the median of ten samples taken after the server has been quiet for a moment.
- The CSS operation runs on the fixture's single stylesheet and is small; treat its absolute time with care.
- `steady-state` uses the browser-like default of accepting compressed responses, so compression work is part of the page and asset measurements for every runtime.
- The watch-reload entry edit is observed through a response header the harness adds to `server.ts` for the measurement; the edit is reverted afterwards, and the fixture copy is discarded.
- The harness measures one machine at one time. Results from different hardware, operating systems, or background load are not comparable.
