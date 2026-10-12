# Benchmark summary

Started 2026-10-12T00:04:54.601Z. Options and environment are in `run.json`; every sample is in the per-workload JSON files beside this one.

## Environment
- Hardware: Mac16,1, Apple M4, 10 cores, 32 GiB
- OS: macOS 27.0.1 (arm64)
- Runtimes: node v26.11.1, bun 1.4.3, bento sha256:e7f8bbad3bda
- Tools: pnpm 12.11.2, oha 1.16.0
- Revisions: bento ac1679b-dirty, fixture b3c5986
- Fixture: Remix 3.0.0 with its locked package graph

## Cold launch (`cold-launch`)

How long from spawning the process until the port accepts connections, and until the first response that passes the document gate? The second milestone includes application initialization.

### development

| Metric | node | bun | bento |
| --- | --- | --- | --- |
| Spawn to listening (ms) | 189.4 (187.0–209.1, IQR 2.0, p90 191.1, n=15) | incompatible | 301.7 (299.3–318.3, IQR 4.2, p90 307.6, n=15) |
| Spawn to first correct page (ms) | 213.7 (210.8–233.5, IQR 2.1, p90 216.0, n=15) | incompatible | 332.7 (329.2–348.3, IQR 3.9, p90 340.2, n=15) |

**bun** incompatible at the document check: response body is empty
  Response evidence:
  - GET http://127.0.0.1:17703/ → 200, 0 bytes, no content type

### production

| Metric | node | bun | bento |
| --- | --- | --- | --- |
| Spawn to listening (ms) | 193.2 (191.1–195.7, IQR 2.4, p90 194.6, n=15) | incompatible | 306.0 (303.4–337.0, IQR 4.2, p90 327.2, n=15) |
| Spawn to first correct page (ms) | 220.2 (218.2–223.4, IQR 2.3, p90 222.9, n=15) | incompatible | 341.0 (338.1–372.0, IQR 4.7, p90 361.8, n=15) |

**bun** incompatible at the document check: response body is empty
  Response evidence:
  - GET http://127.0.0.1:17737/ → 200, 0 bytes, no content type

## Asset requests (`assets`)

How long does the first request for each script and stylesheet the page references take, compared with repeat requests in the same process that hit the fixture's in-memory cache? The fixture compiles its entry graph while the server starts, so first requests for those assets can already be warm.

### development

| Metric | node | bun | bento |
| --- | --- | --- | --- |
| First request, all scripts (sum) (ms) | 139.5 (135.2–141.7, IQR 2.8, p90 141.6, n=10) | incompatible | 158.5 (149.2–162.5, IQR 9.8, p90 162.3, n=10) |
| First request, all stylesheets (sum) (ms) | 3.6 (3.3–4.8, IQR 0.365, p90 4.3, n=10) | incompatible | 2.5 (2.3–3.1, IQR 0.166, p90 2.9, n=10) |
| Repeat request, all scripts (median sum) (ms) | 27.2 (25.8–28.3, IQR 1.3, p90 27.8, n=10) | incompatible | 36.2 (34.4–38.2, IQR 1.0, p90 37.1, n=10) |
| Repeat request, all stylesheets (median sum) (ms) | 0.453 (0.417–0.506, IQR 0.037, p90 0.485, n=10) | incompatible | 0.602 (0.558–0.626, IQR 0.031, p90 0.625, n=10) |

**bun** incompatible at the document check: response body is empty
  Response evidence:
  - GET http://127.0.0.1:17703/ → 200, 0 bytes, no content type

### production

| Metric | node | bun | bento |
| --- | --- | --- | --- |
| First request, all scripts (sum) (ms) | 33.4 (32.6–36.1, IQR 1.0, p90 34.4, n=10) | incompatible | 44.2 (42.0–45.5, IQR 1.9, p90 45.4, n=10) |
| First request, all stylesheets (sum) (ms) | 2.0 (1.9–2.3, IQR 0.163, p90 2.2, n=10) | incompatible | 2.2 (1.9–2.7, IQR 0.507, p90 2.6, n=10) |
| Repeat request, all scripts (median sum) (ms) | 24.7 (23.8–26.2, IQR 0.807, p90 25.4, n=10) | incompatible | 32.5 (31.0–34.6, IQR 0.692, p90 33.0, n=10) |
| Repeat request, all stylesheets (median sum) (ms) | 0.425 (0.399–0.482, IQR 0.017, p90 0.445, n=10) | incompatible | 0.566 (0.530–0.601, IQR 0.033, p90 0.592, n=10) |

