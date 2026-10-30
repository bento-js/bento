![Bento: a bento box whose compartments hold a terminal prompt, code brackets, a globe, and a database](assets/bento-banner.png)

# Bento

Bento is a TypeScript runtime written in Rust. It is a library that runs JavaScript, TypeScript, JSX, and TSX applications on V8 with Node and Web APIs, plus a `bento` command built on that library. The application that embeds the runtime decides which files, network endpoints, environment variables, and native addons the code may reach; a script gets nothing it was not granted.

The long-term aim is one execution model shared by a development server, a production process, a coding agent's scripts, and a native app, with type checking, formatting, linting, bundling, testing, and package installation built around it. [VISION.md](VISION.md) explains the direction and [ARCHITECTURE.md](ARCHITECTURE.md) sketches the planned crates.

## Status

Bento is pre-release. There are no published binaries or packages: you build it from this repository, and it runs only on macOS with Apple silicon.

The first feature set, [proposal 0001](proposals/0001-embeddable-typescript-runtime.md), was implemented and merged on 2026-10-12. Its acceptance target is a real application: a Remix 3 guest book, pinned to `remix@3.0.0`, that runs from its TypeScript source through the `bento` CLI and through a Rust program embedding the runtime in-process. What works today:

- `bento run` executes a source file directly. Oxc transforms honor the project's `tsconfig.json`, resolution follows Node's rules (`package.json` `imports`, `exports`, and `type`, CommonJS and ESM interop, pnpm layouts), and Node-API addons load for real, so Remix's native asset pipeline runs unmodified.
- Grants are explicit: scoped `--allow-read`, `--allow-write`, `--allow-net`, `--allow-env`, and `--allow-addons`, plus the boolean `--allow-language-model` capability. Nothing is granted by default and nothing prompts.
- `bento run <task>` runs the top-level `tasks` in `package.json`, with dependencies, argument forwarding, and cancellation of the whole process group. `.env` files are layered by mode.
- `bento run --watch` restarts when the loaded source graph or its configuration changes, and recovers when a broken file is fixed.
- `bento install` installs dependencies through the project's declared pnpm.
- The `bento_runtime` crate lets a Rust program start an instance, expose its own `host:` modules, deliver signals, shut down within a deadline, and interrupt JavaScript that will not stop.
- `LanguageModel` provides on-device text sessions and constrained JSON through macOS Foundation Models. The reusable `bento_prompt` crate works without V8; its Deno adapter and native backend are separate features. The default CLI build requires macOS 27 and SDK 27, while backend-free builds retain their earlier requirements.

