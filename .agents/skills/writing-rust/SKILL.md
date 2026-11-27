---
name: writing-rust
description: Use when writing, reviewing, or restructuring Rust in Bento — adding or changing a crate, changing the embedding API of bento_runtime or bento_host, writing Node or Deno compatibility code, writing unsafe, FFI, or Node-API code, silencing a Clippy lint, choosing an error type, adding a Cargo feature or touching the backend-free build, adding a dependency, or writing a test.
---

# Writing Rust

**Write Rust that makes the correct behavior easy to express, the incorrect behavior difficult to express, and the implemented behavior easy to verify.**

`references/rust-style-guide.md` is the full house guide: API design, ownership, errors and panics, traits, concurrency, unsafe, documentation, testing, performance and dependencies. This file is only what is specific to this repository. For the crate design and its boundaries, read `ARCHITECTURE.md`; for what hosts and guests rely on, `guides/embedding-bento.md` and `guides/node-compatibility.md`.

## Layout

| Thing                                                                                        | Where                                                                         |
| -------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------- |
| The `bento` executable: arguments, reporting, exit codes, OS signals                         | `crates/bento_cli`                                                            |
| The embedding API: `start`, `RuntimeConfig`, `RunningRuntime`, `RuntimeHandle`, `HostModule` | `crates/bento_runtime/src/lib.rs` and what it re-exports                      |
| The authority an instance receives: `Grants`, `Scope`, `NetTarget`, `canonical_path`         | `crates/bento_host`                                                           |
| Worker assembly over Deno's `deno_lib` and `deno_runtime`                                    | `crates/bento_runtime/src/assembly.rs`, `workers.rs`                          |
| Bento's replacements for upstream Deno ops                                                   | `crates/bento_runtime/src/ops.rs` (`middleware`), `feature_ops.rs`, `fs_ops/` |
| TypeScript, JSX, and `tsconfig.json`, through Oxc                                            | `crates/bento_compiler`                                                       |
| Project discovery, `package.json`, launch targets, environment files                         | `crates/bento_workspace`                                                      |
| Task planning and execution                                                                  | `crates/bento_task`                                                           |
| `bento install` through pnpm                                                                 | `crates/bento_package`                                                        |
| The Web Prompt API and its Foundation Models backend                                         | `crates/bento_prompt`; the Deno adapter is `src/deno/`                        |
| Test helpers more than one crate needs                                                       | `crates/bento_test_support`                                                   |
| A real embedding host, built and tested with the workspace                                   | `examples/guestbook-host`                                                     |

`ARCHITECTURE.md`'s crate table is the target design; the crates above are the ones that exist. Keep its dependency rules for them: `bento_host` depends on no toolchain or CLI crate, `bento_workspace` needs no JavaScript engine, and `bento_cli` is a thin composition layer over the libraries.

A new crate goes in `crates/`, which the workspace's `members` glob picks up. It inherits `edition`, `license`, `publish`, `repository`, and `rust-version` with `.workspace = true`, sets `[lints] workspace = true`, and gets a path entry in `[workspace.dependencies]` once another crate depends on it.

Name what you import; the only glob is a test module's `use super::*`. `rustfmt.toml` sorts imports within blank-line-separated groups but cannot create them, so write three groups by hand: std, external crates, then `crate::` and workspace crates.

## Verification

```sh
cargo fmt --all --check   # rustfmt; builds nothing, so it runs on any platform
mise run rust             # the Rust gate: rustfmt, Clippy, rustdoc, every crate's tests
```

Of the Rust tools, only rustfmt runs in the git hooks and in `mise run check`: hk's `cargo-fmt` step runs `cargo fmt --all --check` in the pre-commit hook (fixing and staging), in `hk check --all`, and so in CI's Linux `check` job. `mise run fmt` formats in place.

`mise run rust` runs `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo doc --workspace --no-deps`, and `cargo test --workspace --no-fail-fast`, with `MACOSX_DEPLOYMENT_TARGET=27.0` and `RUSTDOCFLAGS=-D warnings`. It needs macOS 27 with SDK 27 on Apple silicon, because the default build compiles the Foundation Models backend. It is outside `mise run check`, and no hook runs Clippy; CI's `rust` job runs it on the `xcode-27` runner. Run it whenever a change touches a crate.

When a change touches feature-gated code, also run the backend-free checks that CI's `rust-backend-free` job runs on `macos-26`:

```sh
cargo clippy --workspace --all-targets --no-default-features -- -D warnings
RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-default-features --no-deps
cargo test --workspace --no-default-features --no-fail-fast
cargo test --locked -p bento_prompt --no-default-features --lib
```

