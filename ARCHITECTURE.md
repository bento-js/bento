---
title: Architecture of Bento
status: Proposed
updated: 2026-10-09
---

# Architecture of Bento

## Overview

Bento is a Rust-based, fully integrated TypeScript runtime and web application toolchain. It provides one environment for executing applications, developing and building them, and embedding JavaScript or TypeScript execution inside native software.

Bento is designed for five related uses: a local development environment; a production runtime for conventional servers; an embeddable Node-compatible execution library; a scripting environment for agentic development harnesses; and an extension runtime for native applications, including iOS apps.

The unifying abstraction is **an embeddable runtime with host-controlled capabilities**. The command-line interface, development server, test runner, and package manager are consumers of that runtime and of shared project infrastructure—not prerequisites for embedding it.

Bento builds on existing Rust implementations wherever possible: Deno's runtime and compatibility crates, `v8x` for alternative JavaScript engines, Oxc and Rolldown for compilation, `tsrs` for TypeScript semantics, pnpm for dependency installation, and specialized tools for CSS and Markdown. These are architectural inputs, not guarantees that their upstream interfaces already satisfy Bento's requirements.

## Goals and scope

Bento must:

- Execute modern JavaScript and TypeScript with ESM, Web APIs, and broad Node.js compatibility.
- Run conventional HTTP applications on platforms such as Railway and Fly.io without requiring a separate Node.js installation.
- Offer a Rust embedding interface whose host controls filesystem, networking, environment, subprocesses, module access, and runtime lifecycle.
- Support multiple runtime instances in a larger application, including scoped scripts used by coding agents.
- Support JavaScriptCore-backed execution on Apple platforms, especially iOS, without assuming that all Node APIs are available there.
- Integrate type checking, formatting, linting, bundling, testing, development serving, CSS, Markdown/MDX, dependency installation, and task execution through a coherent project model.
- Compile into feature-selected binaries that omit unnecessary tooling and native capabilities.
- Retain the conventional JavaScript ecosystem's manifests, dependency semantics, and configuration formats wherever possible.

Bento is **not** a new JavaScript language, a new TypeScript syntax, a replacement for the npm ecosystem, or a new distributed Durable Objects platform. Compatibility with Vite, Vitest, Node, and workerd is an explicit engineering target, not an assumed consequence of reusing their dependencies.

## Architectural principles

1. **Embedding is fundamental.** Any operation needed by the CLI must be available through a non-CLI Rust API where practical.
2. **Compatibility is behavioral.** Match observable semantics and test suites, not just module names or public type signatures.
3. **Native by default, interoperable when necessary.** Use Rust implementations for hot paths, while allowing isolated JavaScript plugin execution where ecosystem compatibility requires it.
4. **One project view, specialized internal representations.** Share files, module identity, revisions, and invalidation; do not force Oxc and `tsrs` to share an AST or type graph.
5. **Host authority is explicit.** JavaScript does not acquire ambient OS access merely because an API exists.
6. **Engine differences are visible.** V8 is the reference engine; JavaScriptCore and other engines are independently validated backends.
7. **Features compose without leaking.** A production or embedded runtime does not depend on a dev server, package installer, formatter, or type checker.
8. **Upstream reuse is deliberate.** Preserve useful Deno, pnpm, Oxc, and other crates behind Bento-owned interfaces so upstream changes do not define Bento's public API.

## System architecture

```text
             Native hosts and command-line consumers
    +------------+------------+------------+-------------+
    |  Bento CLI |  Servers   |   Hive X   |   iOS apps  |
    +------------+------------+------------+-------------+
                         |            |
                 Bento public Rust / C APIs
                         |
       +-----------------+--------------------+
       |                                      |
       v                                      v
  Project & toolchain                  Execution runtime
  -------------------                  -----------------
  Workspace / file graph              Runtime lifecycle
  Resolver & dependencies             Module loader
  Oxc / Rolldown                       Web / Node / Worker APIs
  tsrs / Oxlint / Oxfmt                Host capability adapters
  Vite / Vitest                        Isolates / event loop
  CSS / Markdown                       Deno extensions
  Tasks / pnpm / Mise                  deno_core
       |                                      |
       +--------- shared module identity ----+
                                              |
                                             v8x
                                              |
                                   +----------+---------+
                                   |          |         |
                                  V8         JSC    QuickJS-ng
```

