# Bento

This project follows the Workbench process. Workbench loads from `.workbench`, a gitignored link that `mise install` restores: when `.workbench` is missing or dangling, the harness loads no Workbench content and tasks that call `workbench` fail, so run `mise install`.

This file holds only what is true of this project. The process itself comes from the Workbench package.

## Purpose and layout

Bento is an embeddable TypeScript runtime and web toolchain written in Rust: a Cargo workspace plus the TypeScript that supports it. [`ARCHITECTURE.md`](ARCHITECTURE.md) maps the crates; [`VISION.md`](VISION.md) holds the direction.

- `crates/`: the runtime, compiler, host, package, prompt, task, workspace, and CLI crates. `crates/bento_cli` builds the `bento` executable.
- `examples/`: the guest book's Rust embedding host, the Prompt API example, and the direct-source example. `fixtures/remix-guest-book/` is the guest-book application. Both trees hold independent projects with their own installs and lockfiles, outside the gate's formatters and linters.
- `bench/`: the benchmark harness, in TypeScript with Vitest. `bench/results/` holds recorded output and is never reformatted.
- `tools/crate-dependencies.ts`: enforces policy.0001. Node runs it through native type stripping, so it stays erasable TypeScript.
- `docs/`: the Hugo configuration for the record site, which Workbench's theme renders from where the vision, records, and guides live.
- `.agents/skills/writing-rust/`: this repository's Rust conventions. Read it before changing a crate.

## Commands

Beyond Workbench's task interface (`setup`, `check`, `fmt`, `status`, `validate`, and the `beads:*` tasks):

- `mise run rust` is the Rust gate: rustfmt, Clippy, rustdoc, and every crate's tests, on macOS with Apple silicon. It is deliberately outside `mise run check`: the default build needs macOS 27 with SDK 27 and the suite takes minutes, so CI runs it on its macOS runners and you run it whenever a change touches a crate. Only `cargo fmt --check` is cheap enough for every commit, and hk runs it.
- `mise run build` builds the CLI; `mise run dev -- <arguments>` runs it from source, such as `mise run dev -- run --allow-read=. main.ts`.
- `mise run package` builds the default CLI's release archive, `artifacts/bento-aarch64-apple-darwin.tar.gz`, with `--release --locked`, and fails unless the binary declares `minos 27.0`. `mise run package -- backend-free` builds `artifacts/bento-backend-free-aarch64-apple-darwin.tar.gz` with `--no-default-features`. Each archive holds `bento` under one top-level directory.
- `mise run release` runs `knope release` under Varlock with only `GITHUB_TOKEN` loaded: it tags `v<version>` and publishes the GitHub release with both archives from `artifacts/`. Only `release.yml` runs it.
- `mise run lint`, `mise run typecheck`, and `mise run test` cover the TypeScript: Oxlint, tsrs (and tsc for the Prompt API example), `node --test` for `tools/`, and Vitest for `bench/`. `mise run check` runs all three.
- `mise run deps:policy` checks that no crate depends on the SWC stack; `mise run check` runs it.
- `mise run docs:build` builds the record site into `docs/public/`; `mise run docs:dev` serves it.

## Platform rules

- The upstream Deno crates are pinned as a set to exactly what shipped with the Deno version named in `Cargo.toml`. Change them together, never one at a time.
- No crate may depend on the SWC stack: [policy.0001](policies/0001-prefer-voidzero-crates.md).
- `rust-toolchain.toml` pins the Rust channel, and rustup reads it for every `cargo` invocation. `mise.toml` pins the same version; change both together.
- macOS 27 with SDK 27 is the default build target. Backend-free builds (`--no-default-features`, without the Foundation Models prompt backend) keep the earlier requirements, and CI proves both.
- Clippy stays out of the git hooks: CI's Linux `check` job runs `hk check --all`, and Bento does not build on Linux.

## Releases

Knope owns the version (`crates/bento_cli/Cargo.toml` and `Cargo.lock`), `CHANGELOG.md`, and releases, configured in `knope.toml`. On every push to `main`, `.github/workflows/prepare-release.yml` keeps one pull request open from the `release` branch with the next version and its changelog entry, built from the change files in `.changeset/`. Merging that pull request is the release authorization: `.github/workflows/release.yml` runs `mise run check`, builds, smoke-tests, and attests both archives through `build-archives.yml` on the runners CI uses (`xcode-27` for the default build, `macos-26` for the backend-free build), then runs `mise run release`. Bento publishes no Homebrew formula and no crates.io or npm package. To prove both release builds before merging a release pull request, run Actions → Build archives → Run workflow.

## Work tracking

Beads holds the work graph. Bento's user-visible defects that outlive a pull request are also GitHub issues, because the README and the guides link them for people who do not run `bd`. Record new work in Beads; touch an issue only to keep an already-public one honest.

## Previews

- **The record and the guides:** `.github/workflows/preview.yml` publishes the site for each pull request under `pr-preview/pr-<number>/` on `gh-pages` and comments the link.
- **The runtime:** the macOS CI jobs upload installable `bento` archives (`bento-macos27-arm64-prompt` and `bento-macos26-arm64-backend-free`). Installing and running one is the real act for a CLI change.
