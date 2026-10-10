---
id: proposal.0001
title: An Embeddable TypeScript Runtime
authors: [Mark]
status: draft
pull-request: https://github.com/markmals/bento/pull/1
issues: []
supersedes: []
---

# An Embeddable TypeScript Runtime

## Summary

Add a Rust library and CLI for executing TypeScript and JavaScript, loading npm packages, and serving HTTP applications. Rust hosts can expose native APIs, restrict capabilities, and control runtime lifetimes. Both hosts share one implementation, demonstrated by the same Remix 3 guest book.

## Authorization

Mark authorized this umbrella proposal on 2026-10-10 and requested a `remix@3.0.0` application running through both the CLI and an embedded Rust host with additional native APIs. That consent covers proposal revisions. Implementation and the bounded mandate below await his approval of a specific revision; neither is active.

## Motivation

A server application needs a runtime that loads its dependencies and serves requests. A Rust application embedding the same code also needs to decide which files, network endpoints, and native services that code can use, and when execution ends. These requirements belong to the host even when the application uses familiar Node and Web APIs.

Today, a native application can launch a separate JavaScript runtime and communicate with it, or assemble an in-process environment from engine and compatibility libraries. The first approach introduces a process boundary around every native service. The second makes the application responsible for module loading, compatibility, permissions, and lifecycle integration.

Bento's [vision](../VISION.md) calls for one execution model usable by both kinds of host. The repository currently contains the design and development tooling but no runtime. This proposal defines the first independently useful part of that vision: running an existing application directly, then embedding it with host-owned services and authority.

## Domain grounding

### Established context