This is a dependency and responsibility diagram, not a claim that all components share one process, event loop, or engine instance. In particular, the tooling graph is a service usable without starting application execution, while a minimal runtime can execute prebuilt modules without constructing the tooling graph.

## Crate and interface boundaries

The following are proposed Bento-owned crates, not existing published packages.

| Crate | Responsibility | Dependency rule |
| --- | --- | --- |
| `bento_host` | Host capabilities, policy, virtual resources, cancellation, limits | No dependency on toolchain or CLI |
| `bento_engine` | Engine selection, adapter verification, engine-specific hooks | Encapsulates `v8x`; no application policy |
| `bento_runtime` | Runtime construction, execution, isolates, lifecycle, module loading | Depends on core and host interfaces; not on dev tooling |
| `bento_compat_web` | Web-standard globals and operations | Calls host capabilities for privileged operations |
| `bento_compat_node` | Node globals, built-ins, CJS interop, npm resolution adapters | Uses Deno compatibility crates; no CLI dependency |
| `bento_compat_workerd` | Optional Workers-style request handlers and service bindings | No distributed storage or placement engine |
| `bento_resolve` | Canonical module specifiers, conditions, package/URL resolution | Usable by runtime and tooling |
| `bento_workspace` | Files, overlays, revisions, dependency graph, invalidation | No JavaScript engine required |
| `bento_compiler` | Oxc transforms, Rolldown builds, CSS, Markdown/MDX, source maps | Consumes workspace snapshots |
| `bento_intelligence` | `tsrs`, diagnostics, LSP, lint and format coordination | Does not own runtime execution |
| `bento_dev` | HTTP dev server, HMR, module runner, plugin host | May embed a runtime; not the reverse |
| `bento_test` | Test discovery, workers, assertions, mocking and coverage | Uses runtime APIs and workspace snapshots |
| `bento_package` | pnpm-backed package installation, registries, lockfiles | Not required for already-resolved production modules |
| `bento_task` | Task graphs, environment injection, caching, process execution | Uses host process capabilities |
| `bento_cli` | Argument parsing, configuration assembly, user interface | Thin composition layer over the libraries |
| `bento_ffi` | Stable C-compatible embedding surface for Swift and other hosts | Wraps runtime handles, never exposes raw engine pointers |

Crate names describe separation of responsibility. Internally, some may initially delegate to `deno_runtime` or other upstream assemblies; they must not expose those implementations as a permanent public contract.

## Execution runtime

### Deno-derived substrate

Bento uses `deno_core` for JavaScript execution, native operations, resource handling, modules, and event-loop integration. Selected Deno extensions supply Web APIs, system APIs, and Node compatibility. `deno_runtime` can provide an initial reference assembly, but Bento owns runtime construction, embedding, policy, and public interfaces.

The boundary between JavaScript and native capabilities is the Rust operation layer. Permission and capability checks occur at that boundary; JavaScript wrappers are not the security boundary.

### Engine strategy

`v8x` supplies a `rusty_v8`-compatible abstraction over engine implementations.

- **V8:** Reference backend for desktop development and conventional production servers. It establishes the principal Node/Deno compatibility baseline.
- **JavaScriptCore:** Preferred engine for Apple-system integration and the proposed iOS build. Linux JSC support is a target subject to adapter and packaging validation, not an assumption.
- **QuickJS-ng:** Optional lightweight backend where its feature coverage and performance suit the host.

An engine is selected for a compiled runtime artifact, not swapped arbitrarily between isolates at runtime. Engine-independent Bento interfaces must not expose V8-specific handle types, snapshots, or inspector implementations.

Every backend requires differential testing for microtasks, module evaluation, garbage collection, promises, native callbacks, exceptions, WebAssembly, snapshots, and inspector behavior. A successful build of `deno_core` against `v8x` does not prove Node or Web API parity.

### Runtime lifecycle and concurrency

