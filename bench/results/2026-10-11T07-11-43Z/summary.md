# Benchmark summary

Started 2026-10-11T07:11:43.782Z. Options and environment are in `run.json`; every sample is in the per-workload JSON files beside this one.

## Environment
- Hardware: Mac16,1, Apple M4, 10 cores, 32 GiB
- OS: macOS 27.0.1 (arm64)
- Runtimes: node v26.11.1, bun 1.4.3, bento sha256:55ef5f931f27
- Tools: pnpm 12.11.2, oha 1.16.0
- Revisions: bento 8c6abd2-dirty, fixture b3c5986
- Fixture: Remix 3.0.0 with its locked package graph

## Cold launch (`cold-launch`)

How long from spawning the process until the port accepts connections, and until the first response that passes the document gate? The second milestone includes application initialization.

### development

| Metric | node | bun | bento |
| --- | --- | --- | --- |
| Spawn to listening (ms) | 177.7 (174.9–184.4, IQR 3.3, p90 181.5, n=15) | incompatible | 244.3 (240.9–341.5, IQR 4.7, p90 261.1, n=15) |
| Spawn to first correct page (ms) | 200.1 (198.5–209.6, IQR 3.2, p90 205.4, n=15) | incompatible | 271.1 (267.5–371.2, IQR 5.1, p90 288.8, n=15) |

**bun** incompatible at the document check: response body is empty
  Response evidence:
  - GET http://127.0.0.1:17703/ → 200, 0 bytes, no content type

### production

| Metric | node | bun | bento |
| --- | --- | --- | --- |
| Spawn to listening (ms) | 183.7 (182.5–185.5, IQR 1.5, p90 185.2, n=15) | incompatible | 251.4 (248.3–276.4, IQR 3.3, p90 266.3, n=15) |
| Spawn to first correct page (ms) | 210.2 (208.9–212.2, IQR 1.2, p90 211.8, n=15) | incompatible | 281.7 (278.7–309.0, IQR 3.5, p90 297.0, n=15) |

**bun** incompatible at the document check: response body is empty
  Response evidence:
  - GET http://127.0.0.1:17737/ → 200, 0 bytes, no content type

## Asset requests (`assets`)

How long does the first request for each script and stylesheet the page references take, compared with repeat requests in the same process that hit the fixture's in-memory cache? The fixture compiles its entry graph while the server starts, so first requests for those assets can already be warm.

### development

| Metric | node | bun | bento |
| --- | --- | --- | --- |
| First request, all scripts (sum) (ms) | 136.6 (133.1–160.8, IQR 6.0, p90 142.2, n=10) | incompatible | 154.3 (147.9–177.7, IQR 3.9, p90 157.4, n=10) |
| First request, all stylesheets (sum) (ms) | 3.2 (3.1–5.0, IQR 0.131, p90 3.6, n=10) | incompatible | 2.3 (2.2–2.5, IQR 0.079, p90 2.3, n=10) |
| Repeat request, all scripts (median sum) (ms) | 25.1 (24.6–27.5, IQR 0.600, p90 26.3, n=10) | incompatible | 35.6 (34.2–36.5, IQR 0.787, p90 36.2, n=10) |
| Repeat request, all stylesheets (median sum) (ms) | 0.427 (0.384–0.473, IQR 0.027, p90 0.452, n=10) | incompatible | 0.560 (0.546–0.602, IQR 0.024, p90 0.597, n=10) |

**bun** incompatible at the document check: response body is empty
  Response evidence:
  - GET http://127.0.0.1:17703/ → 200, 0 bytes, no content type

### production

| Metric | node | bun | bento |
| --- | --- | --- | --- |
| First request, all scripts (sum) (ms) | 32.4 (30.4–39.7, IQR 2.9, p90 35.9, n=10) | incompatible | 42.6 (41.0–71.1, IQR 0.923, p90 47.2, n=10) |
| First request, all stylesheets (sum) (ms) | 2.0 (1.8–3.0, IQR 0.482, p90 2.7, n=10) | incompatible | 2.0 (1.9–2.2, IQR 0.095, p90 2.1, n=10) |
| Repeat request, all scripts (median sum) (ms) | 23.6 (22.1–24.3, IQR 0.776, p90 24.1, n=10) | incompatible | 32.1 (31.1–33.5, IQR 0.920, p90 33.0, n=10) |
| Repeat request, all stylesheets (median sum) (ms) | 0.403 (0.356–0.443, IQR 0.044, p90 0.431, n=10) | incompatible | 0.545 (0.507–0.568, IQR 0.023, p90 0.560, n=10) |