ESM module loading, server-side Web APIs, and Node compatibility are distinct requirements. Supporting JavaScript syntax alone does not make an npm application executable. The [published `remix@3.0.0` manifest](https://registry.npmjs.org/remix/3.0.0) declares ESM, a Node engine floor of `>=24.3.0`, and runtime subpath exports such as `remix/router` and `remix/component`; its root export is types-only.

Deno provides relevant upstream components. Its [version-pinned worker interface](https://github.com/denoland/deno/blob/v2.9.7/runtime/worker.rs) accepts a module loader, Node services, extensions, and permissions. Its [permission model](https://github.com/denoland/deno/blob/v2.9.7/runtime/permissions/lib.rs) distinguishes filesystem, network, environment, process, import, and FFI access. These interfaces provide integration points; they do not establish that a Bento embedding already works.

### Relevant constraints and principles

A runtime instance executes guest code. Its host supplies authority and owns services that may outlive that instance. Module availability and permission to perform an operation are separate: a host can expose a storage module while denying writes. Native operations must enforce those restrictions.

Dependency installation and asset compilation prepare an application for execution. A prepared application's runtime can omit those tools. Likewise, transforming TypeScript into executable JavaScript does not provide semantic type checking.

### Quality bar and remaining uncertainty

Compatibility means preserving observable application behavior across the CLI and Rust host, including asynchronous work, errors, streaming responses, and persistence. Host authority must survive access through Node-compatible APIs, and the host must regain control during shutdown even when guest code does not cooperate.

A Deno-derived runtime has not yet been compiled into Bento. Resolver integration, bootstrap and snapshot configuration, async-context behavior, streaming, and isolate interruption remain engineering risks. A different upstream assembly or compatible dependency pin may address them; spawning another runtime or weakening the promised behavior would require a revised proposal.

## Existing baseline

The [Railway/Deno guest book](https://github.com/pitlane-tools/templates/tree/8d838f3ea3b7a00f3e906e3e7dcd039ca911a5b8/railway-deno) is an existing application using exactly `remix@3.0.0`. It combines a Fetch-style router, form-data and async-context middleware, server components, browser enhancement, static assets, and SQLite persistence. Its request-time storage needs are listing entries and appending an entry.

An isolated copy was built and exercised under Deno `2.9.7` on macOS arm64. Browser rendering, the live character counter, form submission and reset worked; a plain form POST returned `302` to `/`; entries survived a process restart. It also ran with cached-only dependencies and explicit filesystem, environment, and loopback network grants. This establishes the reference behavior, while Bento compatibility and Rust embedding remain unproven.

The released template is a better reference than the local unbundled RC template, which uses `remix@3.0.0-rc.1`, removed `remix/ui` exports, Node loader hooks, and request-time native asset compilation. The released template's existing [start-task defect](https://github.com/pitlane-tools/templates/issues/11) is tracked upstream. Bento's example will use the actual emitted artifact and correct launch instructions without modifying the original template or its database.

## Proposed solution

Introduce a V8-backed Bento runtime library and two ways to use it. A developer runs a module with `bento run`, or serves a Fetch-compatible application with `bento serve`. A Rust application constructs the same runtime in-process, supplies its capabilities and native modules, and controls its lifetime through Bento's public API.

The embedding host can expose application-specific services that ordinary packages and the stock CLI cannot provide. JavaScript imports those services as modules and awaits their operations. The runtime keeps this extension mechanism separate from the application's framework and from the host's storage implementation.

Reuse Deno's runtime and compatibility components behind Bento-owned interfaces. A coherent reference assembly is acceptable initially, including its dependency footprint. The public contract does not depend on Deno CLI internals or raw engine handles.

The Remix guest book is the compatibility example for this feature set. Its CLI entrypoint uses a JavaScript SQLite adapter; its embedded entrypoint uses a Rust-owned store through a host module. Both retain the same router and UI. This proposal covers execution and embedding; the vision's integrated development toolchain remains a later change.

## Detailed design

### Execute TypeScript, JavaScript, and npm modules

The runtime loads ordinary ESM and executes JavaScript and type-bearing TypeScript source. TypeScript transformation happens natively during loading, without a compiler subprocess. Asynchronous module execution uses the runtime's event loop. Semantic type checking is outside this feature.

Applications use familiar package manifests and lockfiles. Bento resolves local dependencies, package subpath exports, and the Node built-ins needed by the tested application through a shared resolution boundary. Reuse upstream package semantics; application-specific import names and hand-written substitutes for framework dependencies are excluded. CLI and embedded execution use the same loader and compatibility implementation.

Dependencies are prepared before execution. Loading a missing package reports an actionable resolution error; it does not install the package, query a registry, or download a remote module. Module reads remain subject to the instance's granted application and dependency roots. A separate TypeScript example must exercise type-bearing source, npm imports, and asynchronous execution so that precompiled server JavaScript cannot stand in for TypeScript support.

### Run modules and serve HTTP applications

`bento run <entry>` evaluates a module and drives its asynchronous work. `bento serve <entry>` loads a module whose default export supplies a Fetch-compatible `fetch` handler, listens at an explicitly configured loopback address and port, and passes requests to that handler. Request and response behavior includes form bodies, status codes, headers, and the streamed responses required by Remix.

The application loads successfully before `serve` reports readiness. Configuration, loading, and uncaught guest failures produce contextual errors and unsuccessful CLI exits. Unsupported options are rejected. Selecting an entrypoint or listen address does not grant unrelated filesystem or network access.

Both commands use the library's construction, module evaluation, event-loop, capability, and shutdown facilities. The executing application needs no Node, Deno, Bun, package manager, or development server installed alongside Bento.

### Embed the runtime and expose native APIs

A Rust host can construct a runtime, load an entry module, drive execution or HTTP serving, observe failures, request shutdown, and dispose of runtime-owned resources. All execution occurs in the host process. Public types keep engine-specific handles private and do not require the host to spawn a CLI.

Before execution, the host can register modules backed by its own Rust services. Guest code imports a registered module and invokes asynchronous native operations. Registration belongs to the runtime instance; another instance in the same process inherits neither the module nor its grants. Native operation failures are observable by the guest and host rather than converted into empty results or false success.

An unregistered host-module import fails as an unavailable module, without package-resolution or network fallback. The stock CLI has only its standard runtime modules; it does not acquire an embedding application's private APIs. Native services enforce their own operation-level authority even when the importing guest can see the module.

### Grant capabilities per runtime instance

The CLI and embedding API accept explicit, scoped read, write, network, and environment grants. The host supplies the policy before guest execution. Permission requests never prompt interactively.

Filesystem access covers only the prepared application, declared dependency roots, and other explicitly granted resources. Canonical paths and symlinks must not turn an allowed root into access to unrelated files. Environment grants name individual variables; network grants identify permitted endpoints. Outbound destinations outside those grants are denied. Subprocess execution, FFI, native-addon loading, and remote imports are unavailable in this milestone.

Enforcement occurs at the native operation boundary, including operations reached through Node-compatible APIs. An unavailable API, denied operation, unresolved package, and guest exception remain distinguishable failures. Exposing a module does not give it ambient host-process privileges.

A host-owned service may access resources that guest filesystem APIs cannot see. For example, a storage module can write a host-selected database without granting JavaScript access to that path. These capabilities express application policy; they do not claim hostile-code isolation or replace an OS sandbox.

### Control execution lifetime

The host can shut down an instance independently of host-owned resources. Shutdown stops new requests, drains or cancels pending guest work, releases runtime-owned resources, and reports its outcome within a bounded deadline, initially five seconds.

An uncooperative synchronous guest cannot keep the host waiting indefinitely. The runtime interrupts execution, disposes of the interrupted instance, and reports that outcome; its guest state is not reused. A timeout or failure cannot report graceful success.

After disposal, the old listener no longer serves requests and a replacement instance can bind the same port. Data owned by the host remains available to that replacement. Runtime recreation and host-process restart have separate effects on resources, and persistent storage must survive both when the same database is supplied.

### Compatibility example: the Remix 3 guest book

The repository includes a derived, attributed guest book pinned to `remix@3.0.0` and a locked dependency graph. Preparation produces its server module graph, remaining local npm dependencies, asset manifest, browser JavaScript, CSS, and public assets using existing build tools. It is reproducible without the original checkout, its environment files, database, or global package cache. Missing prepared assets cause actionable errors; request handling never invokes an asset compiler.

Both hosts run the same router, controllers, server components, and browser components. The page lists persisted entries and provides the existing required name and message fields, remaining-character indicator, and Sign action. Typing updates the hydrated indicator. Submission creates one entry, returns the reference's `302` redirect to `/`, displays the saved entry, and clears the form. Plain HTML form submission also works without browser JavaScript. Frame-targeted rendering and the asset graph retain their reference behavior.

Preserve the reference's field validation and persist `id`, `name`, `message`, and `createdAt`. The shared application uses a small store interface. Its CLI adapter uses the runtime's `node:sqlite` built-in from JavaScript, as the reference does; this does not require loading an npm native addon. Its embedded adapter imports `host:guestbook`, whose asynchronous list and append operations call Rust-owned SQLite storage. This application-specific module accepts entries, not arbitrary SQL statements or database paths. Any native timestamp representation is converted at the adapter boundary so both adapters expose the same application-facing types.

The embedded host can read the resulting record directly from Rust and retain it across runtime disposal and host restart. A read-only embedded instance can import the module and list entries, but append fails at the Rust boundary without changing the database. The stock CLI rejects an import of `host:guestbook` because it has no such registration. The example uses these differences to demonstrate the general host-module and capability features without duplicating its UI or implementing a partial general-purpose SQL driver.

## Compatibility

The initial supported platform is macOS arm64 with V8. The release will identify the Node and Web API subset actually exercised by the examples and boundary checks. Reusing Deno components does not imply full Deno, Node 24, or arbitrary npm-package compatibility. The public library boundary remains independent of the selected upstream assembly.

Bento has no existing runtime API or stored-data format to migrate. The example uses the exact released Remix package unchanged. Changes are confined to the derived example's storage adapters, host entrypoints, and preparation instructions; the original example remains untouched.

## Implications on adoption

Building Bento requires the pinned Rust toolchain. Preparing the example requires its existing package and asset-build tools. Running the prepared application requires only the produced executable, application files, local dependencies, and explicit capability grants.

Embedders link the library and supply host resources through its API. They can omit application-specific module registration when they do not want to expose that service. Removing the example or replacing its adapter does not require changing the shared runtime or the original template.

## Scope

The feature includes the V8-backed execution library, TypeScript loading, the tested npm/Web/Node compatibility subset, the `run` and `serve` commands, native host modules, per-instance capability enforcement, and bounded lifecycle control. The Rust host and Remix guest book provide usable examples, accompanied by guides and behavioral checks.

### Out of scope

- Integrated package installation, semantic type checking, formatting, linting, bundling, development serving/HMR, test-runner, and task-runner products. This is the execution portion of the vision's broader MVP.
- Additional engines or platforms, C/Swift APIs, Workers deployment, per-extension minimal builds, full Node loader-hook parity, arbitrary native npm addons, and generic SQL drivers.
- New guest-book product features, contacts, authentication, multi-tenant storage, visual redesign, cloud deployment, public releases/packages, or changes to the original templates.
- Non-loopback HTTP serving; the first supported HTTP surface uses local listeners.
- Hostile-code sandboxing and comprehensive resource quotas.

## Build mandate

This proposed grant authorizes delivery of the feature above only after human approval. Execution sequencing, worker assignments, worktree coordination, and authentication runbooks live in Beads `bento-ez1`; they cannot change this grant.

- Outcome and constraints: deliver the complete Detailed design and local preview using one shared in-process runtime, native enforcement, and exactly `remix@3.0.0`. Scope and exclusions above are binding; reducing an outcome requires the human.
- Delegated decisions: choose internal implementations and compatible dependency pins, settle in-scope technical questions, authorize necessary child proposals, and record in-scope policies and decisions before implementing them. Changes to product boundaries require the human.
- Repository authority: create task branches, worktrees and PRs, push feature branches, accept independently reviewed work, merge reviewed task PRs into `proposal/weekend-remix-guest-book`, then merge the verified final PR into `main`. Direct default-branch pushes and bypassing branch protection are excluded.
- Limits: use the existing workstation and configured services, at most four workers and two heavy build/test jobs total, including at most one shared-target Rust dependency build. No new accounts, subscriptions, paid infrastructure, or purchases. Incremental metered usage awaits the spending decision below.
- Evidence: fresh repository and applicable Rust checks, the actual TypeScript and guest-book behavior under both hosts, native denial and isolation checks, persistence, bounded shutdown including an uncooperative guest, operation without another installed runtime or startup downloads, guides, runnable local previews, and the repository's required independent reviews. Unresolved material findings block acceptance.
- Completion and expiry: delivery includes the verified feature, guides, local previews, and authorized final PR merge. Authority ends at completion, revocation, or 48 hours after the human's recorded activation time, whichever comes first. Incomplete work is not accepted on expiry.
- Exclusions: public publication, production deployment, credential/access changes or renewal, force-push, branch deletion, and destructive changes to unrelated data. Building a new swarm platform, migrating Beads, adding a credential broker, or supervising the agent harness at the OS level is also excluded. Only owned temporary processes and smoke artifacts may be cleaned up; retain worktrees and review artifacts. Credential expiry stops remote actions under both bot and personal identities even if personal access remains usable; the human owns credential retention and de-provisioning.
- Approval: none yet. The human must approve the exact revision in the PR, resolve the spending limit, and record activation before the strategist records that evidence and changes the status to `awaiting-implementation`.

## Preview

Provide a local installable macOS-arm64 Bento executable, a separate Rust embedding-host executable, and the prepared guest book with its lockfile and launch instructions. Each host serves the application on an explicit loopback port with a disposable database by default. The reviewer can submit entries, restart the application, inspect Rust-owned persistence, and exercise the embedded read-only denial. Include the separate native TypeScript example.

These artifacts exercise both the CLI and library without a public deployment. Report their source revision and local paths; keep binaries and databases out of Git. Retain the runnable preview for review and remove only owned temporary databases and processes when review is finished.

## Policies and decisions checked

No numbered policies or decisions exist in `policies/` or `decisions/`. The proposal follows the [vision's](../VISION.md) embedding-first, host-authority, ecosystem-compatibility, and upstream-reuse principles and the [proposed architecture's](../ARCHITECTURE.md) separation of execution from development tooling. The initial upstream assembly is allowed without claiming that every proposed crate or feature-selected build already exists.

The [bounded-mandate contract](https://github.com/markmals/bento/blob/fb9d42c/AGENTS.md#bounded-build-mandates) governs delegated delivery. This proposal supplies product intent and bounded authority; Beads supplies the changeable execution plan. No additional policy or architectural decision is established by this revision.

## Future directions

Integrated development tooling can build on the same execution and resolution boundaries. Additional compatibility coverage, feature-selected assemblies, deployment platforms, and engines remain candidates for later proposals, with their own behavioral evidence.

## Alternatives considered

A CLI-only runtime would serve ordinary applications but leave native hosts to manage a subprocess or another integration layer. A library-only release would support embedding but omit the standalone use of the same application. The shared library with a thin CLI addresses both uses without two execution implementations.

Starting from bare `deno_core` and hand-writing the Web and Node layers would make Bento responsible for substantially more compatibility behavior. Reusing a coherent Deno-derived assembly trades a larger initial dependency footprint for existing implementations, while retaining Bento-owned public interfaces.

The contacts application would exercise more routes and interactions. The released guest book already covers rendering, forms, asynchronous execution, assets, and persistence, including an application-sized native service for the embedding example. It provides a smaller observable compatibility boundary for this first feature set.

## Open questions

- [NEEDS CLARIFICATION: What total incremental metered model/tool spending limit is authorized for autonomous delivery, or should it use only subscription-covered usage with no incremental charges? The coordinator must be able to observe and enforce the approved limit before dispatching metered work.]

## Acknowledgments

The compatibility example derives from the Pitlane Remix templates and published Remix packages. Their application behavior provides an independent reference for Bento's execution and embedding features.