A runtime is constructed with a module loader, a compatibility profile, host services, and limits. It owns one or more engine isolates and their event-loop integration. An embedding host can create, run, cancel, dispose, and observe runtimes through command-oriented handles.

Engine-bound values remain on their owning execution thread or isolate. Cross-thread callers use message passing and serializable results rather than exchanging engine-native handles. Host-managed resources have explicit lifetime, cancellation, and disposal rules. Unhandled rejections, pending tasks, and shutdown behavior must be consistent and observable.

An illustrative Rust API (not a finalized signature):

```rust
let runtime = BentoRuntime::builder()
    .profile(RuntimeProfile::Node)
    .host(host_services)
    .module_loader(module_loader)
    .limits(resource_limits)
    .build()?;

let result = runtime.execute_module("./main.ts").await?;
runtime.shutdown().await?;
```

The runtime's module loader may compile TypeScript through an optional transform service in development. A minimal production runtime can instead consume pretransformed JavaScript and omit Oxc entirely.

## Host capability model

A `BentoHost` provides privileged operations through narrowly scoped interfaces: filesystem, network, environment, clocks, random sources, processes, terminal/stdio, module and package access, storage, and host-defined functions. Runtime policies determine which capabilities a script can access and with what scope.

| Runtime host | Filesystem | Network | Process execution |
| --- | --- | --- | --- |
| Desktop CLI | Native, policy-controlled | Native, policy-controlled | Optional native subprocesses |
| Production server | Native/container or virtual | Configured server networking | Optional and explicitly granted |
| Hive X | Workspace-scoped or virtual | Allowlisted or mediated | Harness-owned tool invocation |
| iOS application | App container or virtual filesystem | App-mediated access | No general subprocess capability |
| Worker-style profile | Binding/storage adapters | Fetch-like network model | Unavailable unless explicitly emulated |

Availability and authority are separate: a module such as `node:fs` may resolve successfully even when its operations are denied by policy. Unsupported platform facilities fail with documented capability or compatibility errors; Bento must not pretend they work.

Permissions, quotas, and execution limits are complementary. Isolate memory limits and interrupt hooks are not a complete defense against untrusted native code, denial of service, or engine vulnerabilities. High-risk workloads require an OS process or stronger isolation boundary. Native addons and arbitrary FFI are disabled in constrained hosts unless specifically trusted.

## Module loading and compatibility

### One resolution interface

`bento_resolve` defines canonical module identities and a common resolution service for execution, compilation, editor intelligence, and testing. It supports:

- ESM and CommonJS, including Node's package `exports`, `imports`, conditions, and interop behavior.
- Relative and absolute file specifiers; `node:` built-ins; bare packages; `npm:` and, where enabled, `jsr:` and `http(s):` specifiers.
- Import maps, package manifests, symlinks, workspaces, and TypeScript file extensions.
- Configurable condition sets for Node, browser, development, production, and Worker-style targets.

Installation and resolution are different operations. An `npm:` specifier resolves to a versioned package identity; the package service may acquire missing artifacts in a permitted developer environment. Offline, embedded, and production contexts use a pre-resolved cache, embedded assets, or a supplied filesystem and must not silently fetch dependencies.

TypeScript `paths` aliases are applied only in contexts that opt into those mappings. Bento must not silently interpret build-tool aliases as native Node resolution rules.

### API profiles

Profiles select API surfaces and host adapters; they do not replace the engine or security policy.

- **Web:** Standard server-relevant APIs, including fetch, URL, streams, text encoding, crypto, timers, and WebSocket interfaces where supported.
- **Node:** Web APIs plus Node built-ins, globals, CommonJS behavior, process semantics, and compatibility with npm packages.
- **Worker:** Web-first request handling and explicitly configured Workers-style bindings, with Node compatibility only to the extent that target semantics permit it.
- **Custom:** A host-selected set of modules, globals, and capabilities for constrained embeddings.

The Node layer preferentially adapts Deno's `deno_node` and companion crates. Node's `vm`, `worker_threads`, `async_hooks`, N-API/native addons, inspector, process APIs, and event-loop behavior require explicit compatibility validation; several may be restricted or unavailable on non-V8 engines and on iOS.