**bun** incompatible at the document check: response body is empty
  Response evidence:
  - GET http://127.0.0.1:17737/ → 200, 0 bytes, no content type

## Asset operations (`asset-operations`)

Where does time go inside the asset path? Each operation is timed over the fixture's own files through the locked npm packages with the options remix/assets uses. The cold pass is the first, with empty caches; warm is the median of the later passes. This workload does not render the application.

| Metric | node | bun | bento |
| --- | --- | --- | --- |
| Parse (oxc-parser), cold pass (ms) | 0.732 (0.704–0.885, IQR 0.077, p90 0.822, n=10) | 0.698 (0.652–0.825, IQR 0.025, p90 0.758, n=10) | 0.741 (0.683–0.958, IQR 0.032, p90 0.830, n=10) |
| Parse (oxc-parser), warm pass (median) (ms) | 0.510 (0.498–0.531, IQR 0.011, p90 0.531, n=10) | 0.472 (0.459–0.492, IQR 0.025, p90 0.489, n=10) | 0.504 (0.495–0.524, IQR 0.010, p90 0.516, n=10) |
| Resolve (oxc-resolver), cold pass (ms) | 1.8 (1.6–2.0, IQR 0.056, p90 2.0, n=10) | 1.7 (1.6–1.9, IQR 0.045, p90 1.9, n=10) | 2.4 (2.3–2.7, IQR 0.121, p90 2.5, n=10) |
| Resolve (oxc-resolver), warm pass (median) (ms) | 0.917 (0.890–0.976, IQR 0.050, p90 0.960, n=10) | 0.868 (0.847–0.889, IQR 0.015, p90 0.883, n=10) | 1.6 (1.5–1.6, IQR 0.017, p90 1.6, n=10) |
| Transform (oxc-transform), cold pass (ms) | 1.3 (1.3–1.8, IQR 0.064, p90 1.5, n=10) | 1.4 (1.3–1.5, IQR 0.055, p90 1.4, n=10) | 1.5 (1.3–1.7, IQR 0.093, p90 1.6, n=10) |
| Transform (oxc-transform), warm pass (median) (ms) | 0.883 (0.858–1.0, IQR 0.051, p90 0.969, n=10) | 1.0 (0.990–1.1, IQR 0.031, p90 1.1, n=10) | 0.936 (0.876–0.982, IQR 0.043, p90 0.971, n=10) |
| Minify (oxc-minify), cold pass (ms) | 1.6 (1.4–1.8, IQR 0.113, p90 1.7, n=10) | 1.5 (1.3–1.7, IQR 0.098, p90 1.6, n=10) | 1.7 (1.5–1.9, IQR 0.125, p90 1.8, n=10) |
| Minify (oxc-minify), warm pass (median) (ms) | 0.983 (0.968–1.0, IQR 0.013, p90 1.0, n=10) | 1.1 (1.1–1.1, IQR 0.037, p90 1.1, n=10) | 0.986 (0.953–1.0, IQR 0.031, p90 1.0, n=10) |
| CSS (lightningcss), cold pass (ms) | 0.435 (0.414–0.586, IQR 0.031, p90 0.499, n=10) | 0.447 (0.421–0.505, IQR 0.036, p90 0.490, n=10) | 0.522 (0.486–0.709, IQR 0.029, p90 0.561, n=10) |
| CSS (lightningcss), warm pass (median) (ms) | 0.139 (0.128–0.158, IQR 0.012, p90 0.156, n=10) | 0.129 (0.121–0.133, IQR 0.005, p90 0.132, n=10) | 0.141 (0.131–0.157, IQR 0.011, p90 0.154, n=10) |

Notes:

- Every measured runtime produced identical outputs for parse, resolve, transform, minify, css (output digests match).

## Steady-state load (`steady-state`)

How many requests per second, at what latency, does a runtime sustain at fixed concurrency after a warmup, for a rendered page, an asset, and a persisted valid submission? Responses must keep their expected status and persisted rows must be counted.

### production

