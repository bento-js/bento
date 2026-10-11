---
id: guide.node-compatibility
title: Node Compatibility
describes: [proposal.0001]
---

# Node Compatibility

## What it does

Bento executes installed JavaScript applications in-process using V8 and Deno-derived Node and Web compatibility. The milestone targets the locked Remix 3 guest-book dependency graph, not every Node 26 API or arbitrary npm package.

The exercised Node surface includes `node:path`, `node:fs`, `node:fs/promises`, `node:sqlite`, and streaming `node:http` servers. SQLite supports migrations, parameterized statements, persistent databases, and explicit connection cleanup. Servers support `close()` and `closeAllConnections()`.

Available globals include `process.argv`, instance-local `process.cwd()` and `process.env`, `Buffer`, `import.meta.dirname` and `import.meta.filename`, `createRequire`, microtasks, structured cloning, abort controllers, `AsyncLocalStorage`, Fetch requests and responses, form data, and readable streams.

## Module resolution

JavaScript entries and dependencies can use `.js`, `.mjs`, and `.cjs`. CommonJS modules imported by ESM receive generated facades with statically discovered named exports; CommonJS `require()` uses the upstream Node compatibility resolver. The runtime's ESM resolver uses Oxc with Node's `node`, `import`, and `default` conditions, package `exports` and `imports`, and real-path pnpm symlink resolution. Relative ESM imports must name the complete file, including its extension. TypeScript `paths` and `baseUrl` do not change runtime resolution.

Install the project's locked dependencies before execution. A missing package reports the specifier and recommends `bento install`; application startup never invokes an installer or downloads dependencies. Remote URL imports and guest subprocess execution are unavailable regardless of network grants. Installation and explicitly trusted project tasks are separate host operations.

The core JavaScript assembly rejects source requiring TypeScript or JSX transformation with a diagnostic naming `bento_compiler` integration. This is a stage boundary, not a substitute precompiled application path; the integrated runtime adds direct-source transformation through that compiler.

## Grants and native code

Hosts supply separate read, write, network, environment, and native-addon grants. Ungranted operations fail without prompting. Filesystem checks resolve symlinks before checking scoped roots, so a link inside an allowed directory does not authorize access to an unrelated directory. Environment reads outside the environment grant return `undefined`; enumeration excludes them, and writes fail. Guest environment changes affect only that runtime's map, never the embedding process environment.

The locked `oxc-parser`, `oxc-resolver`, `oxc-transform`, `oxc-minify`, and `lightningcss` packages use their actual Node-API binaries. Addon authority must cover the native binary's real path before initialization. Granting the project's `node_modules` root covers its pnpm virtual store. Loading denied addons does not initialize them.

Native-loading executables must export the Node-API ABI symbols. With the matching `deno_napi = "=0.190.0"` build dependency, call `deno_napi::print_linker_flags("your-binary-name")` from the executable's `build.rs`. Linking the runtime alone does not prevent the linker from stripping addon symbols.

The Remix asset compatibility test explicitly grants unrestricted reads because its package discovery probes missing optional dependencies in ancestor directories. A project-root-only read grant can fail during these probes. Filesystem write and native-addon grants remain independently scoped; the runtime never broadens a host's grant to make discovery succeed.

An authorized addon executes native code inside the Rust host process. It is not contained by JavaScript filesystem or network permissions. Only enable trusted native packages; these grants are application policy, not a hostile-code sandbox. The real Remix asset orchestration still controls development source maps and stable URLs, and production minification and fingerprinting.

## Node compatibility decisions

`EventTarget.addEventListener(type, listener, { signal })` accepts Node-style signal objects with an `aborted` property. Already-aborted signals prevent registration, and an `abort` event removes the listener. Native `AbortSignal` objects retain their ordinary behavior. Remix 3's server renderer relies on this duck-typed signal behavior; strict branded-signal validation would break rendering.

`server.listen(port)` with no host listens on `::`, accepting IPv4 and IPv6 connections when the system supports dual-stack IPv6. IPv4-only systems fall back to `0.0.0.0`. An omitted host does not mean loopback-only; grant and deployment policy must account for the listening address.

## Observable failures

Runtime outcomes distinguish completed execution, guest exits, failures, and host-requested shutdown. Errors identify startup or syntax failures, module resolution failures, transformation failures, permission violations, and uncaught exceptions or unhandled rejections. Diagnostics include the entry or failing source location instead of presenting a failed launch as successful execution. Guest `process.exit` disposes only the runtime, including inside HTTP handlers; it never exits the Rust embedding process.

Subprocess entrypoints such as `spawn` and `execSync` throw `NotCapable` before execution. This explicit denial preserves permission diagnostics rather than wrapping a denied command as a generic unsuccessful child exit. The native subprocess operations also remain ungranted, including access through lower-level compatibility APIs.

Each instance owns its permissions, environment, and execution thread. Concurrent instances do not inherit each other's grants. The Rust host drives execution directly; no Node, Bun, Deno, or pnpm process executes the application.

Signals are virtual inside embedded instances: registering a guest `process.on("SIGTERM")`, `process.on("SIGINT")`, or other supported signal listener never installs an OS signal handler. The embedding host explicitly delivers interrupt or termination through `RuntimeHandle`; the CLI owns process-global OS signal forwarding. Shutdown waits for cooperative guest cleanup up to a five-second default deadline, then interrupts JavaScript and disposes the instance. A deadline interruption is never reported as graceful. Listeners and active connections are released before completed shutdown returns.

See [Embedding Bento](embedding-bento.md) for Rust configuration, asynchronous private host modules, instance isolation, execution outcomes, and shutdown control. Authorized native addons and arbitrary blocking native code are outside the JavaScript containment and interruption guarantees.