On an Apple M4, the guest book handles persisted form submissions at 3978 requests per second under Bento and 4154 under Node, and cold-starts in 341 ms against Node's 220 ms. Rendered pages run at 31.5 requests per second against Node's 120, because Brotli compression is slow under Bento ([#14](https://github.com/bento-js/bento/issues/14)). The [recorded results](bench/results/2026-10-12T00-04-54Z/ANALYSIS.md) include Bun, which cannot render the app yet ([#2](https://github.com/bento-js/bento/issues/2)).

Most guides end with a section on limits. Open defects from the first feature set are [#14](https://github.com/bento-js/bento/issues/14), [#18](https://github.com/bento-js/bento/issues/18), [#242](https://github.com/bento-js/bento/issues/242), [#243](https://github.com/bento-js/bento/issues/243), and [#244](https://github.com/bento-js/bento/issues/244), the last a 55 ms cold-start regression.

## Try it

The default CLI build needs macOS 27.0 or newer on Apple silicon, Xcode with SDK 27 or newer, and the Rust toolchain pinned in `rust-toolchain.toml`, which `rustup` selects automatically. Installing a project's dependencies also needs pnpm 12 and Node, because `bento install` delegates to pnpm and some package tools are Node scripts. Running the installed project needs only Bento. Use `--no-default-features` when installing a backend-free CLI without the Foundation Models SDK dependency.

```sh
cargo install --locked --path crates/bento_cli
```

Run the guest book with the grants it needs:

```sh
cd fixtures/remix-guest-book
bento install --frozen-lockfile
env -u NODE_PATH bento run --allow-read --allow-write=./db --allow-net \
  --allow-env=PORT,DATABASE_URL,NODE_ENV --allow-addons=./node_modules dev
```

Open <http://localhost:1612>. The `dev` task migrates the SQLite database once and then runs `bento run --watch server.ts`, so editing `app/actions/guest-book/page.tsx` restarts the server. `NODE_PATH` is unset because the native Oxc resolver reads it from the process and would look for packages outside the project.

A smaller example runs TypeScript and TSX with an npm import and top-level await:

```sh
cd examples/direct-source
bento install --frozen-lockfile
bento run --allow-read=. main.ts
```

To embed the runtime instead, read [Embedding Bento](guides/embedding-bento.md) and the [guest-book host](guides/guest-book-embedding-example.md), which serves the same app with its storage owned by Rust.

## Documentation

- [Running with Bento](guides/running-with-bento.md): `bento run`, grants, watch mode, exit codes, and error reports.
- [Tasks and environment](guides/tasks-and-environment.md): `package.json` tasks, `.env` files, and nested `bento` launches.
- [Installing dependencies](guides/installing-dependencies.md): `bento install` and pnpm.
- [TypeScript and JSX](guides/typescript-and-jsx.md): which `tsconfig.json` settings Bento honors.
- [Node compatibility](guides/node-compatibility.md): the Node and Web surface exercised so far, and how grants apply to it.
- [Embedding Bento](guides/embedding-bento.md): the Rust API, host modules, and lifecycle.
- [Using the Prompt API](guides/prompt-api.md): local model grants, text and JSON sessions, cancellation, and Rust reuse.
- [Guest-book embedding example](guides/guest-book-embedding-example.md): a complete Rust host.
- [Benchmarks](guides/benchmarks.md): the Node, Bun, and Bento harness and how to read its results.

## Repository layout

| Path | Contents |
| --- | --- |
| `crates/bento_runtime` | The embeddable runtime: module loading, Node and Web APIs, Node-API addons, grants, host modules, lifecycle |
| `crates/bento_prompt` | Engine-independent text sessions, the optional macOS Foundation Models backend, and the optional Deno adapter |
| `crates/bento_cli` | The `bento` command |
| `crates/bento_host` | Grants, scopes, canonical paths, and network targets |
| `crates/bento_compiler` | Oxc transforms and `tsconfig.json` settings |
| `crates/bento_workspace` | Project discovery, targets, and `.env` loading |
| `crates/bento_task` | Task graphs, nested launches, and process-group cancellation |
| `crates/bento_package` | Installation through pnpm |
| `examples/guestbook-host` | A Rust host embedding the guest book |
| `examples/direct-source` | A TypeScript and TSX example |
| `examples/prompt-api` | Native CLI, permission-denial, worker-cancellation, shutdown, and idle-session previews |
| `fixtures/remix-guest-book` | The Remix 3 application used for acceptance and benchmarks |
| `bench` | The benchmark harness and recorded results |
| `guides`, `proposals`, `policies`, `decisions` | Documentation and the project's design record |

The runtime is assembled from Deno 2.9.7's crates behind Bento-owned interfaces. Its `deno_core` patch lives in the [bento-js/deno_core](https://github.com/bento-js/deno_core) fork. The optional model backend uses a pinned [bento-js/foundation-models-rs](https://github.com/bento-js/foundation-models-rs) fork for cancellable asynchronous streaming and typed macOS 27 errors.

## Roadmap

The [Bento Roadmap](https://github.com/orgs/bento-js/projects/1) project holds 226 issues sorted into Now, Next, and Later. It is the source of truth; this summary reflects it on 2026-10-12.

### Now

- The rest of the MVP that proposal 0001 began: the [Rust embedding API](https://github.com/bento-js/bento/issues/238), [host capability grants](https://github.com/bento-js/bento/issues/239), and a [self-contained production process](https://github.com/bento-js/bento/issues/241) are in progress. [Feature-selected builds](https://github.com/bento-js/bento/issues/240) have not started.
- [Web API compatibility](https://github.com/bento-js/bento/issues/19) and [Node compatibility](https://github.com/bento-js/bento/issues/64): 27 Web APIs and 32 Node modules are scheduled now, 28 of them already in progress. Still to start are items such as WebSocket, Web Crypto, `node:crypto`, `node:child_process`, and `node:stream`.
- Compilation: lowering and minification for [ECMAScript](https://github.com/bento-js/bento/issues/134) and [CSS](https://github.com/bento-js/bento/issues/143), CSS import resolution, and a built-in [dynamic asset server](https://github.com/bento-js/bento/issues/167).
- [Package management](https://github.com/bento-js/bento/issues/182): npm and jsr support, on-demand `npm:` and `jsr:` imports, monorepo workspaces, and modern `.npmrc` options, with [workspace-aware tasks](https://github.com/bento-js/bento/issues/179).
- The WinterTC [Functions API](https://github.com/bento-js/bento/issues/133), and defects [#2](https://github.com/bento-js/bento/issues/2), [#14](https://github.com/bento-js/bento/issues/14), and [#18](https://github.com/bento-js/bento/issues/18).

### Next

- A [Vite-compatible development server with SSR](https://github.com/bento-js/bento/issues/168), already in progress, and a [Vitest-compatible test runner](https://github.com/bento-js/bento/issues/169).
- [Type checking](https://github.com/bento-js/bento/issues/139) and [bundling](https://github.com/bento-js/bento/issues/141), plus [HTML](https://github.com/bento-js/bento/issues/151), [Markdown](https://github.com/bento-js/bento/issues/157), and [MDX](https://github.com/bento-js/bento/issues/161) tooling.
- A [formatter](https://github.com/bento-js/bento/issues/209), with 18 of its 25 languages and features scheduled next, among them JavaScript, TypeScript, CSS, HTML, Markdown, JSON, YAML, and import sorting.
- [Cloudflare Workers compatibility](https://github.com/bento-js/bento/issues/111) (Durable Objects, KV, Queues, D1, R2, Cron Triggers, static assets, HTMLRewriter) and the rest of [WinterTC](https://github.com/bento-js/bento/issues/127).
- [Environment variables and secrets](https://github.com/bento-js/bento/issues/180), [toolchain self-management](https://github.com/bento-js/bento/issues/181), [task caching](https://github.com/bento-js/bento/issues/175), [package catalogs](https://github.com/bento-js/bento/issues/187), and [HMR for Remix JSX](https://github.com/bento-js/bento/issues/204).

### Later

- A [linter](https://github.com/bento-js/bento/issues/235), including cross-file and type-aware rules.
- Framework support for [React](https://github.com/bento-js/bento/issues/189) (React Compiler, Fast Refresh, and the Fizz and Flight renderers), [Angular](https://github.com/bento-js/bento/issues/195), [Solid](https://github.com/bento-js/bento/issues/198), [Astro](https://github.com/bento-js/bento/issues/201), [Lit](https://github.com/bento-js/bento/issues/205), and [GraphQL](https://github.com/bento-js/bento/issues/207).
- [Image processing](https://github.com/bento-js/bento/issues/164), [Tailwind CSS compilation](https://github.com/bento-js/bento/issues/149), and [publishing packages with provenance](https://github.com/bento-js/bento/issues/185).
- Browser-oriented Web APIs such as WebGPU, WebCodecs, Web Audio, and IndexedDB.

Some directions in [VISION.md](VISION.md#beyond-the-minimum), such as JavaScriptCore-backed embedding on Apple platforms, are not on the board yet.

## Development

Work in this repository follows [PROCESS.md](PROCESS.md), and [AGENTS.md](AGENTS.md) is the operative contract. Substantial changes start as a proposal in [`proposals/`](proposals/). Tests come before code, guides are derived from the proposal, and work is reviewed independently before it merges. [Policy 0001](policies/0001-prefer-voidzero-crates.md) requires VoidZero's crates (Oxc, `oxc_resolver`, Rolldown) for parsing, transformation, resolution, and bundling, and `mise run check` enforces it.

[mise](https://mise.jdx.dev) owns the toolchains and every task; `mise.toml` pins Node, pnpm, Bun, and the Rust channel that `rust-toolchain.toml` also names. [hk](https://hk.jdx.dev) owns the git hooks, and [Beads](https://github.com/gastownhall/beads) (`bd`) holds the work graph.

```sh
mise run setup    # one-time bootstrap after a clone: dependencies, work tracker, git hooks
mise run status   # which phase the current work is in, and what comes next
mise run check    # formatting, lint, types, artifact validation, tests, and the docs build
mise run rust     # rustfmt, clippy and rustdoc with warnings denied, and every crate's tests (macOS arm64)
```