A change to `bento_prompt` also needs CI's standalone checks: `cargo check --locked -p bento_prompt --no-default-features --lib`, and, with `MACOSX_DEPLOYMENT_TARGET=27.0`, `cargo test --locked -p bento_prompt --features foundation-models --lib` and `cargo build --locked -p bento_prompt --features foundation-models --example prompt_session`.

Never substitute a weaker command (`cargo check` for Clippy, one package for `--workspace`), and never report a check as passing unless it ran.

## Lints

Every workspace member sets `[lints] workspace = true`, taking `[workspace.lints]` from the root `Cargo.toml`. The gate denies warnings in Clippy and rustdoc, so a warning fails it.

| Lint                                         | Table    | Why it is on                                                         |
| -------------------------------------------- | -------- | -------------------------------------------------------------------- |
| `unreachable_pub`                            | `rust`   | an item no public path reaches is `pub(crate)` or `pub(super)`       |
| `undocumented_unsafe_blocks`                 | `clippy` | every `unsafe` block carries its `// SAFETY:` comment                |
| `allow_attributes_without_reason`            | `clippy` | a silenced lint says why                                             |
| `cast_possible_truncation`, `cast_sign_loss` | `clippy` | casts that silently lose data                                        |
| `missing_errors_doc`, `missing_panics_doc`   | `clippy` | a public fallible or panicking function has `# Errors` or `# Panics` |

`bento_runtime`, `bento_host`, and `bento_prompt`, the crates hosts program against, also set `#![warn(missing_docs)]`.

Silence a lint on the narrowest item with `#[expect(lint, reason = "…")]`. An `#[expect]` fails the gate once the lint stops firing, so the exception cannot outlive its cause. Two cases differ:

- A lint that fires in only some builds gets an expectation scoped to those builds, as in `crates/bento_prompt/src/engine/mod.rs`: `#[cfg_attr(not(any(native_backend, test)), expect(dead_code, reason = "…"))]`. A bare `#[expect]` would be unfulfilled in the other builds.
- Helpers in an integration-test support module (`tests/support/mod.rs`, `tests/common/mod.rs`) compile into several test binaries that each use a different subset, so they take `#[allow(dead_code, reason = "…")]`. Use `#[allow]` nowhere else. The workspace does not enable `clippy::allow_attributes`, so review enforces this, not Clippy.

Fix a cast rather than silence it: `try_from`, or a method that names the intended reinterpretation (`cast_unsigned()` in `crates/bento_runtime/src/fs_ops/stat.rs`, `to_le_bytes()[0]` for exit codes in `crates/bento_cli/src/error.rs`). Expect `cast_possible_truncation` only where the code has just proved the value fits, as `integral` in `crates/bento_prompt/src/schema.rs` does.

Never loosen `[workspace.lints]` or a crate's lint levels to make a warning go away.

## Errors

- Library crates return concrete error types: `<Domain>Error` enums deriving `thiserror::Error`, whose variants carry the context that explains them, such as the path and the underlying error (`ManifestError`, `ProjectError`, `TargetError`, `PlanError`, `TaskError`, `InstallError`). Where callers must tell problems apart, each gets its own variant, with a `<Domain>Problem` enum for what was wrong (`ManifestProblem`, `ShapeProblem`, `DotenvProblem`). Never make a caller compare message text.
- A failure the host, the guest, or the file system can cause returns an error: encoding a grant ceiling returns `UnencodableCeiling` instead of panicking on a path that is not UTF-8. A panic marks a broken invariant, with an `expect` message stating it (`close_resources` in `crates/bento_runtime/src/assembly.rs`).
- `bento_runtime` reports failures to hosts as `RuntimeError`. Its `ErrorKind` (`Startup`, `Resolution`, `Transform`, `Permission`, `Uncaught`) keeps the stages distinguishable, and its `message` is plain text without terminal escapes. Pick the kind by the stage that failed; adding a kind changes the embedding API.
- `anyhow` stays inside `bento_runtime`, where Deno's worker assembly uses it (`assembly.rs`, `permissions.rs`), and becomes a `RuntimeError` before it reaches a host.
- `bento_cli` wraps each library error in `CliError` with `#[error(transparent)]` and `#[from]`. `CliError::report` writes it to stderr, `error.rs` holds the one rule for turning a status into an exit code, and `main` returns `ExitCode`.
- A public fallible function's `# Errors` section names the variants it returns (`Manifest::read`).

## The embedding API

Hosts program against `bento_runtime` and `bento_host`, never by shelling out to `bento`; `bento_prompt` serves hosts that want the model without a runtime.