| Metric | node | bun | bento |
| --- | --- | --- | --- |
| Rendered GET /, throughput (req/s) | 121.3 (120.1–121.8, IQR 0.652, n=5) | incompatible | 31.5 (31.5–31.6, IQR 0.036, n=5) |
| Rendered GET /, latency p50 (ms) | 131.0 (129.7–131.4, IQR 0.451, n=5) | incompatible | 503.5 (502.9–504.7, IQR 0.752, n=5) |
| Rendered GET /, latency p99 (ms) | 171.8 (170.9–178.4, IQR 4.7, n=5) | incompatible | 851.3 (822.6–910.1, IQR 55.5, n=5) |
| Asset GET, throughput (req/s) | 5335.1 (5327.8–5340.9, IQR 12.5, n=5) | incompatible | 841.2 (763.3–844.1, IQR 3.0, n=5) |
| Asset GET, latency p50 (ms) | 2.9 (2.9–2.9, IQR 0.005, n=5) | incompatible | 18.2 (18.0–19.6, IQR 0.146, n=5) |
| Asset GET, latency p99 (ms) | 4.1 (4.0–4.1, IQR 0.014, n=5) | incompatible | 31.0 (30.2–38.3, IQR 0.513, n=5) |
| Persisted POST /, throughput (req/s) | 4127.4 (2920.8–4286.7, IQR 39.3, n=5) | incompatible | 3908.5 (3798.3–4005.8, IQR 64.1, n=5) |
| Persisted POST /, latency p50 (ms) | 3.5 (3.4–3.8, IQR 0.014, n=5) | incompatible | 3.9 (3.8–4.0, IQR 0.036, n=5) |
| Persisted POST /, latency p99 (ms) | 8.3 (7.7–29.6, IQR 0.195, n=5) | incompatible | 6.7 (6.2–9.8, IQR 0.464, n=5) |

**bun** incompatible at the document check: response body is empty
  Response evidence:
  - GET http://127.0.0.1:17737/ → 200, 0 bytes, no content type

## Watch reload (`watch-reload`)

With the runtime's watch mode running, how long from editing a file until the first response that shows the change? The transitive edit changes a visible string in a module the entry imports; the entry edit adds a response header in server.ts. The database contents and the listener must survive each reload.

### development

| Metric | node | bun | bento |
| --- | --- | --- | --- |
| Edit of an imported module to changed response (ms) | 434.3 (432.7–449.9, IQR 4.2, p90 439.5, n=10) | incompatible | 384.1 (381.5–387.0, IQR 2.7, p90 385.6, n=10) |
| Edit of the entry to changed response (ms) | 448.5 (437.8–484.4, IQR 11.1, p90 459.4, n=10) | incompatible | 391.7 (379.6–410.3, IQR 7.6, p90 398.2, n=10) |

**bun** incompatible at the document check: response body is empty
  Response evidence:
  - GET http://127.0.0.1:17703/ → 200, 0 bytes, no content type

## Memory (`memory`)

How much resident memory does the whole process tree hold? Peak is the largest sample from spawn through startup and the first request for every asset; idle is the median of samples taken after the server has been quiet for a moment. Sampling is about every 20 ms, so the peak is a lower bound.

### development

| Metric | node | bun | bento |
| --- | --- | --- | --- |
| Peak RSS, startup through first asset compilation (MB) | 222.2 (221.4–224.5, IQR 1.8, p90 224.2, n=10) | incompatible | 206.4 (205.0–208.6, IQR 1.9, p90 208.3, n=10) |
| Idle RSS after startup (MB) | 222.1 (221.4–223.7, IQR 1.5, p90 223.6, n=10) | incompatible | 206.1 (205.0–208.9, IQR 2.1, p90 208.8, n=10) |

**bun** incompatible at the document check: response body is empty
  Response evidence:
  - GET http://127.0.0.1:17703/ → 200, 0 bytes, no content type

### production

| Metric | node | bun | bento |
| --- | --- | --- | --- |
| Peak RSS, startup through first asset compilation (MB) | 178.6 (176.9–180.0, IQR 0.527, p90 179.5, n=10) | incompatible | 156.7 (154.1–159.6, IQR 2.1, p90 158.7, n=10) |
| Idle RSS after startup (MB) | 179.8 (178.7–180.4, IQR 0.484, p90 180.3, n=10) | incompatible | 161.2 (157.6–163.1, IQR 3.1, p90 161.8, n=10) |

**bun** incompatible at the document check: response body is empty
  Response evidence:
  - GET http://127.0.0.1:17737/ → 200, 0 bytes, no content type