### Worker compatibility is not distributed infrastructure

Bento can provide local Workers-style APIs and build targets for `workerd` or `celld`. Durable Objects, KV, D1, R2, queues, and similar services are host bindings or external deployment services. Replication, durable storage ownership, distributed scheduling, and orchestration remain outside the core runtime.

Compatibility with `workerd` is tested as a target rather than inferred from shared Web API names. Code that uses Node-specific facilities may require a different deployment target or a restricted compatibility mode.

## Shared project infrastructure

The `bento_workspace` service owns a coherent view of a project:

- Files and virtual editor/agent overlays, identified by stable canonical paths and content revisions.
- Project manifests, workspace membership, resolved module identities, and import/dependency relationships.
- Content hashes, cache keys, invalidation, and snapshots representing a consistent set of edits.
- Diagnostics and artifact provenance (source revision, transform chain, and maps).

Oxc, Rolldown, `tsrs`, the dev server, linting, and tests subscribe to changes in this view. A group of agent-generated changes can be committed as one workspace revision, so tools do not run against partially applied edits.

**Shared graph does not mean shared AST.** Oxc retains its syntax trees and transform caches; `tsrs` retains TypeScript-compatible binding and semantic representations. Adapters share file identity, versions, module resolution inputs, and mapping metadata. This avoids rewriting a mature type checker around a different compiler's internal data structures.

Content mappers connect generated, embedded, and original source positions for TypeScript diagnostics and language features. The `tsrs` integration should support content mapping without discarding its declaration emit, source maps, or language server functionality.

## Integrated tooling

| Responsibility | Primary implementation | Bento integration |
| --- | --- | --- |
| Parse, resolve, transform, minify | Oxc | Shared compiler APIs and workspace revisions |
| Format | Oxfmt | Native formatter with project-aware configuration |
| Lint | Oxlint | Native rules plus JS rule/plugin compatibility when supported |
| Type check and language intelligence | `tsrs` | Diagnostics, declarations, LSP, source content mapping |
| Bundle and package build | Rolldown | In-process Rust API with a tsdown-style interface |
| Dev server and HMR | Rust-native Vite-compatible layer | Workspace graph, module runner, plugin host |
| Test runner | Rust-native Vitest-compatible layer | Runtime-backed workers, mock/coverage adapters |
| CSS | Lightning CSS | Transforms, compatibility targeting, minification |
| Markdown / MDX | Sätteri | Rust parser/compiler plus compatible JS plugin bridge |
| Dependency installation | pnpm 12 Rust crates | Lockfile, registry, store, workspace operations |
| Tasks and environments | Mise-derived concepts | Project task graph, tool paths, environment injection |

### Development server

`bento_dev` owns HTTP serving, file watching, module transformation, dependency prebundling, HMR, Fast Refresh integration, and SSR module execution. It consumes the shared workspace and Rolldown/Oxc services rather than launching a second independent graph.

Vite compatibility covers observable configuration and plugin behavior—not only the dev-server protocol. JavaScript plugins execute in a persistent Bento-powered plugin host when compatible; explicit Node fallback or bridging is permissible for unsupported APIs. Plugins must be isolated from application code where host authority differs. OJ and Vite+ are reference implementations to reuse or study, not mandatory Bento dependencies.

### Test runner

`bento_test` provides discovery, isolated test execution, scheduling, assertions, snapshots, mocking, fake timers, coverage, and watch mode. Vitest compatibility includes its test-file imports, configuration, plugin integrations, worker isolation, and mock semantics to the extent verified by differential tests. A Rust scheduler alone does not guarantee Vitest compatibility. Test execution uses explicit runtime profiles and sandbox policies.

### Compiler and CSS/Markdown pipelines

Oxc handles TypeScript stripping and JS/TS transformations; Rolldown handles graph-aware builds. `tsrs` performs semantic checking and declaration output where required. Lightning CSS and Sätteri contribute Rust-native processing, with JavaScript plugin hooks surfaced through the same controlled plugin execution service.

The production runtime should not require any of these tools if build outputs have already been prepared.

## Dependencies, tasks, and configuration