- `bento_runtime` and `bento_host` keep their modules private and re-export the public surface from `lib.rs` with `pub use`.
- Keep Deno and V8 types out of that surface. `ARCHITECTURE.md` lets a crate delegate to `deno_runtime` internally but not make it a permanent public contract, and keeps engine-specific handle types out of engine-independent interfaces.
- Library crates print nothing; they return what a host may want to show. Configuration diagnostics come back from `RunningRuntime::diagnostics()` and failures as `RuntimeError`, and the CLI decides what reaches stderr.
- A change to the surface updates `guides/embedding-bento.md`; the `no_run` example in `crates/bento_runtime/src/lib.rs`'s crate docs, which compiles the guide's example; and `examples/guestbook-host`, which `cargo test --workspace` builds and tests.

## Node and Deno compatibility code

The engine, the Web and Node APIs, and Node-API come from the Deno v2.9.7 crate set (`crates/bento_runtime/Cargo.toml`). Bento owns runtime construction, grants, and containment.

- Grant and capability checks happen in Rust, at the op boundary. JavaScript wrappers are not the security boundary (`ARCHITECTURE.md`).
- Change an upstream op's behavior by replacing the op: write a `bento_*` `#[op2]` function and map the upstream op's name to it in `middleware` in `crates/bento_runtime/src/ops.rs`, or in `feature_ops::replacement` or `fs_ops::replacement`. The replacement takes the upstream name, so upstream JavaScript calls it unchanged. This is how an instance stays contained: `op_exit` ends the instance, `op_fs_chdir` is refused, and the environment and signal ops work on the instance's own state, never the host process's. An embedded instance registers no OS signal handlers.
- A refusal a guest sees is a `JsErrorBox` with a class it can catch: `NotCapable` for authority the grants or the host withhold, `NotSupported` for an operation or feature Bento does not offer guests (`ops.rs`, `feature_ops.rs`). Never disguise a denial as a missing file.
- Embed extension JavaScript with `ExtensionFileSource::new(…, ascii_str_include!(…))` in the extension's `customizer`, never `esm = [dir …]`, which reads the file from the build machine at run time and stops a moved binary from starting (`feature_ops.rs`, `ops.rs`, `crates/bento_prompt/src/deno/mod.rs`).
- Unit-test an op by registering it in a test-only `deno_core::extension!` (`crates/bento_runtime/src/fs_ops/tests.rs`).
- Record a deliberate departure from Node's behavior under "Node compatibility decisions" in `guides/node-compatibility.md`.

## Unsafe, FFI, and Node-API

- Bento's own `unsafe` is calls into libc and dyld: `killpg` in `crates/bento_task/src/process_group.rs`, and signal and loaded-image queries in tests. The engine and the Node-API implementation, and their unsafe code, come from upstream crates.
- Every `unsafe` block has a `// SAFETY:` comment directly above it saying why the call's preconditions hold here. When a call's result is deliberately ignored, the comment above it also says why (`process_group.rs`).
- Declare foreign functions in an `unsafe extern "C"` block, as `crates/bento_runtime/tests/addon_denial.rs` does.
- An executable that can load Node-API addons must export the ABI symbols, or the linker strips them. Take `deno_napi` as a build dependency and call `deno_napi::print_linker_flags("<binary>")` from its `build.rs` (`crates/bento_cli/build.rs`, `examples/guestbook-host/build.rs`). Test executables that embed the runtime export `crates/bento_runtime/napi-symbols-macos.def` instead, through `cargo:rustc-link-arg-tests=-Wl,-exported_symbols_list,…` (`crates/bento_runtime/build.rs`). That list is Deno v2.9.7's, kept verbatim, and hk excludes it from every step.
- No C-compatible embedding surface exists yet. `ARCHITECTURE.md` sets the rules for the first one: it wraps runtime handles and never exposes Rust references, `deno_core` internals, or engine pointers across the ABI.

## Features and build variants

Bento ships two builds, and CI builds and tests both:

| Build        | Selected by                                                                                                                                              | Needs                                                               |
| ------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------- |
| Default      | `bento_cli`'s default `prompt-foundation-models`, which enables `bento_runtime/prompt-foundation-models`, which enables `bento_prompt/foundation-models` | Apple silicon, macOS 27 and SDK 27, `MACOSX_DEPLOYMENT_TARGET=27.0` |
| Backend-free | `--no-default-features`                                                                                                                                  | no macOS 27 floor; CI runs it on `macos-26`                         |

