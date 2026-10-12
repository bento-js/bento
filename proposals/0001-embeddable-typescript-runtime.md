---
id: proposal.0001
title: An Embeddable TypeScript Runtime
authors: [Mark]
status: implemented
pull-request: https://github.com/bento-js/bento/pull/3
issues: [https://github.com/bento-js/bento/issues/2]
supersedes: []
---

# An Embeddable TypeScript Runtime

## Summary

Add a Rust library and CLI that run an existing Remix 3 application directly from TypeScript and TSX, with its npm dependencies, native asset pipeline, SQLite database, and project configuration. Developers can install dependencies, run scripts and dependent tasks, and restart a server when its source graph changes. Rust hosts use the same runtime in-process, expose additional native APIs, restrict capabilities, and control execution lifetimes.

## Authorization

On 2026-10-10, Mark selected `~/Developer/Playgrounds/remix-guest-book-bento`, specified the developer-workflow features below, and authorized this revision with: “Update the proposal with anything that needs updating based on that info and then the proposal is approved.” He also authorized all subscription-covered Claude Max and ChatGPT Pro usage and up to two Codex limit resets when needed. This revision records that approval; no further per-feature approval is required for its stated outcomes.

Approval and activation are separate. The autonomous build's 48-hour clock has not started. Only Mark can activate it through the requested `/goal` or an equivalent explicit instruction to begin the autonomous build. The coordinator then records the approved source revision, activation time, and expiry in the PR and Beads. An agent instruction cannot activate the mandate, and this proposal revision does not itself launch implementation.

Mark activated the mandate on 2026-10-10T22:46:01Z; it expires 48 hours later. During the build he also authorized two things. First, moving the patched `deno_core` to a public fork at `bento-js/deno_core`. Second, merging a one-line record-only PR into `main` after this one, to set `implemented`.

On 2026-10-12 the strategist accepted the implementation under the mandate's delegated acceptance authority, on this evidence:

- The readiness report on PR #3.
- `vp run rust` (494 tests) and `vp run check` (372 tests and the docs build) passing on integration head `84da54f`.
- The cross-artifact review.
- Eleven fresh-context adversarial rounds, the last converging on `166fb1f`.
- The exercised local preview at `166fb1f`: 23 smoke commands, and Chromium checks on both hosts.
- The benchmark run in `bench/results/2026-10-12T00-04-54Z/`.

Declined findings and documented limits are recorded in that report. Deferred defects are issues #2, #14, #18, #242, #243, and #244.

## Motivation

A useful runtime must execute the application a developer actually has, rather than a specially precompiled substitute. For this application, that includes package imports, TypeScript and JSX configuration, development tasks, SQLite, and native compilers invoked by `remix/assets` during startup and requests.

Embedding adds a different requirement: a Rust application must decide which resources and native services guest code can use, and when execution ends, without moving those services behind a subprocess boundary. The CLI and embedding host should not acquire different module loaders or compatibility behavior.

Bento's [vision](../VISION.md) calls for this shared execution model and an integrated developer workflow. The repository currently contains design and development tooling but no runtime. This proposal defines the first usable feature set around a real application, not the complete toolchain described by the vision.

## Domain grounding

### Established context

**Remix 3 is the framework under test.** The [published `remix@3.0.0` manifest](https://registry.npmjs.org/remix/3.0.0) exposes `remix/component`, `remix/assets`, `remix/router`, `remix/data-table`, and `remix/node-fetch-server`. Its root export is types-only. Some constituent packages, including `@remix-run/component` and `@remix-run/assets`, are versioned `1.0.0`; they belong to this Remix 3 distribution. Remix 1/2, React Router, React rendering, and their build conventions are not interchangeable evidence for these APIs.

ESM loading, CommonJS interoperability, Node built-ins, Web APIs, and native-addon support are distinct compatibility surfaces. The fixture uses all of them. Native asset compilation is application execution here, not an optional preparatory build step.

[Node-API](https://nodejs.org/api/n-api.html) is an ABI-stable interface independent of a particular JavaScript engine. Supporting the interface still requires an implementation and compatible platform binaries; engine choice alone does not establish addon compatibility. [Deno supports Node-API addons](https://docs.deno.com/runtime/fundamentals/node/#use-packages-with-native-addons) with local dependencies and explicit native-code authority. Its [worker interface](https://github.com/denoland/deno/blob/v2.9.7/runtime/worker.rs) also provides integration points for loaders, Node services, extensions, and permissions. Those are reusable components, not proof that a Bento embedding already works.

### Relevant constraints and principles

A runtime instance executes guest code; a host supplies authority and owns services that may outlive the instance. Making a module importable does not authorize every operation it exposes. JavaScript compatibility APIs and custom native services must enforce their applicable grants at native boundaries.

Arbitrary native addons are a stronger trust boundary than JavaScript APIs. An enabled addon executes native code in the host process and is not contained by JavaScript filesystem or network checks. Supporting trusted native compilers must not be presented as hostile-code isolation.

Installing packages, executing a project task, and running an application are separate operations. Installation may use the declared package manager; a trusted task may invoke Node-based tools. Application execution, including TypeScript transformation and Remix assets, must occur in Bento rather than secretly delegating to another runtime or compiler process. Transforming TypeScript does not provide semantic type checking.

### Quality bar and remaining uncertainty

Compatibility means preserving observable behavior across the CLI and Rust host: rendering, browser enhancement, validation, streaming, persistence, errors, and cleanup. Successful startup or an HTTP 200 alone is insufficient. A compatible native binding must preserve the called package API, not merely produce vaguely similar JavaScript.

The Deno-derived assembly, package resolution, async contexts, native addons, host-module integration, and interruption have not yet been proven inside Bento. Upstream reuse and compatible internal dependency choices are delegated engineering decisions. Spawning another application runtime, prebuilding away the required asset path, or reducing the accepted behavior is not.

## Existing baseline

The selected source is `~/Developer/Playgrounds/remix-guest-book-bento`, pinned to `remix@3.0.0`. It replaces the earlier Railway/Deno prebuilt template as this proposal's acceptance fixture. The original checkout, environment files, and database remain untouched; implementation will add an attributed, self-contained derived fixture with the selected source and lockfile provenance.

Its `server.ts` creates a `node:http` server with `createRequestListener(router.fetch)`. There is no default Fetch-handler export. The application imports `#app/*` through `package.json.imports`, uses `remix/data-table/sqlite`, and compiles assets through `remix/assets`. Its `tsconfig.json` selects NodeNext resolution and `react-jsx` with `jsxImportSource: "remix/component"`; that transform setting does not make the application React-based. The local project's declared Node engine floor is `>=26`.

The manifest already contains top-level `tasks`, alongside Node-oriented `scripts`. The `dev` task depends on `db:migrate` and runs `bento run --watch server.ts`. Tasks include string commands, sequential command arrays, objects with `command` and `dependsOn`, and dependency-only aggregators. Existing formatters, linters, the type checker, and Remix's migration CLI are package commands.

The pnpm 12 lockfile contains separate environment/tool-manager and application YAML documents, including a pinned pnpm 12.11.2. Its locked asset pipeline uses `oxc-transform`, `oxc-parser`, and `oxc-minify` 0.121.0, `oxc-resolver` 11.24.2, and `lightningcss` 1.33.0. The associated `pnpm-workspace.yaml` permits the esbuild build script; installation must respect that policy rather than approving additional scripts.

An isolated copy was installed with `pnpm install --frozen-lockfile --ignore-scripts`, migrated, and exercised with Node 26.11.1. Browser rendering, hydration, the character counter, submission and form reset worked. A valid plain POST returned **303** to `/`; invalid whitespace-only fields returned **400** with validation errors. This preparation did not exercise lifecycle-script-dependent tools or the entire task suite.

Bun 1.4.3 started the same server but returned an empty 200 after a server-side `EventTarget.addEventListener` error. Remix 3's server renderer supplies a duck-typed signal; a minimal probe confirmed that Node accepts it while Bun rejects it, although both accept a native `AbortSignal`. [Issue #2](https://github.com/markmals/bento/issues/2) tracks this external benchmark-baseline incompatibility. It is not permission to waive Bento's fixture behavior or count an empty page as a fast result.

## Proposed solution

Introduce a V8-backed execution library, a thin CLI, and a Rust embedding example. `bento run` evaluates a source file or a declared project task. Watch mode recreates the application when relevant source changes. `bento install` prepares the declared dependency graph using pnpm semantics. Both hosts run the same Remix 3 application and native asset pipeline.

Reuse a coherent Deno-derived runtime and compatibility assembly behind Bento-owned interfaces. Use native Oxc transformation for direct TypeScript, JSX, and TSX execution. The public embedding contract does not expose raw engine handles or require Deno CLI internals. A larger initial upstream dependency footprint is acceptable in exchange for real compatibility.

The embedding host additionally exposes Rust-owned guest-book storage through a private host module. The stock CLI uses JavaScript's `node:sqlite` path. Both retain the same application behavior and share the runtime, resolver, transforms, and compatibility layer.

## Detailed design

### Read the existing project and install its dependencies

For a script, locate project configuration from its enclosing package; for a task or installation, locate it from the working directory. Relative project paths, task working directories, configuration discovery, and dependency resolution use that project root consistently. Explicit script paths select files; otherwise an exact top-level task name takes precedence over a relative script filename. An unknown target reports an error rather than becoming an arbitrary shell command.

Respect `package.json` package type, dependencies, dev dependencies, package exports and imports, and the declared package-manager selection. Preserve the selected fixture's existing files rather than requiring conversion to a Bento-specific manifest. Resolve `#app/*` using the package's `imports` field, including package scope and supported conditions; the matching `tsconfig.json.paths` entry is not a substitute. NodeNext package conditions, ESM/CommonJS boundaries, pnpm symlinks, and local dependency roots must work for the fixture's graph. Do not silently turn TypeScript `paths` into a different Node resolution policy.

Read `tsconfig.json` and its applicable inherited settings. Honor execution-relevant settings, including the JSX transform and import source, while leaving semantic checking to the configured type checker. `noEmit` does not disable in-memory runtime transformation; explicit TypeScript extensions remain usable. Unsupported execution-affecting settings produce actionable diagnostics rather than silently selecting another JSX runtime.

`bento install` installs both `dependencies` and `devDependencies`. For this milestone, delegate installation to the declared pnpm 12 toolchain rather than inventing a second lockfile writer. Honor `devEngines.packageManager` and pnpm's tool-manager lock as used by the fixture. `bento install --frozen-lockfile` reproduces the locked graph without rewriting either lockfile document and fails on a manifest/lock mismatch. Normal installation retains the package manager's documented update behavior. Respect `pnpm-workspace.yaml`, platform-specific optional dependencies, and lifecycle-script authorization; never auto-approve untrusted build scripts.

Installation may require the documented pnpm/Node tooling and registry access. Execution does not install packages, resolve new versions, or download remote modules. Missing installed dependencies identify the import and preparation step that failed. A prepared application's ordinary CLI or embedded execution must work without Node, Bun, Deno, or pnpm executables on its application PATH.

### Load project environment files

Before starting a script or task, load the project-root `.env` automatically. Determine the environment mode from inherited `NODE_ENV`, then the base file; watch mode defaults to `development` only when neither supplies it. Load `.env.<mode>` when a mode is selected, then `.env.local`, with later files overriding earlier file values. Existing process variables and explicit task environment assignments take precedence over file values. Select the mode once per launch; later files do not trigger recursive profile loading.

The effective environment value is exposed as `process.env.NODE_ENV` when the host grants access to that variable. In particular, a watch-defaulted `development` value enters the launch environment even when no dotenv file exists, so the fixture's `dev` task receives development asset behavior. This default never overrides an explicit process, task, or file value.

Use dotenv-style quoted values and comments, without shell execution or command substitution. Missing optional files are harmless; malformed files produce contextual errors without logging secret values. Environment loading is per project and runtime, not a mutation of the embedding host's process environment. Loading a value does not automatically grant guest access to it. Task tools receive the task's prepared environment; restricted guests receive only what their host authorizes.

Nested Bento task commands distinguish actual inherited or explicitly assigned environment values from values Bento loaded from files. Forwarding a base `.env` value through a task must not accidentally give it precedence over the nested command's selected mode file. For example, an unset mode in `bento run dev` must not prevent its nested watch command from using `.env.development`.

### Execute source files and Node-compatible applications

`bento run <script-path>` executes JavaScript, TypeScript, JSX, and TSX directly, including top-level asynchronous work, and drives the runtime event loop. It requires neither a prebuilt JavaScript entrypoint nor `--import remix/node-tsx`. Native transformation preserves the selected JSX import source and supplies useful source locations for errors. Semantic type checking is not implied.

The required Node compatibility floor includes:

| Module | Required application behavior |
| --- | --- |
| `node:http` | Create and listen on the fixture's HTTP server; support its request/response adaptation and streaming; close the listener and active connections through the APIs used by `server.ts`. |
| `node:path` | Resolve, join, and interpret paths with the platform semantics used by application, configuration, and asset loading. |
| `node:sqlite` | Run the fixture's actual Remix SQLite adapter, migrations, parameterized reads/writes, and connection cleanup against a persistent database. |
| `node:fs` and `node:fs/promises` | Perform the file, directory, metadata, and asynchronous operations needed by configuration discovery, migrations, source loading, and assets, subject to applicable grants. |

This is a minimum list, not a reason to reject additional built-ins required by the actual dependency graph. The same graph also needs Node globals and interoperability such as `process`, `Buffer`, `createRequire`, CommonJS native wrappers, and `import.meta.dirname`, alongside Fetch, streams, form data, async context, and event APIs. Reuse existing compatibility implementations; do not replace Remix packages with fixture-specific stubs.

The application's `server.listen` determines its listening behavior under the configured network policy, including Node's omitted-host semantics. Local previews connect over loopback; Bento must not silently reinterpret an omitted host as an explicit loopback binding. Startup, module-resolution, permission, and uncaught guest failures remain distinguishable and produce unsuccessful exits when fatal.

The selected server is a side-effectful script, so `run` is the required HTTP entrypoint. The earlier draft's separate `bento serve` command and mandatory default Fetch export are replaced by this source-execution contract, not added as another weekend interface. The Rust API can still drive a Fetch handler or a script-created server in-process.

### Run tasks and their dependencies

`bento run <task-name>` reads **top-level `package.json.tasks`**. A string is a command; an array is a sequence of commands; an object has `command`, `dependsOn`, or both. A dependency-only task completes when its dependencies succeed. Existing `scripts` remain available to Node tooling but are not implicitly converted into tasks, and npm-style `pre`/`post` hooks do not run automatically. In particular, `dev` must not migrate twice because a `predev` script also exists.

Validate task shapes, missing dependencies, and cycles before executing the selected graph. Run dependencies in their declared order for this milestone, once per invocation even when shared by multiple branches. Then run the task's own command or commands. Command arrays are sequential and stop on failure; a failed dependency prevents its dependents from running. This deterministic order also avoids concurrently modifying files in the fixture's `check:fix` task.

Commands run from the project root with local `node_modules/.bin` on PATH. Preserve shell quoting, inline environment assignments, and operators used by the fixture through ordinary platform shell semantics. Forward arguments after `--` to the selected task's own command, not its dependencies; reject forwarded arguments for a dependency-only task or a multi-command array rather than guessing which command receives them. Surface child output and unsuccessful exits. Cancellation stops the task's child process group, including a nested watch server, without leaving orphan listeners.

Task execution is an explicit trusted host operation, not permission for guest JavaScript to spawn subprocesses. Nested Bento commands inherit the invoking host's runtime policy as a ceiling. Their own grant options may narrow that policy; requesting a wider grant fails before guest execution rather than silently broadening authority or silently discarding the option. This propagation contract does not sandbox the trusted task shell or external executables, which retain their host-process authority.

Bare package commands such as `oxfmt`, `oxlint`, `tsc`, and `remix db migrate` use their installed executable/shebang and may run under Node this weekend. Bento-native package-bin execution and `bento exec` are deferred, as authorized; no command should pretend a Node subprocess is Bento execution. Flagless Bento-native package execution remains the intended later default.

### Reload the server when its source graph changes

`bento run --watch <script-path>` performs a complete runtime restart, not hot module replacement. Watch the entrypoint and its transitive loaded source graph, including modules discovered after startup, and configuration that changes resolution or transformation. Also cover the fixture's browser source graph consumed by `remix/assets`, even when those files are not server-side imports; a conservative watch of configured application source roots is acceptable.

Handle edits, replacements, creations that repair a missing import, and deletions. Coalesce edit bursts and allow only one replacement instance to start after the previous instance releases its resources. A failed initial load or reload reports the failure and stays watching so a repair can recover. Do not serve stale code as though a failed reload succeeded. The next valid request after a successful reload must observe the changed source without a second manual restart.

Preserve the database and other host-owned resources across runtime replacement. Exclude databases, SQLite sidecars, dependency installations, generated outputs, and caches from source watching so requests do not trigger restart loops. A server restarted by the nested `dev` watch command does not rerun the outer task's migrations on every edit. Environment-file values are launch inputs; changing them requires restarting the enclosing command in this milestone, not a promised dotenv hot-reload feature.

Shutdown, failures, and interruption use the same lifecycle contract as ordinary execution. Final cancellation stops watching as well as the application. No HMR protocol, task cache, or parallel task scheduler is required.

### Execute the native Remix 3 asset pipeline

Run the actual `remix/assets` implementation with the locked dependency graph, at startup and on demand. Preserve development versus production behavior, source maps, minification, fingerprinting, allow/deny rules, package asset resolution, and JavaScript/TSX/CSS results. An asset may be compiled before the first HTTP request because entry registration is asynchronous. A prebuilt asset bundle does not satisfy this feature.

The straightforward compatibility route is real Node-API loading and execution for the fixture's installed native modules. A permitted alternative is to expose Bento's native implementations behind compatible package-facing APIs, preserving the JavaScript Remix asset orchestration. This alternative must cover every needed native dependency, not just `oxc-transform`: parsing and visitor keys, resolver factories, transformation, minification, and Lightning CSS are all present. A mixed route is acceptable when each package's behavior is genuine and tested.

The strategist may select the route that proves the complete fixture with the least new compatibility code; there is no obligation to implement both. Built-in replacements must respect package identity and supported versions and preserve used options, result shapes, errors, source maps, and synchronous/asynchronous behavior. Unsupported combinations fail explicitly. Do not silently patch the framework, use fake bindings, substitute a reduced asset pipeline, or spawn Node to do the work. Arbitrary addons and every historical Node-API version are not promised.

Native-code loading requires explicit host authority before addon initialization; denied loading must not execute addon code. Its trust implications remain visible in both CLI and embedding configuration. The locked compiler packages must work through reload and disposal without keeping the old server alive or corrupting a replacement instance. They already contain native compiler implementations under Node and Bun, so “Rust instead of JavaScript” is not evidence of a speedup; any benefit from built-in bindings must be measured.

### Embed the runtime and expose native APIs

A Rust host can construct a runtime, load an entry module, drive asynchronous execution and HTTP serving, observe failures, request shutdown, and dispose of runtime-owned resources. All application execution occurs in the host process. The public library boundary keeps engine-specific handles private and does not require spawning a CLI.

Before execution, a host can register modules backed by Rust services. Guest code imports them and awaits native operations. Registrations and grants belong to the instance; another instance in the same process inherits neither. Failures remain observable rather than becoming empty results. An unregistered host-module import fails without package-resolution or network fallback. The stock CLI does not acquire an embedding application's private APIs.

The guest-book embedding exposes `host:guestbook` with asynchronous list and append operations backed by Rust-owned SQLite storage. This interface accepts entries, not arbitrary SQL or database paths. Use a small application storage boundary with CLI and embedded adapters, preserving the common router, validation, rendering, and browser components rather than implementing a partial general-purpose Remix database driver. Convert native timestamp representations at that boundary so both adapters present the same application-facing types.

Rust can inspect saved records directly and retain the store across runtime disposal and host restart. A read-only instance can list entries but append fails at the Rust operation boundary without changing the database. The CLI rejects `host:guestbook` because it has no registration. These are observable demonstrations of general embedding features, not built-in guest-book functionality in the runtime.

### Grant capabilities and control execution lifetime

The CLI and embedding API accept explicit scoped read, write, network, and environment grants, plus explicit native-addon authority when that route is used. The host supplies policy before execution; permission requests never prompt interactively. Command forms above describe dispatch, not implicit unrestricted grants. Fixture launch instructions provide the required grants, including through tasks and watch mode.

Canonical paths and symlinks must not convert an allowed root into unrelated filesystem access through controlled runtime APIs. Environment grants name permitted variables; network grants identify permitted endpoints. Enforce policy at native operation boundaries, including Node-compatible APIs and registered host services. Guest subprocess execution and remote imports remain unavailable. Installer and task subprocesses are separate trusted host operations. An enabled arbitrary native addon is outside the containment of these JavaScript-facing checks; restricted execution must not claim otherwise.

A host-owned service may access a database that guest filesystem APIs cannot see. Exposing its module does not authorize writes, reveal its path, or transfer ambient host-process authority. These capabilities express application policy, not an OS sandbox or comprehensive resource quota system.

Shutdown stops new requests, drains or cancels guest work, releases runtime-owned resources, and reports its outcome within a bounded deadline, initially five seconds for cooperative operation and interruptible JavaScript. An uncooperative synchronous JavaScript guest is interrupted; the interrupted instance is disposed rather than reused. A timeout cannot report graceful success. Arbitrary blocking native code is outside the isolate's interruption guarantee; the supported fixture's native packages must nevertheless pass the actual shutdown and recreation checks.

A guest's `process.exit` ends its runtime, never the embedding Rust process. CLI signals reach the relevant guest shutdown handling; embedding shutdown remains under Rust's control rather than installing competing process-global signal handlers for each instance. After disposal, the old listener no longer serves requests and a replacement can bind the same port. Runtime recreation and host restart preserve persistent data when supplied the same database.

### Preserve the selected guest book's behavior

Include a self-contained derived fixture pinned to `remix@3.0.0` and its locked graph, without copying the original `.env` files, database, dependency cache, or generated artifacts. Keep the existing Node launch path as the baseline. Changes to shared application source are limited to the storage boundary, host entrypoints, and necessary Bento configuration; do not replace Remix 3 or redesign the app to avoid compatibility work.

Both hosts render persisted entries and the existing required name/message form. Typing updates the hydrated remaining-character indicator; pending submission and the Sign action retain their behavior. Trim and validate fields, enforce the 280-character message limit, and preserve submitted values with field errors on an invalid **400** response. A successful submission creates one entry, returns **303** with `Location: /`, displays the entry, and clears the form. Plain HTML submission also works without browser JavaScript.

Persist `id`, `name`, `message`, and `createdAt`. Preserve frame-targeted rendering, browser modules, CSS, and asset behavior in development and production. The CLI's SQLite path and the Rust host's storage path must each survive restart. Exercise the actual browser surface as well as HTTP responses; a synthetic Fetch endpoint is not a substitute.

### Measure performance against Node and Bun

Provide a repeatable benchmark harness and recorded results as the runtime becomes usable, then rerun against the integrated build. Correctness gates precede timing. Use the same selected Remix 3 source, locked packages, input data, compiler options, environment mode, and equivalent authority; record hardware, runtime versions, configuration, and source revisions. Benchmark one workload at a time, without concurrent swarm builds or tests contaminating measurements. Keep raw samples and report repeated-trial medians and spread, with latency percentiles where the sample count supports them.

| Workload | What it answers |
| --- | --- |
| Cold process launch to listening and to the first correctly rendered page | Startup versus application initialization cost; do not confuse the two milestones. |
| First JavaScript/TSX/CSS asset graph and repeat requests in the same process | Native compilation cost versus the fixture's warm in-memory cache, separately in development and production modes. |
| Matching parse, resolve, transform, minify, and CSS operations | Where time goes inside the asset path; compare native package bindings with compatible built-ins if both actually exist. |
| Steady rendered GET, asset GET, and persisted POST workloads | Throughput and latency with equivalent response bodies, validation, redirects, and database state. |
| Entry and transitive-source edits to changed responses | Watch reload latency, successful recovery, and whether persistence/listener correctness survives. |
| Idle and peak process memory during startup and asset compilation | The footprint of the usable application and native dependencies, not only an empty isolate. |

Node uses the fixture's `--import remix/node-tsx` launch path; Bun and Bento use their direct source execution. Browser-source transformations still go through the same Remix 3 asset implementation. Do not invent a persistent asset-cache scenario: this fixture uses an in-process cache. Reset databases and cold state deliberately, use warmups for steady-state measurements, and alternate runtime order to reduce environmental bias.

A runtime that fails a workload receives an explicit incompatibility result, not a favorable timing row. In particular, the current Bun full-app SSR failure blocks that comparison until resolved, while genuinely equivalent component workloads may still be measured. No arbitrary speed target or claim that Bento is faster is an acceptance criterion; reproducible, honest evidence is.

## Compatibility

The initial supported platform is macOS arm64 with V8. Identify the Node, Web, package-manager, and native-addon surfaces actually exercised. Upstream reuse does not imply full Node 26 or Deno compatibility, arbitrary npm execution, every TypeScript compiler option, or general Node loader-hook support.

Top-level `tasks` is the selected project contract; the earlier architectural illustration under `bento.tasks` is corrected rather than retained as a second schema. Other Bento-specific runtime configuration may remain namespaced. The original fixture and published Remix 3 package remain unchanged. There is no existing Bento runtime API or stored-data format to migrate.

## Implications on adoption

Building Bento requires the pinned Rust toolchain. Installing dependencies and running the fixture's package tools may require pnpm and Node; the guide must state these dependencies rather than advertising a fully self-contained development toolchain. Executing the installed application and its native assets requires only Bento or the Rust host, the project files, compatible installed native dependencies, and the host's grants.

Embedders link the library and supply resources and modules through its API. They may omit the guest-book service entirely. Replacing the example's storage adapter does not require changing the runtime or adopting an application-specific runtime fork.

## Scope

The feature includes the shared runtime library, direct JS/TS/JSX/TSX loading, the fixture's npm/Web/Node and native asset compatibility, `run`, source-graph watch, top-level tasks with dependencies, existing project configuration and imports, pnpm-backed installation including dev dependencies, automatic environment loading, custom Rust host modules, capabilities, lifecycle control, the guest book, guides, and comparative benchmarks.

### Out of scope

- Bento-native package-bin execution and `bento exec`; installed package tools may continue to use Node. Also excluded: native semantic type checking, formatter/linter/test-runner products, a general bundler, HMR, task caching, and parallel task scheduling. Running existing tools as tasks is in scope.
- Additional engines or platforms, C/Swift APIs, Workers deployment, minimal feature-selected builds, full Node compatibility, arbitrary native addons, and generic SQL drivers.
- New guest-book features, contacts, authentication, multi-tenant storage, visual redesign, cloud deployment, public releases/packages, or modifications to the original fixture.
- Hostile-code sandboxing, arbitrary native-code preemption, and comprehensive resource quotas.

## Build mandate

This approved grant covers delivery of the feature above after activation. Execution sequencing, ownership, worktrees, authentication preflight, quota observations, and durable handoffs live in Beads `bento-23w`, superseding the earlier plan in `bento-ez1`. Those operational records cannot widen this grant.

- Outcome: deliver the complete Detailed design and local preview using one shared in-process runtime and exactly `remix@3.0.0`. Scope reductions require the human; neither a precompiled fixture nor an unavailable mandatory feature counts as completion.
- Delegated decisions: choose internal implementations and compatible internal dependency pins, select the allowed native-asset route, settle in-scope technical questions, authorize necessary child proposals, and record in-scope policies and decisions before implementing them. Keep the fixture's declared dependency baseline intact; product-boundary changes require the human.
- Repository authority: create task branches, worktrees and PRs, push feature branches, accept independently reviewed work, merge reviewed task PRs into `proposal/weekend-remix-guest-book`, then merge the verified final PR into `main`. Direct default-branch pushes and bypassing branch protection are excluded.
- Compute and spending: use the existing workstation and configured services, at most four workers and two heavy build/test jobs total, including at most one shared-target Rust dependency build. All available subscription-covered Claude Max 20× and ChatGPT Pro 5× usage is authorized, including model/provider switching. Observe each provider's reported weekly, session, and model-specific windows rather than estimating remaining quota from token prices. Incremental API charges, paid extra usage, credit purchases, new accounts, subscriptions, or paid infrastructure are not authorized: the incremental spending limit is **$0**.
- Reset authority: redeem at most **two Codex limit resets**, only when needed for this delivery, through a supported provider/OMP path. Maintain a durable before/after usage and redemption ledger; none were consumed while preparing this revision. This grant does not authorize Anthropic reset credits or purchases. Do not enable unbounded global auto-redemption or consume credits merely to test the mechanism. If quota cannot be refreshed or reset redemption is unavailable, switch to an authorized subscription window, wait, or checkpoint the blocker rather than falling back to paid usage.
- Evidence: fresh repository and applicable Rust checks; actual direct-source, installation, task, watch, asset, and guest-book behavior; both hosts; native denial and instance isolation; persistence; bounded shutdown and uncooperative-JavaScript interruption; application execution without another runtime or startup downloads; benchmark correctness and recorded comparisons; guides, runnable local previews, and independent reviews. Unresolved material findings block acceptance.
- Completion and expiry: delivery includes the verified feature, guides, local previews, and authorized final PR merge. Authority ends at completion, revocation, or 48 hours after recorded activation, whichever comes first. Incomplete work is not accepted on expiry; subscription/reset availability does not extend the mandate.
- Exclusions: public publication, production deployment, credential/access changes or renewal, force-push, branch deletion, and destructive changes to unrelated data. Building a swarm platform, migrating Beads, adding a credential broker, or supervising the harness at the OS level is excluded. Clean up only owned temporary processes and smoke artifacts; retain worktrees and review artifacts. Credential expiry stops remote actions under both bot and personal identities even if personal access remains usable; the human owns credential retention and de-provisioning.
- Approval record: the human's conditional approval and subsequent reset authorization apply to this expanded revision. Record the published commit and that evidence in the PR. Activation remains pending; the status records approved intent, not an active or completed build.

## Preview

Provide a local installable macOS-arm64 Bento executable, a separate Rust embedding-host executable, and the self-contained source fixture with its lockfile and launch instructions. Demonstrate installation, `bento run server.ts`, the `dev` dependency task and watch reload, package tools through tasks, both storage adapters, and the embedded read-only denial. Use disposable databases by default and documented ports/grants. Include a focused direct-source example proving TypeScript, JSX/TSX, npm imports, and asynchronous execution rather than relying on emitted JavaScript.

The reviewer can render the page in a browser, submit entries, edit a transitive source file, see new behavior, restart, inspect Rust-owned persistence, and reproduce benchmark commands. Report source revisions and artifact paths; keep executables, databases, and raw temporary diagnostics out of Git. Retain runnable previews for review and clean up only owned temporary resources.

## Policies and decisions checked

No numbered policies or decisions exist in `policies/` or `decisions/`. The proposal follows the [vision](../VISION.md) and [architecture](../ARCHITECTURE.md): embedding-first execution, host authority, ecosystem compatibility, upstream reuse, and shared runtime/tooling boundaries. The user's selected top-level task schema replaces the illustrative namespaced schema in the architecture.

The [bounded-mandate contract](https://github.com/markmals/bento/blob/fb9d42c/AGENTS.md#bounded-build-mandates) governs delegated delivery. This proposal supplies approved product intent and bounded authority; Beads supplies the changeable execution plan. No additional standalone policy or architectural decision is needed merely to restate this feature.

## Future directions

Flagless execution of installed package binaries under Bento is the intended successor to the weekend's Node-backed task tools. Native installation, additional integrated development tools, broader compatibility, feature-selected assemblies, deployment platforms, and engines remain candidates for later proposals. Measurements from this feature should inform those choices rather than imply an unmeasured performance advantage.

## Alternatives considered

A CLI-only runtime omits native embedding; a library-only release omits the standalone developer workflow. One library with thin hosts supplies both without divergent execution implementations.

A prebuilt guest book would avoid much of the requested compatibility work, but would not exercise direct source, native assets, or watch mode. The selected Remix 3 source fixture is the acceptance target instead. The older Remix 1/2 ecosystem is not an alternative implementation of that target.

Starting with bare `deno_core` and hand-writing Web and Node behavior increases compatibility work. A coherent upstream assembly is preferred, with Bento-owned interfaces. Likewise, pnpm delegation preserves the fixture's real lockfile and installation semantics without a new package manager implementation.

Replacing all native package bindings with built-ins could reduce some integration overhead and duplicate compiler code, but introduces API-compatibility obligations. Reusing Node-API is a legitimate baseline. Choose from measured working implementations, not an assumption that embedding Oxc must be faster.

## Open questions

No product or spending question blocks this approved revision. The exact native-asset integration and runtime assembly remain bounded implementation decisions whose acceptance criteria are above. The autonomous build still requires activation; the known Bun benchmark incompatibility must remain visible until resolved.

## Acknowledgments

The selected guest-book application and published Remix 3 packages provide an independent reference through their behavior, manifests, and locked dependencies. Preserve the source's attribution and licenses when importing the fixture.
