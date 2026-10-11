# Analysis of the 2026-10-11 run

This run measured Node 26.11.1, Bun 1.4.3, and Bento on the integration build of proposal.0001, one workload at a time on an otherwise idle Apple M4 (details in `run.json` and `summary.md`). It is evidence, not a performance claim; the proposal sets no speed target.

## What was measured

- **Bento build.** `bento` was installed with `cargo install --locked --path crates/bento_cli` (release profile) from integration commit `7e8341a`. The recorded revision `8c6abd2-dirty` is that commit plus a guide-only change; "dirty" reflects an untracked Beads export, not source changes. The binary's SHA-256 prefix is in `run.json`.
- **Correctness first.** Node and Bento passed every gate in both modes. Bun returned a `200` with an empty body for the rendered page in every application workload (issue #2), so it has no application timings; its asset-operation results are valid because that workload does not render the application.
- **Native compilers are the same code.** In `asset-operations`, the locked `oxc-parser`, `oxc-transform`, `oxc-minify`, and `lightningcss` Node-API addons run at the same speed under all three runtimes and produce identical output digests. `oxc-resolver` is slower under Bento (warm 1.6 ms vs 0.92 ms), a difference in the addon's filesystem access path that this run does not explain further.

## Findings

| Workload | Node | Bento | Reading |
| --- | --- | --- | --- |
| Cold launch, spawn to first correct page (production) | 210 ms | 282 ms | About 70 ms slower; the gap is already present when the port starts listening (184 vs 251 ms), so it lies in process startup and module loading, not first-request rendering. |
| Watch reload, imported-module edit to changed response | 434 ms | 384 ms | Bento's in-process restart is faster than `node --watch`'s process restart. |
| Idle RSS (production) | 180 MB | 161 MB | The whole process tree, including native compilers. |
| Persisted `POST /` throughput | 4127 req/s | 3909 req/s | Near parity: validation, SQLite write, and redirect with no response body. |
| Rendered `GET /` throughput | 121 req/s | 31.5 req/s | Dominated by Brotli response compression; see below. |
| Asset `GET` throughput | 5335 req/s | 841 req/s | Same cause. |

### Steady-state GET is limited by Brotli compression

`oha` advertises Brotli by default, so the fixture's `compression()` middleware compressed every rendered page and asset with Brotli. A follow-up probe on the same build and fixture, varying only `Accept-Encoding`:

| Request                  | Node, `br` | Bento, `br` | Node, `gzip` | Bento, `gzip` |
| ------------------------ | ---------- | ----------- | ------------ | ------------- |
| Rendered `GET /` (req/s) | 122.8      | 33.3        | 596.5        | 486.8         |
| Asset `GET` (req/s)      | 5072       | 851         | 9424         | 5685          |

With gzip, rendering is within about 20% of Node. With Brotli, both runtimes slow sharply, and Bento's `node:zlib` Brotli path, inherited from its Node-compatibility layer, is several times slower than Node's. The probe used 300-request (page) and 1000-request (asset) bursts at concurrency 16, not the full steady-state protocol, so it identifies the cause rather than replacing the recorded numbers. This is tracked as a follow-up rather than changed here.

## Reproducing

```sh
cargo install --locked --path crates/bento_cli --root <dir>
env -u NODE_PATH BENTO_BIN=<dir>/bin/bento node bench/src/main.ts
```

`NODE_PATH` must be unset so the fixture's native resolver stays inside the fixture copy. The run takes about eleven minutes with the default trial counts and requires `oha`, Node, Bun, and pnpm 12.