- `bento_runtime`'s `prompt-foundation-models` stays off by default: embedders opt in (`guides/embedding-bento.md`).
- The Prompt API adapter is installed in both builds, so guests see one JavaScript surface; only the native backend is optional, and a build without it reports the model unavailable. Never gate guest-visible API on the feature.
- Gate native-backend code on `#[cfg(native_backend)]`, which `crates/bento_prompt/build.rs` sets when `foundation-models` is on and the target is macOS on aarch64; never repeat that condition. The same script refuses an SDK or `MACOSX_DEPLOYMENT_TARGET` below 27.
- The deployment target matters at the final link: at Rust's default macOS floor the linker selects Swift's back-deployment concurrency library, which the executable cannot resolve at run time. `crates/bento_cli/build.rs` passes `-mmacosx-version-min=27.0` to the `bento` binary when the feature is on, and CI checks that the binary declares `minos 27.0`.
- `bento_prompt` with no features is a standalone core with no native or engine dependencies. Its `deno` feature adds the `deno_core` adapter, which `bento_runtime` enables.
- Code compiled into only one build must still pass Clippy with `-D warnings` in the other; Lints shows how to scope an expectation to a build.

## Dependencies

- The upstream runtime crates in `[workspace.dependencies]` (`deno_*`, `node_resolver`) are pinned with `=` to exactly the set published with Deno v2.9.7. Change them together, never one at a time.
- `deno_core` itself comes from `bento-js/deno_core` through `[patch.crates-io]` at a fixed revision: the published crate plus one fix, carried as separate commits (see `PATCHES.md` there). A new `deno_core` version needs the fork moved to it, or the patch no longer applies.
- policy.0001: parsing, transformation, source maps, minification, module resolution, and bundling use VoidZero crates (`oxc`, `oxc_resolver`). No crate may declare a direct dependency on `deno_ast`, `deno_graph`, `dprint-swc-ext`, or any `swc` or `swc_*` crate; `mise run deps:policy`, part of `mise run check`, fails if one does. When only a Deno crate can fill a role, say why next to the dependency in that crate's `Cargo.toml`, as `crates/bento_runtime/Cargo.toml` does for `node_resolver`.
- A third-party crate that more than one member uses is declared once in `[workspace.dependencies]` and inherited with `.workspace = true`, so a bump cannot miss a copy. One used by a single crate (`ariadne`, `notify`) stays in that crate's manifest.
- `bento_test_support` is only ever a `[dev-dependencies]` entry.
- Read the comment above a dependency or profile before changing it; several exist for build reasons rather than code. `deno_napi` as a build dependency exports the Node-API symbols, `deno_core` as a build dependency of `bento_runtime` gives the V8 snapshot build its engine feature, and `[profile.dev.package.v8]` stays at `opt-level = 1` because rusty_v8 miscompiles below it.
- `rust-toolchain.toml` and the `rust` pin in `mise.toml` name the same toolchain; change both together. Never change `edition` or `rust-version` incidentally.

## Tests

- Unit tests sit beside the code in `#[cfg(test)] mod tests`, inline or as `tests.rs` in the module's directory (`crates/bento_cli/src/watch/tests.rs`, `crates/bento_runtime/src/fs_ops/tests.rs`).
- Integration tests are `crates/<crate>/tests/<behavior>.rs`, one test binary each, written against the public API. Helpers for one crate's test binaries live in its `tests/support/` (`tests/common/` in `bento_compiler`).
- A helper more than one crate needs goes in `bento_test_support` (the HTTP client, free ports, `isolated_fixture_test`), never into a second copy. Where callers differ on purpose, the difference is an argument (`ChildLimit`).
- Tests that run the CLI start `env!("CARGO_BIN_EXE_bento")` (`crates/bento_cli/tests/support/mod.rs`).
- A test that runs a fixture instance inside the test process wraps its body in `isolated_fixture_test`, which reruns the test in a child process without `NODE_PATH`. The fixture's native resolver reads the host process's `NODE_PATH` and would follow it outside the project.
- Name a test for the behavior it protects, never for the proposal that introduced it: `real_fixture_renders_assets_validates_persists_and_restarts_in_both_modes`.
- Run `cargo test` with `--no-fail-fast`, as `mise run rust` does, so one failing test binary does not hide another.

## Anti-patterns — stop if you are about to

- Bump one Deno crate on its own, or add a crate from the SWC stack.
- Add `#[allow]` outside a shared test-support module, or silence a lint without a reason.
- Print from a library crate, or let a Deno or V8 type into the embedding API.
- Enforce a grant only in JavaScript.
- Load extension JavaScript with `esm = [dir …]`.
- Gate guest-visible API on `prompt-foundation-models`.
- Copy a test helper into a second crate.
- Add `.clone()`, `Arc`, `Mutex` or `Box` to make a borrow error go away instead of deciding the ownership.
- `unwrap()` or `expect()`, outside tests, on anything a guest, a host, or the file system supplied.
- Report `mise run rust` or the backend-free checks as passing without running them, or claim correctness because it compiled.