**bun** incompatible at the document check: response body is empty
  Response evidence:
  - GET http://127.0.0.1:17737/ → 200, 0 bytes, no content type

## Asset operations (`asset-operations`)

Where does time go inside the asset path? Each operation is timed over the fixture's own files through the locked npm packages with the options remix/assets uses. The cold pass is the first, with empty caches; warm is the median of the later passes. This workload does not render the application.

| Metric | node | bun | bento |
| --- | --- | --- | --- |
| Parse (oxc-parser), cold pass (ms) | 0.801 (0.775–0.913, IQR 0.030, p90 0.874, n=10) | 0.793 (0.764–0.944, IQR 0.085, p90 0.892, n=10) | 0.811 (0.779–1.3, IQR 0.025, p90 0.881, n=10) |
| Parse (oxc-parser), warm pass (median) (ms) | 0.550 (0.529–0.568, IQR 0.015, p90 0.558, n=10) | 0.492 (0.483–0.530, IQR 0.018, p90 0.517, n=10) | 0.558 (0.524–0.579, IQR 0.014, p90 0.568, n=10) |
| Resolve (oxc-resolver), cold pass (ms) | 2.1 (1.7–2.2, IQR 0.120, p90 2.2, n=10) | 1.9 (1.8–2.1, IQR 0.054, p90 1.9, n=10) | 2.6 (2.4–2.8, IQR 0.112, p90 2.7, n=10) |
| Resolve (oxc-resolver), warm pass (median) (ms) | 0.982 (0.961–1.0, IQR 0.019, p90 0.999, n=10) | 0.882 (0.855–0.903, IQR 0.032, p90 0.900, n=10) | 1.7 (1.7–1.8, IQR 0.018, p90 1.8, n=10) |
| Transform (oxc-transform), cold pass (ms) | 1.7 (1.4–2.1, IQR 0.188, p90 1.9, n=10) | 1.4 (1.4–1.7, IQR 0.067, p90 1.5, n=10) | 1.5 (1.3–1.8, IQR 0.110, p90 1.7, n=10) |
| Transform (oxc-transform), warm pass (median) (ms) | 1.1 (1.0–1.1, IQR 0.030, p90 1.1, n=10) | 1.0 (0.984–1.0, IQR 0.027, p90 1.0, n=10) | 1.1 (1.0–1.1, IQR 0.022, p90 1.1, n=10) |
| Minify (oxc-minify), cold pass (ms) | 1.7 (1.5–1.8, IQR 0.121, p90 1.8, n=10) | 1.5 (1.4–1.7, IQR 0.023, p90 1.6, n=10) | 1.8 (1.6–2.0, IQR 0.137, p90 1.9, n=10) |
| Minify (oxc-minify), warm pass (median) (ms) | 1.2 (1.1–1.2, IQR 0.030, p90 1.2, n=10) | 1.1 (1.1–1.2, IQR 0.027, p90 1.1, n=10) | 1.2 (1.1–1.2, IQR 0.014, p90 1.2, n=10) |
| CSS (lightningcss), cold pass (ms) | 0.497 (0.461–0.559, IQR 0.040, p90 0.531, n=10) | 0.482 (0.460–0.556, IQR 0.032, p90 0.510, n=10) | 0.558 (0.534–0.594, IQR 0.028, p90 0.591, n=10) |
| CSS (lightningcss), warm pass (median) (ms) | 0.167 (0.145–0.187, IQR 0.028, p90 0.181, n=10) | 0.139 (0.132–0.154, IQR 0.006, p90 0.144, n=10) | 0.163 (0.157–0.188, IQR 0.019, p90 0.180, n=10) |

Notes:

- Every measured runtime produced identical outputs for parse, resolve, transform, minify, css (output digests match).

## Steady-state load (`steady-state`)

How many requests per second, at what latency, does a runtime sustain at fixed concurrency after a warmup, for a rendered page, an asset, and a persisted valid submission? Responses must keep their expected status and persisted rows must be counted.

### production

