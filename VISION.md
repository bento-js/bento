---
title: A Vision for Bento
updated: 2026-10-10
---

# A Vision for Bento

## Introduction

Bento is a TypeScript runtime that belongs to the application using it.

JavaScript development has become a collection of capable tools that each maintain their own understanding of the same project. The runtime loads modules one way, the bundler resolves them another, the type checker builds a separate graph, and the test runner starts its own execution environment. Production then sheds most of that machinery. Embedding JavaScript in a native app usually means assembling another runtime from lower-level pieces and rebuilding the compatibility those tools already had.

None of these tools is necessarily deficient. The problem is that their boundaries force every host to do the integration again. A development server, a production process, a coding agent, and an iOS app should not need four unrelated answers to what it means to load and execute a TypeScript module.

Bento makes execution an embeddable capability and development tooling a set of cooperating services around it. It combines a Node- and Web-compatible runtime with compilation, type checking, formatting, linting, bundling, testing, dependency management, and tasks. These share a coherent view of the project without requiring every consumer to ship every tool.

It is for developers who want to run a server, build a web application, or embed scripts in a native host without assembling a new JavaScript environment for each purpose. It is also for software that needs to execute code on someone else's behalf, where the host—not the script—must decide what that code can access.

## Goals

**Make the runtime a library, in addition to a command.** A Rust application can construct, run, inspect, and dispose of a Bento runtime without spawning the Bento CLI. The CLI is one host among others.

**Use one execution model across development, production, and embedding.** A module should not acquire different semantics merely because it runs inside a dev server, a deployed service, or an agent harness. Where a platform cannot support an API, the difference must be explicit.

**Treat TypeScript as the working language.** Loading, transforming, checking, diagnosing, and building TypeScript belong to the same environment. Type checking remains a real semantic check, not a claim that stripping types makes TypeScript support complete.

**Give tools one coherent view of the project.** Resolution, source revisions, dependencies, and invalidations are shared across the runtime, compiler, type checker, development server, and tests. Specialized tools may keep their own internal representations; they should not disagree about which files constitute the current project.

**Preserve the JavaScript ecosystem.** Existing packages, Node APIs, Web APIs, `package.json`, lockfiles, and familiar tool conventions should keep working. Bento integrates the ecosystem rather than replacing it with a parallel one.

**Make host authority explicit.** Filesystem, network, process, and storage access come from the embedding host. A script can have Node-compatible APIs without automatically inheriting the host process's privileges.

**Reuse excellent implementations.** Deno's runtime and compatibility crates, Oxc, Rolldown, pnpm, TypeScript tooling, and other Rust libraries are foundations to compose, not achievements to reimplement for their own sake.

## Minimum viable product

A Rust library and standalone executable that can, from the same runtime implementation:

- execute real TypeScript and JavaScript applications using ESM, server-side Web APIs, and a useful, tested subset of Node compatibility;
- run an ordinary HTTP application as a self-contained production process, without a separate Node.js installation;
- expose a Rust embedding API for module loading, execution, lifecycle management, and host-controlled filesystem, networking, and process capabilities;
- resolve and install existing npm dependencies using familiar manifests and lockfiles, without requiring installation machinery in an embedded or production runtime;
- provide integrated type checking, formatting, linting, and bundling, with consistent module resolution and source revisions;
- provide a development server, test runner, and task execution interface that use those same services rather than treating the runtime as an external subprocess;
- produce feature-selected builds, so the development toolchain can be omitted from applications that only need to execute code.

The test is not whether Bento can demonstrate every advertised command. It is whether the same small application can be developed, checked, tested, built, deployed, and embedded without changing its runtime assumptions or creating another integration layer.

## Design principles

**Embedding is the default.** Public capabilities belong in libraries with explicit ownership and lifecycles. A feature available only through a CLI is a limitation to correct, not an architectural boundary to preserve.

**Compatibility is behavior, not a list of names.** Providing `node:fs` or `fetch` is insufficient if module resolution, errors, scheduling, streams, and lifecycle behavior differ unexpectedly. Compatibility is established by running applications and conformance tests.

**The host decides what code may do.** API availability and permission to use an API are different questions. Restricted hosts provide scoped or virtual resources; unsupported operations fail clearly rather than silently escaping the host's control.

**One project view does not mean one compiler.** Oxc and a TypeScript-compatible checker solve different problems. Share identities, revisions, resolution inputs, and source mappings without forcing their ASTs or semantic models into one representation.

**Native where it helps; interoperable where it matters.** Rust implementations should handle the work that benefits from being in-process and shared. JavaScript plugins and established ecosystem behavior remain supported where replacing them would make compatibility worse.

**Pay only for what the host uses.** A deployed server should not carry a formatter, a package installer, or a dev server. An iOS extension should not acquire subprocess APIs because desktop Node applications have them. Feature selection is a property of the build; authority is a property of the runtime instance.

**Engines are implementations, not promises of equivalence.** V8 provides the principal compatibility baseline. JavaScriptCore and other engines are valuable for constrained and native hosts, but each backend must prove the semantics it supports. An abstraction that compiles is not evidence that its engines behave the same.

**Own the integration, not every dependency.** Bento should have stable boundaries around reused crates so upstream projects can evolve. Forking or rewriting a component is justified by an actual integration or maintenance problem, not by a preference for having all the code under one name.

## Beyond the minimum

Directions worth considering once the core is proven:

- JavaScriptCore-backed embeddings on macOS, Linux, and iOS, with a clearly defined subset of Node and Web APIs and the platform-specific restrictions made visible;
- deeper native Vite and Vitest compatibility, including HMR, plugin execution, coverage, mocking, and framework-specific development behavior;
- a persistent, transactional project graph that editors and coding agents can query and update, making diagnostics, tests, and previews observe the same complete change;
- stronger TypeScript language intelligence, embedded-language source mapping, declaration emit, and reusable diagnostics for native applications;
- Workers-compatible execution and deployment targets for `workerd` and `celld`, while leaving distributed persistence and orchestration to those systems;
- portable extension packaging and host-defined APIs that let an application expose exactly the scripting surface it intends to support.

## Non-goals

**Not a new JavaScript or TypeScript dialect.** Bento should execute the language people already write. It does not need syntax that only Bento understands to justify its existence.

**Not a new package ecosystem.** npm packages, existing lockfiles, and established workspace conventions remain the source of truth. Bento-specific configuration should be the exception, not a new universe of manifests.

**Not a Rust rewrite of everything.** Oxc, Rolldown, Deno, pnpm, and the rest exist so Bento does not have to reproduce their work. Tight integration matters more than owning every line.

**Not unrestricted Node on every platform.** iOS, Workers, and agent sandboxes cannot honestly promise every desktop process API, native addon, or system facility. Portable code is code whose requirements and host capabilities are understood, not code that is told every capability exists.

**Not a distributed application platform.** Compatibility with Workers is useful; owning durable storage, replication, placement, and orchestration is a different product.

**Not a sandbox merely because it uses isolates.** Host capability checks are necessary but do not replace process isolation or other controls for hostile code. Bento should make the trust boundary clear instead of claiming an engine abstraction makes untrusted execution safe.

## Revision history

- **2026-10-10** — Initial vision.
