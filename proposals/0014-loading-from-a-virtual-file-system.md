---
id: proposal.0014
title: Loading a Program from a Virtual File System
authors: [Mark]
status: draft
pull-request: https://github.com/bento-js/bento/pull/279
issues: []
supersedes: []
---

# Loading a Program from a Virtual File System

## Summary

`bento run --vfs-load=<directory>` mounts a directory as a virtual file system and runs the program inside it, as Node 26.10 and later do. `start_vfs_load` gives Rust hosts the same launch over any provider, and `vfs.registerProvider` gains Node's behavior. This builds on proposal 0010's virtual file system.

## Motivation

Node 26.10 added `--vfs-load=<source>`.

- It mounts a directory or archive in Node's reserved VFS namespace and runs the program's entry point from there.
- Every later `require` and `import` resolves against the mount, so the program sees only what the source contains.
- The program cannot be confused with, or shadow, real paths.

It is how Node runs a self-contained program tree without installing it in place, and Node 27 enables it by default. Bento accepts no such option: `bento run --vfs-load=./app` fails as an unknown option, so a command written for Node does not run under Bento.

Proposal 0010 ([PR #270](https://github.com/bento-js/bento/pull/270)) gives Bento the virtual file system itself. A Rust host can mount a provider and start an entry inside it, but it must choose that entry itself. Nothing yet does what `--vfs-load` does:

- choose the entry as `node <directory>` would;
- make the mount the project root;
- report the source as the program's path;
- put the same mount in every thread.

[VISION.md](../VISION.md) makes embedding the default: a capability only the CLI offers is a limitation. So the CLI option and a library function must arrive together.

Node also exports `vfs.registerProvider()` for choosing how a `--vfs-load` source is mounted. Proposal 0010 leaves it throwing `NotCapable`, because its only purpose is this option.

## Domain grounding

Observations below were made on macOS arm64 on 2026-11-27 with Node v26.13.1 (`--experimental-vfs`) and Node v27.0.0-alpha.0.2.0 (no flag), using probe scripts that exercised each claim directly.

### Established context

- **What `--vfs-load` does.** Added in v26.10.0, it "mounts source as a virtual file system (`node:vfs`), and runs the entry point and all subsequent `require()`/`import` resolution against that mount rather than the real file system. The mount is placed at a reserved mount point assigned by Node.js, so it never shadows real paths and no target can be chosen." ([Node `--vfs-load`](https://nodejs.org/api/cli.html#--vfs-loadsource))
    - The entry point is "taken from the mount the same way `node <directory>` takes one", and "any positional command-line argument is the program's own (available from `process.argv[2]` onward), never an entry-point override."
    - `process.argv[1]` "reports source rather than the reserved mount point".
- **Flags and limits.** Node 26 requires `--experimental-vfs`; otherwise the launch fails with `--vfs-load requires --experimental-vfs`. Node 27 alpha needs no flag. Observed:
    - A second `--vfs-load` fails with `--vfs-load may only be given once`.
    - `NODE_OPTIONS` may not carry it: `--vfs-load= is not allowed in NODE_OPTIONS`.
- **Choosing a provider.** Node chooses the provider "from the source itself rather than from its file name". A directory is mounted with a `RealFSProvider` rooted there, and a file whose bytes are a ZIP archive with a `ZipProvider`.
    - Providers registered with `vfs.registerProvider()` are consulted first, newest first, and may claim directories as well as files.
    - If none claims the source, Node exits with an error. Observed for a missing path and for a file that is not an archive: `Error [ERR_VFS_INVALID_TARGET]: <real path> is not a valid --vfs-load source: must be an existing file or directory`, with exit status 1.
- **The mount.** Observed:
    - The mount point is `/dev/null/vfs/0`; ordinary mounts start at `1`. `__filename` and `require.main.filename` are mounted paths.
    - `process.argv[1]` is the source's resolved real path, as in `/private/tmp/...` for a source given as `/tmp/...`. Extra arguments follow from `process.argv[2]`, and a file-like argument such as `perm.cjs` is still the program's argument.
    - `process.cwd()` is the shell's directory, and a dependency in the source's own `node_modules` resolves.
    - A directory mount writes through to disk.
- **Entry selection.** Observed:
    - The mount root's `package.json` `"main"` is used.
    - An invalid `"main"` falls back to `index.js`, with `[DEP0128] DeprecationWarning: Invalid 'main' field in '/dev/null/vfs/0/package.json' of 'missing.js'. Please either fix that or report it to the module author`.
    - Without `package.json`, `index.js` runs. A directory holding only `index.mjs` fails with `Cannot find module '/dev/null/vfs/0'` and exit status 1.
    - `"type": "module"` makes `index.js` an ES module, and a `"main"` of `main.ts` runs with types stripped.
    - This is CommonJS's directory loading: `LOAD_AS_DIRECTORY` tries `"main"`, then `index.js`, `index.json`, and `index.node`. ([Node modules: all together](https://nodejs.org/api/modules.html#all-together))
- **Workers.** "In worker threads `--vfs-load` mounts but does not load: a worker inherits the mount and runs its own entry point." The mount is at the same point in every thread. A worker given its own `execArgv` "inherits none of the parent's options, and so does not mount the source at all". Observed: a worker with inherited options read the mount at `/dev/null/vfs/0`, and one created with `execArgv: []` saw no mount.
- **Permissions.** Under `--permission`, `--vfs-load` without `--allow-fs-vfs` fails at start-up with `ERR_INVALID_STATE` and `Invalid state: VFS cannot be used when the permission model is enabled. Use --allow-fs-vfs to allow it.` The two versions disagree about the source. Observed:
    - Node 27 alpha requires `--allow-fs-read` to cover the source and implies nothing. Without it, it reports `ERR_VFS_INVALID_TARGET`; with it, files beside the source stay unreadable.
    - Node 26.13.1 is inconsistent. With a relative source, files elsewhere under the working directory became readable without any read flag. With an absolute `/tmp/...` source, the source was rejected as invalid.
    - On both, writes through a directory mount need `--allow-fs-write`; a write with only `--allow-fs-read=*` failed with `ERR_ACCESS_DENIED`.
- **Watch mode.** Observed with Node 26.13.1: `--watch --vfs-load=app` restarts when a file in `app` changes, but `process.argv` then repeats the source, as in `["/private/tmp/vfs-probe/app", "/private/tmp/vfs-probe/app"]`.
- **Eval.** Observed: with `-e`, Node mounts the source and runs the evaluated code instead of the entry.
- **`vfs.registerProvider(entry)`.** Added in v26.10.0. It "registers a provider that `--vfs-load` can select for a source it recognizes". `canHandle(path, stats)` and `create(path, stats)` receive the resolved path and its `fs.Stats`, and "providers must be registered before the source is mounted. Register from a module preloaded with `--require` or `--import`." ([Node `vfs.registerProvider`](https://nodejs.org/api/vfs.html#vfsregisterproviderentry)) Observed:
    - A valid entry returns `undefined`.
    - Invalid entries throw `ERR_INVALID_ARG_TYPE`, with `The "entry" argument must be of type object. Received type number (5)`, `The "entry.name" property must be of type string. Received undefined`, or `The "entry.canHandle" property must be of type function. Received type number (1)`.
    - `canHandle` was called with `/private/tmp/vfs-probe/app` and stats that are a directory.

### Relevant constraints and principles

- [VISION.md](../VISION.md): "Embedding is the default. Public capabilities belong in libraries." [ARCHITECTURE.md](../ARCHITECTURE.md): "Any operation needed by the CLI must be available through a non-CLI Rust API where practical."
- Proposal 0010 ([PR #270](https://github.com/bento-js/bento/pull/270)) supplies the providers, the reserved root, and instance mounts present in every thread. It reserves mount id `0` for this proposal's load mount. It also supplies the rule that a `RealFsProvider` operation is checked with the permissions of the guest that caused it.
- Proposal 0003 ([PR #259](https://github.com/bento-js/bento/pull/259)) defines `bento run`'s permission flags, including `--allow-fs-vfs`. Proposal 0004 ([PR #268](https://github.com/bento-js/bento/pull/268)) removes task names from `bento run`, leaving script paths as its only target.
- [policy.0001](../policies/0001-prefer-voidzero-crates.md): resolution Bento owns uses `oxc_resolver`.

### Quality bar

- For every `--vfs-load` command this design covers, Bento produces Node v26.13.1's observable result: which entry runs, `process.argv`, `__filename`, the error messages, and the exit status. The exceptions are the source permissions, which follow Node v27.0.0-alpha.0.2.0, and the differences listed in [Where Bento differs from Node](#where-bento-differs-from-node).
- `start_vfs_load` with a source behaves exactly as the CLI does, so a host and the command line cannot disagree about which program runs.

### Remaining uncertainty

- Node 26.13.1 and 27 alpha disagree about the source permission. A later Node 26.x release may adopt 27's rule or change it again.
- How the CLI's argument parser accepts `--vfs-load` with no script target, while still treating every positional argument as the program's, is an implementation question that does not change the behavior below.

## Existing baseline

`bento run` takes a required `TARGET` and passes the arguments after it to the program (`crates/bento_cli/src/arguments.rs`, `required = true, num_args = 1.., trailing_var_arg = true`). `bento_runtime::start(config, entry)` canonicalizes a real entry and runs it with `config.project_root` as the project root.

Proposal 0010 extends `start` to accept a project root and an entry beneath a host mount. A host can therefore already run a program from a provider by naming its entry. It cannot yet:

- have Bento choose the entry;
- mount the source at id `0`;
- report the source as `process.argv[1]`.

Worth keeping:

- script launches stay exactly as they are;
- one start function already handles mounted project roots, and the load path reuses it.

## Proposed solution

`--vfs-load` mounts the source as the load mount, `/dev/null/vfs/0`, in every thread, and makes it the project root. It then runs the entry `node <directory>` would choose, with every positional argument passed to the program. `start_vfs_load` is the same launch for a Rust host, over a source path or a provider the host built.

```text
bento run --vfs-load=./app serve 3000 → runs ./app's package.json "main" from /dev/null/vfs/0; process.argv[1] is ./app's real path; process.argv[2..] is ['serve', '3000']
bento run --vfs-load=./app --vfs-load=./other → fails: --vfs-load may only be given once
bento run --permission --vfs-load=./app → fails: Invalid state: VFS cannot be used when the permission model is enabled. Use --allow-fs-vfs to allow it.
bento run --permission --allow-fs-vfs --allow-fs-read=./app --vfs-load=./app → runs; writes through the mount are denied
```

```rust
let project = ComposableProvider::new(vec![
    Arc::new(MemoryProvider::new()) as Arc<dyn VirtualProvider>,
    Arc::new(RealFsProvider::new(&checkout)?),
])?;
let runtime = start_vfs_load(RuntimeConfig::new(&scratch, permissions), VfsLoad::provider(Arc::new(project)))?;
```

## Detailed design

### Vocabulary

- The **source** is the real path given to `--vfs-load` or `VfsLoad::source`, or the provider given to `VfsLoad::provider`.
- The **load mount** is the instance mount, in proposal 0010's sense, that serves the source at id `0`.
- The **entry** is the module chosen from the load mount and run as the program.

### Command line

```text
bento run [--permission] [--allow-...]... [--experimental-vfs] [--watch] --vfs-load=SOURCE [args...]
```

- `--vfs-load` takes a value, as `--vfs-load=SOURCE` or `--vfs-load SOURCE`. Without one, the launch fails with `--vfs-load requires an argument`. Given twice, it fails with `--vfs-load may only be given once`.
- With `--vfs-load`, `bento run` takes no script target. Every positional argument is the program's own, from `process.argv[2]` on, including arguments that name files or that look like options after the first positional one.
- `--vfs-load` is accepted whenever `node:vfs` is available under proposal 0010's availability rule. If that rule requires `--experimental-vfs`, a launch with `--vfs-load` but without it fails with `--vfs-load requires --experimental-vfs`.
- Launch errors exit with status 1 and print their message, like Bento's other usage errors.
- `bento run --help` lists `--vfs-load`.

### Choosing the provider

- `SOURCE` resolves against the directory `bento` runs in, then to its real location.
- A directory is served by a `RealFsProvider` rooted at its real location.
- Archive files are proposal 0015's. Until it lands, every file is an invalid source, as is anything that does not exist. The launch then fails before guest code runs, with `<real path> is not a valid --vfs-load source: must be an existing file or directory` and exit status 1. The real path is the resolved path, or the path as resolved against the working directory when it does not exist.
- Registered providers take no part in the choice, as [`vfs.registerProvider`](#vfsregisterprovider) explains.

### The load mount

- The source is mounted at `/dev/null/vfs/0`. It is in place in the main thread and every worker before any guest code runs, and for the instance's whole life. A worker's `execArgv` does not change this.
- The load mount is the instance's project root: its `package.json`, `tsconfig.json`, and `node_modules` are the ones that count.
- `process.cwd()` is the directory `bento` runs in.
- Guest code has no `VirtualFileSystem` for the load mount and cannot unmount it. `fs.rmdirSync('/dev/null/vfs/0')` fails with `EBUSY`, as proposal 0010 specifies for every mount point.
- Writes through a directory load mount reach the directory on disk, as in Node.
- Host mounts from `RuntimeConfig.mounts` keep ids `1` and up beside it.

### The entry

- Bento chooses the entry at the load mount's root with CommonJS's directory loading, using `oxc_resolver` configured for it.
    1. If `package.json` has a `"main"`, that path is tried as a file, then as a directory.
    2. If that fails, `index.js`, `index.json`, and `index.node` are tried in order.
    3. An invalid `"main"` that falls back this way emits Node's DEP0128 warning in the guest before the entry runs: `[DEP0128] DeprecationWarning: Invalid 'main' field in '/dev/null/vfs/0/package.json' of '<main>'. Please either fix that or report it to the module author`.
- `index.mjs`, `index.ts`, and other files are never chosen unless `"main"` names them.
- The entry's module format follows its extension and the nearest `package.json` `"type"`, as for any module. A TypeScript entry is transformed as proposal 0010 describes.
- Without an entry, the run fails with `Cannot find module '/dev/null/vfs/0'` and exit status 1.
- `process.argv[1]` is the source's real path, and `__filename` and `require.main.filename` are the entry's mounted path.

### Permissions

- Under `--permission`, `--vfs-load` needs `--allow-fs-vfs`. Without it, the launch fails before guest code runs with `Invalid state: VFS cannot be used when the permission model is enabled. Use --allow-fs-vfs to allow it.`
- Under `--permission`, `--allow-fs-read` must cover the source, as in Node 27 alpha. Otherwise the source is reported as invalid, with the message above. Bento implies no read permission for the source or anything beside it.
- Reads and writes through a directory load mount are checked against `--allow-fs-read` and `--allow-fs-write` on their real paths. This is proposal 0010's rule for a `RealFsProvider` operation caused by guest code.

### Watch mode

- `bento run --watch --vfs-load=<directory>` restarts when a file the run read through the load mount changes on disk. Proposal 0010's watch inputs record those files at their real paths, and they include `package.json` when entry selection read it.
- `process.argv` is the same on every run; the source is not repeated.

### `vfs.registerProvider`

- `vfs.registerProvider(entry)` replaces proposal 0010's `NotCapable` with Node's validation. These checks throw `ERR_INVALID_ARG_TYPE` with Node's messages:
    - `entry` must be an object;
    - `entry.name` must be a string;
    - `entry.canHandle` and `entry.create` must be functions.
- A valid entry is recorded, newest first, and the call returns `undefined`.
- Bento has no `--require` or `--import` preload, so guest code always runs after the load source has been chosen. Registered entries are therefore never consulted. See the open questions.

### Rust embedding API

- `bento_runtime::start_vfs_load(config: RuntimeConfig, load: VfsLoad) -> Result<RunningRuntime, StartError>` is the library form of `--vfs-load`.
    - It mounts `load` at `0` in every thread before guest code runs.
    - It makes the load mount the project root, replacing `config.project_root`, and runs the entry chosen as above.
    - `config.cwd` must be a real directory, and `config.mounts` are mounted beside it at `1` and up.
- `VfsLoad::source(path)` chooses a provider by the CLI's rules, and `process.argv[1]` is the source's real path.
    - An invalid source returns `StartError` with `ErrorKind::Startup` and the message above.
    - The host reads the source with its own authority while choosing the provider. The guest's later reads and writes through the mount are checked with its permissions.
- `VfsLoad::provider(provider)` mounts a provider the host built, such as an overlay, and `process.argv[1]` is `/dev/null/vfs/0`.
- A load with no entry returns `StartError` with `ErrorKind::Startup` and `Cannot find module '/dev/null/vfs/0'`. The CLI prints it and exits with status 1.
- Neither form checks the virtual-file-system permission: the load is the host's decision, as proposal 0010 says of instance mounts. The CLI applies Node's `--allow-fs-vfs` rule itself, because it is the host that accepts Node's command line.
- `bento run --vfs-load` is implemented with `start_vfs_load` and `VfsLoad::source`.

### Where Bento differs from Node

These are the only intended differences from Node v26.13.1:

- **Source permission.** Under `--permission`, the source needs `--allow-fs-read`, as in Node 27 alpha. Node 26.13.1 sometimes implies read access, depending on the working directory and how the path is spelled.
- **Watch mode.** `--watch` with `--vfs-load` does not repeat the source in `process.argv`.
- **Workers.** Every worker has the load mount, whatever its `execArgv`.
- **Preloads.** `vfs.registerProvider()` cannot influence `--vfs-load`, because Bento has no preloads.
- **Archives.** Until proposal 0015 lands, archive files are invalid sources.

### Documentation

- The virtual-file-system guide from proposal 0010 gains a section on running a program from a VFS, written from this proposal.
- [Running with Bento](../guides/running-with-bento.md) documents `--vfs-load`, and [Embedding Bento](../guides/embedding-bento.md) documents `start_vfs_load`.

## Compatibility

- **CLI.** The change is additive: `bento run` gains `--vfs-load`, and existing script launches are unchanged.
- **Guest code.** `vfs.registerProvider` stops throwing `NotCapable`.
- **Rust callers.** `start_vfs_load` and `VfsLoad` are new.
- **Dependencies.** This proposal depends on proposals 0010 and 0003 and is implemented after both land. If proposal 0004 has not landed, a `--vfs-load` launch never selects a task, because it has no target.
- Nothing stored needs migrating.

## Implications on adoption

No implications on adoption: the option and function are opt-in, add no dependency, and are reversed by not using them.

## Scope

- `bento run --vfs-load`, including its parsing, errors, help text, permission rules, and `--watch` behavior.
- Provider choice for directory sources, the load mount at id `0` in every thread, the load mount as project root, and entry selection with the DEP0128 fallback.
- `start_vfs_load` and `VfsLoad`.
- `vfs.registerProvider`'s validation and recording.
- Tests derived from the Detailed design, with each Node expectation taken from the observations in Domain grounding.
- The guide updates and changeset named above.

### Out of scope

- Archive sources and `ZipProvider`: proposal 0015 ([PR #280](https://github.com/bento-js/bento/pull/280)).
- `--require` and `--import` preloads, the only route by which `registerProvider` affects `--vfs-load` in Node.
- `-e` and `--eval` with `--vfs-load`: inline evaluation is proposal 0008's.
- Single executable applications: [#241](https://github.com/bento-js/bento/issues/241).
- `NODE_OPTIONS`: Bento reads no `NODE_OPTIONS`, so Node's rule against `--vfs-load` there does not arise.

## Preview

- **Artifact.** The installable `bento` archive from the pull request's macOS CI job (`bento-macos27-arm64-prompt`).
- **Exercising it.** The human runs `bento run --vfs-load` against small program directories and compares each result with `node --experimental-vfs --vfs-load` on the same directory. The directories cover a `"main"`, a missing `"main"`, a bare `index.js`, an ES module, and a TypeScript entry. They also try extra arguments, `--permission` with and without `--allow-fs-vfs` and `--allow-fs-read`, and `--watch`.
- **Reason.** The CLI calls `start_vfs_load`, so the archive exercises the library path too, and it is the cheapest real artifact.
- **Cleanup.** None beyond the CI artifact.

## Policies and decisions checked

- [policy.0001](../policies/0001-prefer-voidzero-crates.md) — Honored. Entry selection is resolution Bento owns and uses `oxc_resolver`; no Deno or SWC crate is added.
- `decisions/` — No decision records exist yet.
- Proposal 0010 (in flight) — Builds on it: the load mount is an instance mount at the id 0010 reserves, and its permission and watch rules are 0010's.
- Proposal 0003 (in flight) — Uses its `--permission`, `--allow-fs-read`, `--allow-fs-write`, and `--allow-fs-vfs` flags and its usage-error status.
- [VISION.md](../VISION.md) — Honored: the CLI option and the library function arrive together.

## Future directions

- `--require` and `--import` preloads would let registered providers claim `--vfs-load` sources.
- Once proposal 0008 lands, `-e` with `--vfs-load` could run evaluated code against the load mount, as Node does.
- A self-contained `bento` binary (#241) could mount an embedded program as the load mount.

## Alternatives considered

- **A `vfs_load` field on `RuntimeConfig` instead of `start_vfs_load`.** It would let a load mount and an explicit entry be configured together, a combination Node forbids. A separate start function keeps it unrepresentable.
- **An option to choose the entry inside the mount.** Node deliberately takes the entry from the mount, and positional arguments are the program's. A host that wants a specific entry already has proposal 0010's `start` with a mounted path.
- **Node 26.13.1's implied source read.** It depends on the working directory and how the path is spelled, and can make unrelated files readable. Node 27 alpha's explicit rule is coherent and narrower.
- **Mounting the load at a host-chosen id.** Node fixes it at `0`, so code and tools can rely on it. Proposal 0010 keeps `0` reserved for it.

## Open questions

- [NEEDS CLARIFICATION: Without preloads, `vfs.registerProvider()` validates and records entries that Bento never consults. Is an inert `registerProvider` acceptable, as written here, or should this proposal add `--require` and `--import` preloads so registered providers can claim sources as in Node?]