`package.json` is Bento's primary project entry point. Bento recognizes standard `scripts`, `dependencies`, `devDependencies`, `exports`, `imports`, `workspaces`, and `packageManager` fields. Bento-specific options live under a namespaced `bento` field rather than replacing existing standards.

An illustrative configuration:

```json
{
    "name": "example",
    "type": "module",
    "packageManager": "pnpm@12.0.0",
    "scripts": { "start": "bento run src/server.ts" },
    "bento": {
        "runtime": { "profile": "node" },
        "tasks": {
            "check": { "dependsOn": ["lint", "types"] },
            "types": { "command": "bento typecheck" },
            "lint": { "command": "bento lint" }
        }
    }
}
```

The exact schema is to be specified independently of the underlying crates. Existing files such as `pnpm-workspace.yaml`, `pnpm-lock.yaml`, `yarn.lock`, `package-lock.json`, `vite.config.ts`, and `tsconfig.json` are recognized according to their native semantics. Reading another package manager's lockfile does not imply that Bento can install from it with full fidelity; unsupported operations must either delegate to the declared manager or fail clearly, rather than rewrite it silently.

pnpm 12 provides the preferred installation and store implementation. `npm:` loading is routed through resolution and the package service, with deterministic lockfile and offline behavior. Mise concepts inform task dependencies, environment interpolation, tool-path injection (including `node_modules/.bin`), and caching, without making Mise itself a mandatory runtime dependency.

The CLI exposes coherent commands such as `bento run`, `dev`, `test`, `typecheck`, `lint`, `fmt`, `build`, `install`, and `task`; these map onto the same underlying libraries available to embedders.

## Build configurations and deployment

Bento distinguishes **compiled feature sets** from **runtime profiles**.

Compiled features determine whether a binary contains an engine backend, transforms, type checker, package manager, dev server, test runner, Node extensions, or platform adapters. Runtime profiles choose which APIs and capabilities an instance exposes from those available features.

| Artifact | Contents | Intended use |
| --- | --- | --- |
| Full | Runtime plus compiler, type checker, package manager, formatting, linting, dev, test, and tasks | Development workstation |
| Server | Runtime, Web/Node APIs, server host, prebuilt module loader | Containerized production deployment |
| Embedded | Runtime, selected modules, policy and host adapter APIs | Agent harnesses and native software |
| Apple/iOS | System JSC adapter, constrained runtime, selected APIs, Swift-facing ABI | App-integrated scripting and extensions |

Production bundles may include generated code and assets alongside a minimal executable, or ship as files loaded at runtime. Feature selection must permit building the runtime without a dependency on TypeScript checking or package installation. Modules requiring dynamic TypeScript compilation need an explicitly enabled transform capability.

The iOS form must use platform-permitted execution facilities and respect App Store restrictions on downloaded executable code and feature-changing extensions. General-purpose Node native addons, subprocesses, and unrestricted filesystem access are not part of the iOS compatibility promise.

## Embedding contracts

Rust embedders work through `bento_runtime` and `bento_host` rather than shelling out to `bento run`. The API supports construction and destruction of independent runtimes, asynchronous evaluation, preloaded or virtual modules, host function registration, structured result exchange, cancellation, runtime limits, logging, and diagnostics.

Hive X can inject a virtual project filesystem, expose approved agent tools as host functions, and apply separate capabilities to individual script instances. User-authored scripts run as guests; they do not inherit the harness process's own filesystem, credentials, or network authority.

Apple embedding uses a stable C-compatible handle API with Swift wrappers. It must not leak Rust references, `deno_core` internals, or engine handles across the ABI. Host callbacks, scheduling, cancellation, and value ownership have documented rules. The runtime can run on an appropriate dedicated executor while the app retains control of UI-thread interactions.

## Security and observability

The trust boundary is the host capability layer, backed by Rust-side enforcement. Each runtime has explicit grants for resources, module origins, network destinations, environment variables, and process features. Trusted desktop and server configurations can expose broad access; untrusted extensions receive narrowly scoped access.

Bento should expose structured errors, exceptions, async-operation spans, module-resolution traces, type/lint diagnostics, source maps, test results, and runtime resource statistics to the host. Telemetry exporting is opt-in and cannot override host data-access policy.