| Metric | node | bun | bento |
| --- | --- | --- | --- |
| Rendered GET /, throughput (req/s) | 120.0 (117.6–121.2, IQR 2.2, n=5) | incompatible | 31.5 (31.5–31.5, IQR 0.043, n=5) |
| Rendered GET /, latency p50 (ms) | 132.6 (131.2–137.2, IQR 3.5, n=5) | incompatible | 504.0 (503.5–504.8, IQR 0.619, n=5) |
| Rendered GET /, latency p99 (ms) | 173.2 (165.9–177.4, IQR 5.4, n=5) | incompatible | 883.3 (522.4–940.8, IQR 109.2, n=5) |
| Asset GET, throughput (req/s) | 5104.1 (4868.1–5134.8, IQR 37.3, n=5) | incompatible | 838.3 (832.2–841.5, IQR 4.4, n=5) |
| Asset GET, latency p50 (ms) | 3.1 (3.0–3.1, IQR 0.014, n=5) | incompatible | 18.2 (18.1–18.2, IQR 0.130, n=5) |
| Asset GET, latency p99 (ms) | 4.5 (4.3–6.2, IQR 0.050, n=5) | incompatible | 31.7 (30.4–31.9, IQR 0.286, n=5) |
| Persisted POST /, throughput (req/s) | 4154.1 (3465.3–4262.5, IQR 124.0, n=5) | incompatible | 3977.5 (3929.8–3997.6, IQR 21.0, n=5) |
| Persisted POST /, latency p50 (ms) | 3.5 (3.4–3.6, IQR 0.018, n=5) | incompatible | 3.9 (3.9–4.0, IQR 0.024, n=5) |
| Persisted POST /, latency p99 (ms) | 8.2 (7.6–19.8, IQR 3.8, n=5) | incompatible | 6.6 (6.3–6.7, IQR 0.248, n=5) |

**bun** incompatible at the document check: response body is empty
  Response evidence:
  - GET http://127.0.0.1:17737/ → 200, 0 bytes, no content type

## Watch reload (`watch-reload`)

With the runtime's watch mode running, how long from editing a file until the first response that shows the change? The transitive edit changes a visible string in a module the entry imports; the entry edit adds a response header in server.ts. The database contents and the listener must survive each reload.

### development

| Metric | node | bun | bento |
| --- | --- | --- | --- |
| Edit of an imported module to changed response (ms) | 438.0 (431.6–466.1, IQR 6.6, p90 453.0, n=10) | incompatible | 431.4 (424.5–535.9, IQR 12.7, p90 532.0, n=10) |
| Edit of the entry to changed response (ms) | 451.5 (436.6–468.7, IQR 10.7, p90 456.1, n=10) | incompatible | 439.7 (434.9–463.3, IQR 7.7, p90 457.8, n=10) |

**bun** incompatible at the document check: response body is empty
  Response evidence:
  - GET http://127.0.0.1:17703/ → 200, 0 bytes, no content type

## Memory (`memory`)

How much resident memory does the whole process tree hold? Peak is the largest sample from spawn through startup and the first request for every asset; idle is the median of samples taken after the server has been quiet for a moment. Sampling is about every 20 ms, so the peak is a lower bound.

### development

| Metric | node | bun | bento |
| --- | --- | --- | --- |
| Peak RSS, startup through first asset compilation (MB) | 221.9 (221.2–223.8, IQR 1.3, p90 223.3, n=10) | incompatible | 208.0 (205.9–210.2, IQR 2.2, p90 209.4, n=10) |
| Idle RSS after startup (MB) | 222.1 (221.5–223.8, IQR 1.2, p90 223.5, n=10) | incompatible | 207.9 (206.2–210.2, IQR 2.1, p90 209.6, n=10) |

**bun** incompatible at the document check: response body is empty
  Response evidence:
  - GET http://127.0.0.1:17703/ → 200, 0 bytes, no content type

### production

| Metric | node | bun | bento |
| --- | --- | --- | --- |
| Peak RSS, startup through first asset compilation (MB) | 177.7 (176.0–180.9, IQR 1.3, p90 178.6, n=10) | incompatible | 157.8 (152.3–161.7, IQR 2.1, p90 158.9, n=10) |
| Idle RSS after startup (MB) | 180.1 (179.0–180.9, IQR 1.2, p90 180.9, n=10) | incompatible | 160.1 (158.2–165.2, IQR 4.6, p90 163.9, n=10) |

**bun** incompatible at the document check: response body is empty
  Response evidence:
  - GET http://127.0.0.1:17737/ → 200, 0 bytes, no content type
