# Analysis of the 2026-10-12 run

This run measured Node 26.11.1, Bun 1.4.3, and Bento on the final candidate build of proposal.0001, one workload at a time on an otherwise idle Apple M4 (details in `run.json` and `summary.md`). It is evidence, not a performance claim; the proposal sets no speed target. It repeats the [2026-10-11 run](../2026-10-11T07-11-43Z/ANALYSIS.md) against the build that review produced.

## What was measured

- **Bento build.** `bento` was installed with `cargo install --locked` (release profile) from candidate commit `ac1679b`, which takes `deno_core` from the pinned `bento-js/deno_core` fork. The recorded revision `ac1679b-dirty` is that commit; "dirty" reflects an untracked Beads export, not source changes. The binary's SHA-256 prefix is in `run.json`.
- **Correctness first.** Node and Bento passed every gate in both modes. Bun again returned a `200` with an empty body for the rendered page in every application workload (issue #2), so it has no application timings; its asset-operation results are valid because that workload does not render the application. The asset-operations gate now also fails a runtime when any timed native call reports an error; no runtime did.
- **Native compilers are the same code.** In `asset-operations`, the locked `oxc-parser`, `oxc-transform`, `oxc-minify`, and `lightningcss` addons run at about the same speed under all three runtimes and produce identical output digests. `oxc-resolver` stays slower under Bento (warm 1.7 ms vs 0.98 ms).

## Findings

| Workload | Node | Bento | Reading |
| --- | --- | --- | --- |
| Cold launch, spawn to first correct page (production) | 220 ms | 341 ms | About 120 ms slower; the gap is already present when the port starts listening (193 vs 306 ms). |
| Watch reload, imported-module edit to changed response | 438 ms | 431 ms | Parity with `node --watch`. |
| Peak RSS through first asset compilation (production) | 178 MB | 158 MB | The whole process tree, including native compilers. |
| Persisted `POST /` throughput | 4154 req/s | 3978 req/s | Near parity: validation, SQLite write, and redirect with no response body. |
| Rendered `GET /` throughput | 120 req/s | 31.5 req/s | Dominated by Brotli response compression (issue #14). |
| Asset `GET` throughput | 5104 req/s | 838 req/s | Same cause. |

### Compared with the 2026-10-11 run

Node's numbers moved by a few percent between runs. Bento's steady-state, memory, and asset-operation numbers are unchanged, but its startup is slower: spawn to listening rose from 251 to 306 ms and spawn to first correct page from 282 to 341 ms in production, and the watch-reload restart, which includes a startup, rose from 384 to 431 ms. The review rounds between the two builds added work on every startup and filesystem check. The likely contributors are per-component path canonicalization for permission checks on paths that do not exist (the asset pipeline probes many missing `node_modules` directories), the CommonJS resolution hook that tracks watch inputs, and the filesystem and feature operation wrappers. This attribution is an inference; the run does not isolate them. Issue #244 tracks it.

## Reproducing

```sh
cargo install --locked --path crates/bento_cli --root <dir>
env -u NODE_PATH BENTO_BIN=<dir>/bin/bento node bench/src/main.ts
```

`NODE_PATH` must be unset so the fixture's native resolver stays inside the fixture copy. The run takes about eleven minutes with the default trial counts and requires `oha`, Node, Bun, and pnpm 12.