Packages, JavaScript plugins, native addons, WebAssembly modules, and remote imports represent different risk categories. Native addons and FFI require stronger host trust; engine isolation is not an OS sandbox. Security updates to V8/JSC/QuickJS-ng, Tokio, TLS, and Node-compatibility code remain an ongoing runtime maintenance responsibility.

## Verification and compatibility

Bento maintains separate, reproducible compatibility evidence for:

- **JavaScript language behavior:** Test262 and engine differential tests.
- **Web APIs:** Relevant Web Platform Tests, with unsupported browser-only APIs documented.
- **Node:** Node core test suites, npm ecosystem fixtures, ESM/CJS integration, N-API where supported, process and event-loop behavior.
- **TypeScript:** Compiler baselines, `tsrs` comparisons, declaration and source-map tests, content-mapper scenarios.
- **Vite and Vitest:** Representative configs, plugins, HMR behavior, test mocks, coverage, fake timers, SSR, and framework fixtures.
- **Workers:** Differential execution against `workerd` and, when applicable, `celld`.
- **Engines:** The same API and application fixtures executed on each supported `v8x` backend and platform.
- **Embedding/security:** Capability-denial tests, virtual filesystems, cancellation, cleanup, resource exhaustion, and native-boundary tests.

Every published compatibility claim should identify the tested engine, operating system, profile, and feature set. Failed or intentionally unsupported cases are documented rather than masked by a global percentage.

## Architectural boundaries and open decisions

The following decisions require validation but do not change Bento's intended separation of concerns:

- **Deno dependency shape:** Which extension crates can be consumed upstream versus which need a maintained fork or Bento-specific host adaptation.
- **`v8x` completeness:** JSC integration on iOS and Linux, engine parity, snapshots, inspector functionality, and performance characteristics.
- **Node native extension policy:** Which N-API modules can run on V8 servers and whether non-V8 backends can offer meaningful support.
- **Vite plugin compatibility:** When JavaScript plugins can execute in Bento's own runtime and when an explicit Node interoperability process is necessary.
- **`tsrs` integration:** The adapter for shared file versions and source content mapping, without replacing the type checker's own semantic architecture.
- **Package manager interoperability:** Exact delegation and lockfile policies for npm, Yarn, Bun, and pnpm projects.
- **Worker target semantics:** The supported subset of `workerd` interfaces and whether particular bindings are simulated locally or require external services.
- **iOS distribution model:** Which extension sources and script capabilities are permitted for the intended app distribution.

The invariant is that changes to one of these integrations must not require rewriting Bento's host contract, shared workspace model, or other unrelated subsystems.

## Summary

Bento consists of two cooperating platforms: **a portable, host-controlled JavaScript/TypeScript runtime** and **a shared Rust-native application toolchain**. They communicate through stable module identities, source revisions, resolution interfaces, and execution APIs while remaining independently usable.

Deno supplies the most valuable starting point for execution and Node compatibility. `v8x` creates a plausible path to JavaScriptCore-backed embeddings. Oxc, Rolldown, `tsrs`, pnpm, and the specialized Rust tooling reduce the need to reproduce existing compiler and package-management work. Bento's own architecture supplies the missing connective tissue: capabilities, lifecycle, module identity, project snapshots, cross-tool coordination, and public embedding interfaces.

The resulting system is not merely another JavaScript CLI. It is a common execution and development substrate for servers, developer tools, coding agents, and native applications.

## References

- [Deno architecture](https://github.com/denoland/deno/blob/main/doc/architecture.md)
- [Deno's Cloudflare announcement](https://deno.com/blog/cloudflare)
- [`v8x`](https://github.com/denoland/v8x)
- [`tsrs`](https://github.com/maschwenk/tsrs)
- [Oxc](https://oxc.rs/)
- [Rolldown](https://rolldown.rs/)
- [Vite+](https://viteplus.dev/)
- [OJ](https://github.com/lovablelabs/oj)
- [pnpm 12 announcement](https://pnpm.io/blog/releases/12.0)
- [Sätteri](https://github.com/bruits/satteri)
- [`celld`](https://github.com/denoland/celld)
