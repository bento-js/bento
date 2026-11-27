---
id: proposal.0010
title: Node-Compatible Virtual File System
authors: [Mark]
status: draft
pull-request: https://github.com/bento-js/bento/pull/270
issues: [https://github.com/bento-js/bento/issues/106]
supersedes: []
---

# Node-Compatible Virtual File System

## Summary

Bento implements Node's `node:vfs` module and `--allow-fs-vfs`, and gives Rust hosts the same providers. An agent harness can then run guest code against an in-memory project or a copy-on-write overlay of a real one, and read back everything the guest changed.

## Motivation

Two groups need a virtual file system, and today Bento serves neither.

**Node applications and their tests.** Node 26.4 added `node:vfs`, a `node:fs`-shaped file system that lives in memory, in an archive, or over a real directory. Once it is mounted, `node:fs`, `require()`, and `import` can reach it. Node 27 enables it by default. Tests use it for fixtures, tools for embedded assets, and libraries for files they generate at runtime.

In Bento, `import vfs from 'node:vfs'` fails at resolution. `crates/bento_runtime/src/loader.rs` treats a `node:` specifier as built in only when `deno_node` lists it, and `vfs` is not in `node_resolver` 0.97.0's builtin list. Issue [#106](https://github.com/bento-js/bento/issues/106) records the gap. Every package that adopts `node:vfs` is one Bento cannot run.

**Agent harnesses embedding Bento.** [ARCHITECTURE.md](../ARCHITECTURE.md) promises that a harness "can inject a virtual project filesystem" and that guests "do not inherit the harness process's own filesystem". A harness running model-authored TypeScript wants three things:

- run it against a project that exists only in memory;
- let it edit a real project without the edits reaching disk until the harness accepts them;
- afterward, list exactly what it wrote and deleted.

Today `RuntimeConfig` accepts only real paths, which `start` canonicalizes with `std::fs::canonicalize`. Every file the guest touches reaches the real disk through one of several hard-wired paths, listed under [Existing baseline](#existing-baseline). The workaround is to copy the project into a temporary directory, run there, and diff the trees afterward. That writes the whole project to disk on every turn, exposes temporary paths to the guest, and recovers deletions only by diffing.

Node already designed the abstraction both groups need. Without it, Bento's Node compatibility and its embedding story both stop at the real disk.

This proposal is the core of three. Proposal 0014 ([PR #279](https://github.com/bento-js/bento/pull/279)) runs a program from a virtual file system with `--vfs-load`, and proposal 0015 ([PR #280](https://github.com/bento-js/bento/pull/280)) adds ZIP archives. Each builds on this one.

## Domain grounding

Observations below were made on macOS arm64 on 2026-11-27 with Node v26.13.1 (`--experimental-vfs`) and Node v27.0.0-alpha.0.2.0 (no flag), using probe scripts that exercised each claim directly.

### Established context

- **What `node:vfs` is.** Added in v26.4.0 at Stability 1, it "provides a virtual file system with a `node:fs`-like API... useful for tests, fixtures, embedded assets, and other scenarios where you need a self-contained file system without touching the actual file-system." ([Node `node:vfs`](https://nodejs.org/api/vfs.html))
    - In Node 26 the module requires `--experimental-vfs`; without it, `require('node:vfs')` throws `ERR_UNKNOWN_BUILTIN_MODULE`.
    - In Node 27 alpha it is available without a flag, and `--no-experimental-vfs` removes it again.
    - Both versions emit an `ExperimentalWarning` when a VFS is created, unless `{ emitExperimentalWarning: false }` is passed.
    - It exists only under the `node:` scheme. `require('vfs')` fails with `MODULE_NOT_FOUND`, `module.builtinModules` lists `node:vfs`, and `module.isBuiltin('vfs')` is `false`.
    - Observed exports: `create`, `vfsBase`, `registerProvider`, `VirtualFileSystem`, `VirtualProvider`, `MemoryProvider`, `RealFSProvider`, `ComposableProvider`, and `ZipProvider`.
- **It is not a security boundary.** Node's documentation says: "The VFS API is not a sandbox, permission system, or access-control mechanism. It does not isolate untrusted code from the host file system or from other Node.js capabilities." `RealFSProvider`'s root check "is not a security boundary".
- **The reserved mount namespace.** `vfs.mount()` takes no target. It mounts inside `vfs.vfsBase()`, which is `path.join(os.devNull, 'vfs')` (`/dev/null/vfs` on macOS), so a virtual path can never shadow a real one. Observed:
    - The first mount in a thread is `/dev/null/vfs/1`; only `--vfs-load` uses id `0`.
    - A VFS keeps its id when it is unmounted and mounted again, and a new VFS never reuses an id another VFS held.
    - The reserved root lists the current mount ids and rejects changes to its own entries with `EROFS`. With nothing mounted, reading it fails with `ENOTDIR`, because `/dev/null` is a device. An id that is not mounted gives `ENOENT`.
    - On a mount point, `rmdir` fails with `EBUSY` and `rename` with `EXDEV`. The documentation says `EBUSY` for both; this proposal matches the observed behavior.
    - Mounting a VFS that is already mounted throws `ERR_INVALID_STATE` with `Invalid state: VFS is already mounted`. `unmount()` is idempotent, and a `using` declaration unmounts.
- **Mounts belong to a thread.** Observed: a worker cannot see a mount made on the main thread, and reading the reserved root there gives `ENOTDIR`. The worker's own first mount is also `/dev/null/vfs/1`. Only the `--vfs-load` mount appears in every thread.
- **Path dispatch.** A path under a mount is served by that mount's VFS, and every other path by the real file system; there is "no search order or fallback between the two". Observed:
    - Node resolves the path lexically to decide. Inside a mount, `missing/../lib/t.txt` succeeds and a trailing slash after a file name is accepted.
    - Outside any mount the path reaches the operating system as written, so `/dev/null/vfs/1/../../../../etc/hosts` fails rather than reaching `/etc/hosts`.
    - A VFS object's own methods do not normalize. `myVfs.readFileSync('/missing/../x')` and relative paths fail with `ENOENT`.
- **Crossing the boundary.** Observed:
    - `rename`, `copyFile`, and `link` between a mount and the disk fail with `EXDEV`. `fs.cpSync` from a mount fails with `ENOTDIR`, because Node 26.13.1 does not redirect `cp`.
    - `process.chdir()` into a mount fails with `ENOTDIR`, and child processes cannot see mounts.
    - A real symbolic link pointing into a mount fails with `ENOTDIR`.
    - Inside a memory provider, an absolute link target such as `/etc/hosts` is read as a path within the provider, so following it gives `ENOENT`. Relative targets work.
    - Descriptors opened on mounted files start at `1073741824` (2³⁰).
- **Modules.** The CommonJS and ESM resolution algorithms are unchanged; each file system probe they make is dispatched by path.
    - The mount point acts as a file system root for package scope and `node_modules` lookup. Observed: a module in a mount cannot `require` a package that exists only on disk.
    - `__filename` and `import.meta.url` carry the mounted path, and Node strips TypeScript types from a mounted `.ts` module.
    - Modules are cached per path. Observed: after `unmount()` and a remount, `require` and `import` both re-read changed files.
    - Native addons in a mount are loaded "from a private, self-cleaning temporary image". ([Node `node:vfs` module loader integration](https://nodejs.org/api/vfs.html#module-loader-integration))
- **Providers.**
    - `VirtualProvider` is the base class. Subclasses implement primitives (`open`, `stat`, `lstat`, `readdir`, `mkdir`, `rmdir`, `unlink`, `rename`, and the rest) and inherit derived operations (`readFile`, `writeFile`, `exists`, `copyFile`, `access`). An unimplemented primitive throws `ERR_METHOD_NOT_IMPLEMENTED` with `The statSync method is not implemented`, and a read-only provider rejects writes with `EROFS`.
    - `MemoryProvider` supports symbolic links and watching, and `setReadOnly()` cannot be undone. Observed memory stats: the device number is `4085`; `ino` comes from a per-process counter, and a whole-file write assigns a new one; `blksize` is `4096`; `blocks` is `Math.ceil(size / 512)`.
    - `RealFSProvider(rootPath)` serves a real directory and reports the real file's stats. Observed: a `..` above the root stays at the root. `realpath` of a link that escapes the root fails with `EACCES`, but on 26.13.1 `readFile` through the same link returns the outside file, although the documentation says such links "are rejected".
    - `ComposableProvider(providers)`, added in v26.12, layers providers. The first is the writable top layer, and reads search from first to last.
    - `ZipProvider` serves ZIP archives; proposal 0015 covers it.
- **Composition semantics observed with `[MemoryProvider, RealFSProvider(disk)]`:**
    - A read falls through to the disk, and directory listings merge the layers.
    - A write lands in memory. Appending to a disk file first copies it up.
    - `unlink` and recursive `rm` of disk entries hide them. The disk keeps them, and the memory provider's own tree records no trace of the deletion: the hiding state lives inside the composition.
    - Re-creating a hidden path clears the hiding. Renaming a disk file copies it up and hides the source.
    - Renaming a directory over a non-empty lower-layer directory fails with `ENOTEMPTY`.
    - An empty provider list throws `ERR_OUT_OF_RANGE`, and a read-only top layer makes the composition read-only.
    - Node documents further limits: handles keep the layer they were opened on, and watching covers only the currently selected layer. Symbolic links resolve within their own layer, traversal through a linked directory is unsupported, and copy-up is synchronous.
- **Permissions.** Under `--permission`, `vfs.mount()` throws `ERR_INVALID_STATE` with `Invalid state: VFS cannot be used when the permission model is enabled. Use --allow-fs-vfs to allow it.` unless `--allow-fs-vfs` is given. ([`--allow-fs-vfs`](https://nodejs.org/api/cli.html#--allow-fs-vfs)) Observed:
    - Mounted paths need neither `--allow-fs-read` nor `--allow-fs-write`.
    - A `RealFSProvider`'s disk operations are checked against those flags whether it is mounted or not. Through a mount, Node 26.13.1 reports the denial as `ENOENT`.
    - `process.permission.has('fs.vfs')` is `false`, and `drop('fs.vfs')` changes nothing.
- **Prior art.** Linux overlayfs defines the vocabulary for layered file systems: an upper (writable) layer over lower layers, copy-up on first write, and whiteouts that hide lower entries on deletion. ([Linux overlayfs](https://docs.kernel.org/filesystems/overlayfs.html)) Upstream crates offer pieces but no mount or provider model:
    - `deno_lib` 0.77.0 ships `standalone::virtual_fs`, the read-only file-tree format `deno compile` embeds (`VfsBuilder`, `VirtualDirectory`, `VirtualFile`, `VirtualSymlink`).
    - `sys_traits` 0.1.28 ships `InMemorySys`, an in-memory implementation of the traits `node_resolver` reads through.

### Relevant constraints and principles

- [VISION.md](../VISION.md):
    - "The host decides what code may do" and "Restricted hosts provide scoped or virtual resources."
    - "Not a sandbox merely because it uses isolates": the VFS must be described as Node describes it, not as isolation.
    - "Compatibility is behavior, not a list of names."
- [ARCHITECTURE.md](../ARCHITECTURE.md) assigns "virtual resources" to `bento_host`, and file overlays with revisions to `bento_workspace`. The workspace's editor and agent overlays are a tooling concept for snapshots of project edits; a VFS is a runtime namespace. The two must not be conflated.
- [policy.0001](../policies/0001-prefer-voidzero-crates.md): ESM resolution and tsconfig loading stay with `oxc_resolver`, and `node_resolver` stays only where `deno_node` requires it.
- Proposal 0003 ([PR #259](https://github.com/bento-js/bento/pull/259), awaiting implementation) defines `Permissions` and reserves `--allow-fs-vfs`, with no `has()` scope for it, as in Node. Its quality bar is Node's result for every flag and denial, apart from the differences it lists.
- Distinctions the design must keep, found by an ontology review against the facts above:
    - **Thread mounts and instance mounts differ.** A guest mount lives in one thread until unmounted. A host-provided mount lives in every thread for the instance's lifetime, as Node's `--vfs-load` mount does.
    - **An overlay's changes are not its top layer.** Deletions live in the composition's whiteouts, invisible in the top provider, so reading back changes needs both.
    - **A provider has no authority of its own.** `RealFSProvider` checks the caller's file permissions.
    - **A mounted path differs from a provider path.** `/dev/null/vfs/1/lib/a.ts` is a different string from the provider's `/lib/a.ts`.

### Quality bar

- For every `node:vfs` call, `node:fs` call on a mounted path, and import or require from a mount that this design covers, Bento produces Node v26.13.1's observable result: return values, error `code`, `syscall`, `path`, and message. The exceptions are `node:vfs` availability, which follows Node v27.0.0-alpha.0.2.0, and the differences listed in [Where Bento differs from Node](#where-bento-differs-from-node).
- A Rust host runs a guest against `ComposableProvider[MemoryProvider, RealFsProvider(project)]`. The project's files on disk are byte-for-byte unchanged afterwards, and the host can list every file the guest wrote and every path it deleted.
- No operation on a mounted path touches the real disk, except through a `RealFsProvider` beneath that mount.

### Remaining uncertainty

- **ESM eviction on unmount.** Invalidating ES modules on `unmount()` needs a way to evict entries from `deno_core`'s module map, which has no public removal API. Bento already carries a `deno_core` fork, so this would likely be a fork patch. Its size is unknown.
- **JavaScript providers inside Rust resolution.** Running a JavaScript-implemented provider from inside Rust module resolution would require calling back into the isolate during `ModuleLoader::resolve` or a `require` operation. Whether `deno_core` permits that safely is unproven; see the open questions.
- **Upstream drift.** `node:vfs` is Stability 1. Its behavior may change in later Node 26.x and 27.x releases, and Bento pins to the two versions named here. The `RealFSProvider` symbolic-link discrepancy is upstream behavior that may be fixed later.
- **Dispatch overhead.** _Hypothesis:_ testing every real-disk path against the reserved root costs nothing measurable. The existing benchmark harness can measure it during implementation.

## Existing baseline

Bento has no virtual file system. Its baseline is the set of places where file access is hard-wired to the real disk, each of which this proposal must route through one dispatcher:

| Access | Where | Today |
| --- | --- | --- |
| `node:fs`, `Deno` file APIs | `assembly.rs:207`, `workers.rs:143`, `fs_ops/` | `deno_fs::RealFs` behind `FileSystemRc`, with Bento's permission-checking replacement ops |
| ESM resolution and tsconfig | `loader.rs:44` | `oxc_resolver::Resolver::new`, the default OS file system |
| ESM source loading | `loader.rs:210` | `std::fs::read_to_string` after `check_open` |
| CommonJS `require`, package scopes, CJS detection | `node.rs:51-85`, `workers.rs:146-151` | `node_resolver` and `deno_node` over `sys_traits::impls::RealSys` |
| CommonJS source loading | `loader.rs:272-276` | `std::fs::read_to_string` |
| Source lines for diagnostics | `loader.rs:254-257` | `std::fs::read_to_string` |
| Entry, project root, working directory | `lib.rs:155-161` | `std::fs::canonicalize` |
| Native addons | `deno_napi` | `dlopen` of a real path |

Worth keeping:

- Permission checks happen at native operation boundaries, with paths compared at their real locations.
- One `Sources` transform pipeline serves every module.
- Oxc resolves ESM, and `deno_node` keeps CommonJS.

The dispatcher must also leave real paths exactly as they are. [PR #267](https://github.com/bento-js/bento/pull/267) records, as ignored tests in `crates/bento_runtime/tests/native_paths.rs`, that following operations currently act on the lexically normalized check path rather than the requested one. That is Beads item `bento-iv0`. Node decides VFS dispatch on the lexically resolved path but hands every non-mounted path to the operating system as written. The dispatcher must do the same, so this proposal neither depends on that defect nor entrenches it.

## Proposed solution

Bento owns one virtual file system core in Rust, shared by guest JavaScript and the embedding host.

- A new engine-independent crate, `bento_vfs`, implements:
    - the memory, real-directory, and composable providers;
    - the `VirtualProvider` trait for custom Rust providers;
    - the mount table and path dispatch.
- `bento_runtime` routes every file access listed above through dispatch. Mounted paths therefore behave identically for `node:fs`, `require`, `import`, TypeScript transforms, diagnostics, and native addons.
- `node:vfs` is a Bento-owned built-in whose JavaScript classes are handles over the Rust providers.
- A Rust host builds the same providers, mounts them into an instance before guest code runs, and keeps handles to read them back afterwards.

```text
bento run vfs-demo.mjs              → vfs.create(); write /mod.mjs; mount(); import(`${mountPointURL}/mod.mjs`) yields 42, as in Node
bento run --permission vfs-demo.mjs → vfs.mount() throws ERR_INVALID_STATE: VFS cannot be used when the permission model is enabled. Use --allow-fs-vfs to allow it.
bento run --permission --allow-fs-vfs --allow-fs-read=. vfs-demo.mjs → yields 42
```

```rust
let edits = MemoryProvider::new();
let overlay = ComposableProvider::new(vec![
    Arc::new(edits.clone()) as Arc<dyn VirtualProvider>,
    Arc::new(RealFsProvider::new(&project)?),
])?;
let mut config = RuntimeConfig::new(host_mount_point(0), permissions);
config.cwd = scratch_dir;
config.mounts = vec![Arc::new(overlay.clone())];
let runtime = start(config, &host_mount_point(0).join("agent/task.ts"))?;
runtime.wait();
let deleted = overlay.whiteouts(); // paths the guest removed; the disk still has them
let written = edits.read_dir_recursive("/")?; // files the guest created or changed
```

## Detailed design

### Vocabulary

- A **virtual file system** (VFS) is one provider together with its mount state. In JavaScript, it is a `VirtualFileSystem` object.
- A **provider** stores entries and implements operations. Several VFSs and compositions may share one provider, and a change made through one is visible through all.
- A **provider path** is an absolute POSIX path inside a provider, such as `/lib/a.ts`. It is never a host path.
- The **reserved root** is `vfs.vfsBase()`, which is `/dev/null/vfs` on macOS. A **mount point** is `<reserved root>/<id>`. A **mounted path** is a path beneath a mount point; removing the mount point from it leaves the provider path.
- A **thread mount** is made by guest code with `vfs.mount()`. It exists only in the thread that made it, until unmounted or until the thread ends.
- An **instance mount** is made by the host before guest code runs. It is present in every thread of the instance, at the same mount point, for the instance's whole life. Guest code has no object for an instance mount and cannot unmount one.
    - This proposal defines one kind of instance mount, **host mounts** (`RuntimeConfig.mounts`).
    - Proposal 0014 adds the second kind, the **load mount**, at id `0`.
- A **layer** is one provider in a composable provider's ordered list. The first is the **top layer**.
- A **whiteout** is a composable provider's record that a path was removed, hiding that path in every layer below the top. Whiteouts belong to the composition, not to any layer.
- A provider has no authority of its own. A real-disk operation a `RealFsProvider` performs is checked against the permissions of whoever caused it: the instance, for calls caused by guest code in any of its threads, and nobody, for calls a Rust host makes directly.

### Availability of `node:vfs`

- `node:vfs` resolves for static and dynamic `import`, `require`, `import.meta.resolve` (returning `node:vfs`), and `process.getBuiltinModule('node:vfs')`. `module.builtinModules` lists `node:vfs`, and `module.isBuiltin('node:vfs')` is `true`.
- Without the scheme, `vfs` is not a built-in: `require('vfs')` fails with `MODULE_NOT_FOUND` and `import 'vfs'` with `ERR_MODULE_NOT_FOUND`, as in Node.
- It is available in every instance without a flag, as in Node 27 alpha. `bento run` accepts `--experimental-vfs` and ignores it, so commands written for Node 26 run unchanged, and `bento run --help` lists it. See the open questions.
- The module exports exactly Node 26.13.1's names: `create`, `vfsBase`, `registerProvider`, `VirtualFileSystem`, `VirtualProvider`, `MemoryProvider`, `RealFSProvider`, `ComposableProvider`, and `ZipProvider`. The ESM default export is the same object.
- `registerProvider` belongs to proposal 0014 and `ZipProvider` to proposal 0015. Until each lands, calling `registerProvider` or constructing `ZipProvider` throws `NotCapable`, with `vfs.registerProvider is unavailable in Bento` or `vfs.ZipProvider is unavailable in Bento`.
- Bento emits no `ExperimentalWarning` for `node:vfs`. It accepts the `emitExperimentalWarning` option and ignores it.
- `VirtualFileSystem` provides the synchronous, callback, and promise methods, streams, and watchers that Node's `node:vfs` documentation lists, with the same signatures and results. It also has `mount()`, `unmount()`, `[Symbol.dispose]()`, `mounted`, `mountPoint`, `mountPointURL`, `provider`, and `readonly`. Its own methods take provider paths as given: a relative path or `/missing/../x` fails with `ENOENT`.

### The reserved root and mount points

- `vfs.vfsBase()` returns `path.join(os.devNull, 'vfs')`.
- Mount ids are decimal and numbered per thread:
    - `0` is reserved for proposal 0014's load mount and never assigned to any other mount.
    - Host mounts take `1` through `n` in the order of `RuntimeConfig.mounts`.
    - A guest VFS, when first mounted, receives the lowest id above every id its thread has assigned so far: `n + 1` for the first guest mount, or `1` when there are no host mounts. It keeps that id across unmounts and remounts, and no other VFS ever receives it.
- `mount()` returns the mount point. Mounting an already-mounted VFS throws `ERR_INVALID_STATE` with `Invalid state: VFS is already mounted`. `unmount()` is idempotent, and `[Symbol.dispose]()` unmounts.
- While its thread has at least one mount, the reserved root is a read-only directory:
    - `readdir` lists the mounted ids in increasing order, and the recursive form adds `<id>/<provider path>` entries.
    - Creating, writing, or removing an entry of the reserved root fails with `EROFS`.
    - An unmounted id gives `ENOENT`.
- While its thread has no mounts, the reserved root behaves as the real disk does: on macOS, every operation on it fails with `ENOTDIR`.
- On a mount point, `rmdir` fails with `EBUSY` and `rename` with `EXDEV`. A recursive `rm` empties the mount's file system, then fails with `EBUSY`.

### Path dispatch

- Every path a file operation receives anywhere in the runtime is first resolved lexically against `process.cwd()`, as Node's `path.resolve` does. This only decides where the operation goes. `file:` URLs and `Buffer` paths are converted as Node converts them.
- If the resolved path is the reserved root or lies beneath it, and the thread has a mount, the reserved root or the owning mount serves the operation with the resolved path's provider path. Paths are therefore normalized inside mounts: `<mount>/missing/../a` reaches `/a`, and `<mount>/file.txt/` reaches the file.
- Otherwise the operation goes to the real disk exactly as it does today, with the requested path. Real-path semantics, including those recorded in `bento-iv0`, are unchanged by this proposal.
- There is no fallback in either direction. A mount answers `ENOENT` without consulting the disk, and real paths never consult a provider.
- An operation taking two paths fails with `EXDEV` when the paths lie in different mounts, or one in a mount and one on the disk. This applies to `rename`, `link`, `copyFile`, and `cp`.
- A symbolic link inside a provider resolves within that provider: an absolute target is a provider path. `readlink` returns the stored target, and `realpath` returns a mounted path. A real-disk link into the reserved root fails as the operating system fails it, with `ENOTDIR`.
- `process.cwd()` is always a real directory.
- Opening a mounted file yields a descriptor of at least `1073741824`, which never collides with a real one. Every descriptor-taking `node:fs` function works with it.
- Paths given to child processes are passed to the operating system unchanged, so a subprocess cannot see mounts.

### `node:fs` on mounted paths

- Dispatch happens in Bento's file system layer. Every `node:fs` and `node:fs/promises` function therefore serves mounted paths from their mount, as do `fs.openAsBlob`, `fs.opendir`, streams, and the `Deno` file APIs.
- Errors are Node's, with `code`, `errno`, `syscall`, and `path` set to the path the caller passed.
- Memory entries report Node's synthetic stats:
    - `dev` is `4085`;
    - `ino` comes from a per-process counter that a whole-file write advances;
    - `blksize` is `4096`, and `blocks` is `Math.ceil(size / 512)`;
    - the times are those of creation and last modification.
- A `RealFsProvider` reports the real file's stats.
- Writes to a read-only provider fail with `EROFS`.
- `fs.watch`, `fs.promises.watch`, `fs.watchFile`, and `fs.unwatchFile` work on mounted paths whose provider supports watching, with Node's events. A composition watches only the layer that currently provides the path.
- Asynchronous calls on mounted paths always complete asynchronously, even though providers are synchronous.

### Module loading from mounts

- `import`, `import()`, `import.meta.resolve`, `require`, `require.resolve`, and `module.createRequire` resolve and load mounted paths with the same algorithms as real ones. Every probe is dispatched, whether it comes from the Oxc resolver, `deno_node`'s `require`, the package-scope lookup, or the CommonJS export analyzer.
- The mount point is a resolution root: package scope lookup and the `node_modules` search stop there. Absolute specifiers may cross from a mount to the disk and back.
- A module's identity is the `file:` URL of its mounted path. The mounted path is what `__filename`, `__dirname`, `import.meta.url`, `import.meta.filename`, `import.meta.dirname`, stack traces, source maps, and `RuntimeError::location()` all carry.
- Each URL loads once. `unmount()` invalidates the modules loaded under that mount point in that thread, so a later `require` or `import` re-reads them from whatever is mounted there. Module objects that have already executed stay valid. Instance mounts never unmount, so their modules are never invalidated.

### TypeScript, JSX, and project settings

- Mounted `.ts`, `.mts`, `.cts`, `.tsx`, and `.jsx` files are transformed like files on disk.
- A mounted file is a dependency when a `node_modules` component appears below its mount point. Otherwise it uses the instance's project settings, as sources outside the project root do today.
- The project's `tsconfig.json` is read through dispatch, so a mounted project root reads it from its provider.

### Native addons

Loading a `.node` file from a mount proceeds as follows:

1. Bento checks the addon permission first, under proposal 0003's rules for disk addons. A denial stops the load before any bytes are written.
2. Bento reads the addon's bytes from the provider.
3. It writes them to a uniquely named file in a fresh directory, with mode `0700`, under the system temporary directory.
4. It loads the addon from that file.
5. As soon as loading returns, whether it succeeded or not, Bento removes the file and its directory.

`module.filename` and error messages report the mounted path.

### Watch inputs

Bento records the files a run reads so that `bento run --watch` can restart when one changes.

- A file read through a mount is recorded at its real path when a `RealFsProvider` served it, directly or as the layer of a composition that provided it. Editing it on disk restarts the run.
- A file served from memory is not recorded: nothing outside the instance can change it.

### Providers

**`MemoryProvider`** holds files, directories, and symbolic links in memory, with a root directory of mode `040755`. It reports `readonly` `false`, `supportsSymlinks` `true`, and `supportsWatch` `true`. `setReadOnly()` makes it read-only permanently.

**`RealFSProvider(rootPath)`** serves a real directory.

- `rootPath` must be a non-empty absolute string. The `rootPath` property reports it resolved lexically; containment checks use its real location.
- A provider path resolves lexically beneath the root, so `..` never climbs above it.
- An operation whose fully resolved real target lies outside the root fails with `EACCES`, including reads and writes through a symbolic link.
- Every real-disk operation it performs is a file system operation of the authority that caused it, as [Permissions](#permissions) describes.
- It reports `readonly` `false`, and `supportsSymlinks` and `supportsWatch` `true`.

**`ComposableProvider(providers)`** composes one or more providers, ordered from highest to lowest.

- An empty list throws `ERR_OUT_OF_RANGE`, with `The value of "providers.length" is out of range. It must be >= 1. Received 0`.
- `providers` returns a copy of the list, and the capability flags are the top layer's.
- **Reads.** A path is served by the first layer that has it, unless a whiteout hides it. A directory listing merges every layer's entries: a higher layer's entry shadows a lower one of the same name, and whiteouts are excluded.
- **Copy-up.** Writing, appending, truncating, or changing the metadata of a file that only a lower layer has first copies it up into the top layer, creating its parent directories there.
- **Creation.** `mkdir` creates in the top layer, and re-creating a whited-out path clears its whiteout.
- **Removal.** `unlink`, `rmdir`, and `rm` remove the top layer's entry. Each records a whiteout whenever a lower layer still has the path; a recursive removal records one whiteout for the directory. Lower layers are never modified.
- **Rename.** Renaming copies the source up, writes the target in the top layer, and whites out the source. Renaming a directory over a non-empty lower-layer directory fails with `ENOTEMPTY`.
- Node's other documented limits apply as written: handles stay on the layer where they were opened, and symbolic links resolve within their own layer. Traversal through a linked directory is unsupported, and copy-up is synchronous.
- A read-only top layer makes the composition read-only.

**JavaScript subclasses of `VirtualProvider`** work through `vfs.create(provider)` and `new VirtualFileSystem(provider)` without mounting.

- An unimplemented primitive throws `ERR_METHOD_NOT_IMPLEMENTED`, with `The <name> method is not implemented`. A provider whose `readonly` is `true` rejects writes with `EROFS`.
- Mounting a VFS whose provider is a JavaScript subclass throws `NotCapable`, with `Mounting a JavaScript-implemented VirtualProvider is unavailable in Bento`. So does using one as a layer of a `ComposableProvider`. See the open questions.

### Workers

- Every `worker_threads.Worker` and Web `Worker` of an instance starts with the instance mounts in place, at the same mount points, served by the same provider objects. A write through an instance mount in one thread is therefore visible in every other.
- A worker's `execArgv` does not change this. `workers.rs` builds every worker from the instance's own configuration and reads no Node options from `execArgv`.
- Thread mounts stay in the thread that made them, as in Node.

### Permissions

- Mounted paths are not file system resources: no fs read or write permission covers them or is needed for them, as in Node.
- A guest `vfs.mount()` needs the virtual-file-system permission from proposal 0003. The CLI grants it with `--allow-fs-vfs` under `--permission`; a Rust host grants it through `Permissions`.
    - Without it, `mount()` throws `ERR_INVALID_STATE` with Node's message, both in restricted instances and in unrestricted embeddings that deny it.
    - `process.permission.has('fs.vfs')` answers `false`, and `drop('fs.vfs')` changes nothing, as in Node.
- Instance mounts are the host's decision and need no permission.
- A `RealFsProvider` operation caused by guest code is checked with the instance's permissions against the resolved real path, exactly as the equivalent direct `node:fs` call would be. That holds both for a provider the guest created and for one in an instance mount.
    - A denial throws `ERR_ACCESS_DENIED`, with `permission` `FileSystemRead` or `FileSystemWrite` and `resource` set to the real path; see the open questions.
    - Calls a Rust host makes on a provider outside any guest operation are not checked.
- A copy-on-write overlay therefore keeps the guest from writing to disk through the overlay, but it is not a sandbox. To keep the guest from writing to the project directly, run the instance restricted without write permission for it. Copying up and recording whiteouts need only read permission on the lower layer.

### Rust embedding API

`bento_vfs` is a new engine-independent crate with no V8 dependency, re-exported as `bento_runtime::vfs`.

```rust
pub trait VirtualProvider: Send + Sync + Debug + 'static {
    fn read_only(&self) -> bool { false }
    fn supports_symlinks(&self) -> bool { false }
    fn supports_watch(&self) -> bool { false }

    // Primitives; the optional ones default to `ErrorKind::Unsupported`.
    fn stat(&self, path: &str) -> io::Result<Metadata>;
    fn lstat(&self, path: &str) -> io::Result<Metadata>;
    fn read_dir(&self, path: &str) -> io::Result<Vec<DirEntry>>;
    fn open(&self, path: &str, options: &OpenOptions) -> io::Result<Box<dyn VirtualFile>>;
    fn create_dir(&self, path: &str, mode: u32) -> io::Result<()>;
    fn remove_dir(&self, path: &str) -> io::Result<()>;
    fn remove_file(&self, path: &str) -> io::Result<()>;
    fn rename(&self, from: &str, to: &str) -> io::Result<()>;
    fn symlink(&self, target: &str, path: &str) -> io::Result<()>;
    fn read_link(&self, path: &str) -> io::Result<String>;
    fn hard_link(&self, existing: &str, new: &str) -> io::Result<()>;
    fn set_permissions(&self, path: &str, mode: u32, follow: bool) -> io::Result<()>;
    fn set_owner(&self, path: &str, uid: u32, gid: u32, follow: bool) -> io::Result<()>;
    fn set_times(&self, path: &str, accessed: SystemTime, modified: SystemTime, follow: bool) -> io::Result<()>;
    fn watch(&self, path: &str, recursive: bool, events: WatchSink) -> io::Result<WatchHandle>;

    // Derived from the primitives; a provider may override them.
    fn read(&self, path: &str) -> io::Result<Vec<u8>>;
    fn write(&self, path: &str, contents: &[u8]) -> io::Result<()>;
    fn exists(&self, path: &str) -> bool;
    fn create_dir_all(&self, path: &str) -> io::Result<()>;
    fn read_dir_recursive(&self, path: &str) -> io::Result<Vec<String>>;
}
```

- **Errors.** Provider methods return `std::io::Result`. Bento turns an error into Node's error by its raw OS error number when it has one, and otherwise by its `ErrorKind`. `ErrorKind::Unsupported` becomes `ERR_METHOD_NOT_IMPLEMENTED`, and a read-only provider's default mutators fail with `EROFS`.
- **Paths.** Every method receives an absolute, lexically normalized provider path.
- **Built-in providers:**
    - `MemoryProvider::new()` and `set_read_only()`;
    - `RealFsProvider::new(root)`, which canonicalizes `root` and fails if it is not an existing directory;
    - `ComposableProvider::new(layers)`, which fails on an empty list, with `layers()` and `whiteouts()`. `whiteouts()` returns the composition's whiteouts as sorted provider paths.
- **Sharing.** Each provider type is `Clone`, and clones share state, so a host keeps a clone to read back after the run.
- **Host mounts.** `RuntimeConfig.mounts: Vec<Arc<dyn VirtualProvider>>` lists host mounts, which are mounted at `1..=n` in every thread before guest code runs. `bento_runtime::vfs::host_mount_point(index)` returns the mount point of `mounts[index]`. `RuntimeConfig::new` leaves `mounts` empty.
- **Starting from a mount.** `start(config, entry)` accepts a `project_root` and an `entry` that are mounted paths of host mounts, and resolves them through dispatch. The mounted project root supplies `package.json`, `tsconfig.json`, and `node_modules`. `cwd` must be a real directory; a mounted `cwd` fails at start with `ErrorKind::Startup` and `working directory must be a real directory`.
- **Concurrency.** Providers are shared across the instance's threads and the host. A host may read a provider while the guest runs, and sees each guest operation either complete or not started.
- **Restricted guests.** For a mounted overlay of a real project, the guest needs fs read permission for the project's real path. `package_lookup_paths` keeps its role for building that allowance.

### Where Bento differs from Node

These are the only intended differences from Node v26.13.1:

- **Availability.** `node:vfs` is available without `--experimental-vfs`, as in Node 27 alpha, and Bento prints no `ExperimentalWarning` for it.
- **Containment.** `RealFsProvider` rejects every operation whose real target leaves its root, including reads through a symbolic link, which Node 26.13.1 follows.
- **Denials through a mount.** A denied `RealFsProvider` operation reached through a mount throws `ERR_ACCESS_DENIED`, where Node reports `ENOENT`.
- **Dispatch covers every operation.** `fs.cp` fails with `EXDEV` across a mount boundary and works inside a mount. Node 26.13.1 does not redirect `cp`, and fails with `ENOTDIR`.
- **Workers.** Every worker gets the instance mounts, whatever its `execArgv`.
- **JavaScript providers.** Providers implemented in JavaScript cannot be mounted or composed.
- **Not yet available.** Until proposals 0014 and 0015 land, `vfs.registerProvider` and `vfs.ZipProvider` throw `NotCapable`.

Host mounts have no Node counterpart.

### Documentation

- A new guide, `guides/virtual-file-system.md`, written from this proposal. It covers `node:vfs` in Bento, permissions, Rust host mounts, copy-on-write overlays with reading back changes, and the security statement.
- [Node Compatibility](../guides/node-compatibility.md), [Embedding Bento](../guides/embedding-bento.md), and [Running with Bento](../guides/running-with-bento.md) gain the corresponding sections.
- [ARCHITECTURE.md](../ARCHITECTURE.md)'s crate table gains `bento_vfs`.
- [VISION.md](../VISION.md) gains a revision-history entry at acceptance.

## Compatibility

- **Guests.** The change is additive: `node:vfs` now resolves, and paths under `/dev/null/vfs` are served while something is mounted there. Real paths behave exactly as before.
- **CLI.** `bento run` gains `--experimental-vfs`, which has no effect.
- **Rust callers.** `RuntimeConfig` gains the public field `mounts`, so struct literals without `..` must add it; `RuntimeConfig::new` is unaffected. `start` accepts mounted project roots and entries, and `bento_runtime::vfs` is new.
- **Prerequisite.** This proposal depends on proposal 0003 for `Permissions`, `--permission`, and `--allow-fs-vfs`, and is implemented after it lands.
- Nothing stored needs migrating.

## Implications on adoption

- Nothing changes for users who do not use the feature. Hosts opt in by configuring mounts, and stop by removing them.
- The workspace gains the `bento_vfs` crate and no new third-party dependency.
- No version floor changes.

## Scope

- `bento_vfs`: the `VirtualProvider` trait; the memory, real-directory, and composable providers; the per-thread mount table; the reserved root; and path dispatch.
- `bento_runtime` integration:
    - a `deno_fs::FileSystem` implementation replacing `RealFs` in the main worker and in workers;
    - a `sys_traits` implementation replacing `RealSys` for `node_resolver`, `deno_node`, and CommonJS tracking;
    - an `oxc_resolver::FileSystem` implementation through `Resolver::new_with_file_system`;
    - dispatched source loading, source-line lookup, and tsconfig loading;
    - start-up path resolution;
    - native-addon loading from mounts;
    - watch-input recording;
    - instance mounts in workers;
    - ES-module invalidation on unmount.
- The `node:vfs` built-in module, its availability, and its `node:fs`, `require`, and `import` integration.
- Permission integration with proposal 0003's `Permissions` and `--allow-fs-vfs`.
- `RuntimeConfig.mounts` and `host_mount_point`, and mounted project roots and entries in `start`.
- Tests derived from the Detailed design, with each Node expectation taken from the observations in Domain grounding.
- An example host, `examples/vfs-overlay`. It runs a guest script against a copy-on-write overlay of a sample project, prints the files written and the whiteouts, and leaves the project untouched.
- The guide, guide updates, architecture-table update, and changeset named above.

### Out of scope

- Running a program from a virtual file system: `--vfs-load`, `start_vfs_load`, and `vfs.registerProvider` are proposal 0014's ([PR #279](https://github.com/bento-js/bento/pull/279)).
- ZIP archives: `ZipProvider`, `node:zlib`'s `ZipBuffer` and `ZipFile`, and archive sources are proposal 0015's ([PR #280](https://github.com/bento-js/bento/pull/280)).
- Single executable applications (`"useVfs"`, `"vfsArchive"`): they belong to the self-contained production process work in [#241](https://github.com/bento-js/bento/issues/241).
- Loading `node:ffi` libraries from a VFS: Bento does not ship `node:ffi`.
- Keeping mounts across turns of a persistent session: proposal 0012.
- Typed host modules: proposal 0013.
- `bento_workspace` overlays, revisions, and snapshots: a separate tooling concept this proposal does not build.
- Windows path syntax for mounts: Bento builds only for macOS.
- Any sandboxing claim: Node's security statement stands.

## Preview

- **Artifact.** The installable `bento` archive from the pull request's macOS CI job (`bento-macos27-arm64-prompt`), together with `examples/vfs-overlay` run from the pull request's checkout.
- **Exercising the CLI.** The human runs `node:vfs` scripts that mount memory, real-directory, and composed providers and import from them. Each runs with and without `--permission` and `--allow-fs-vfs`, and the human compares the results with `node`.
- **Exercising the library.** The example runs an agent-style script inside an overlay and shows its edits and deletions while the sample project on disk stays unchanged.
- **Reason.** Bento publishes no packages, so the CI archive and an example host are the cheapest real ways to exercise both the command line and the library.
- **Cleanup.** Nothing persists beyond CI artifacts and the example's temporary directory, which the example removes.

## Policies and decisions checked

- [policy.0001](../policies/0001-prefer-voidzero-crates.md) — Honored:
    - `oxc_resolver` stays the ESM resolver and the tsconfig loader, reading through its own `FileSystem` trait;
    - `node_resolver` stays only for `deno_node`'s `require`, reading through a `sys_traits` implementation;
    - no SWC or Deno parsing crate is added.
- `decisions/` — No decision records exist yet.
- Proposal 0003 (in flight) — This proposal uses its `Permissions`, its `--allow-fs-vfs` flag, and its rule that `has()` has no VFS scope.
    - It conflicts with 0003's quality bar of Node's exact denial result in one place: a denied `RealFsProvider` operation through a mount throws `ERR_ACCESS_DENIED` where Node reports `ENOENT`.
    - Bento's guides already promise never to disguise denials as missing files.
    - The conflict is raised in Open questions rather than settled here.
- [ARCHITECTURE.md](../ARCHITECTURE.md) — It assigns "virtual resources" to `bento_host`. This proposal places them in a new `bento_vfs` crate, so a host that needs only grants does not link provider code. It also updates the crate table; see Alternatives considered.
- [VISION.md](../VISION.md) — Honored: host mounts are host decisions, the security statement is kept, and compatibility is measured against Node's observed behavior.

## Future directions

- `bento_workspace` could expose a revision of the project as a provider, so tooling and agent runs share one view.
- Persistent sessions could keep host mounts across turns, with snapshot and restore of a `MemoryProvider` between them.
- Mounting providers implemented in JavaScript, if deferred now.

## Alternatives considered

- **One proposal for the core, `--vfs-load`, and ZIP.** The first draft covered all three, which is more independent outcomes than one approval should carry. Mark chose on 2026-11-27 to split it. This proposal keeps the core, proposal 0014 takes loading a program from a VFS, and proposal 0015 takes ZIP archives. Each is approved on its own, and the later two depend on this one.
- **Port Node's JavaScript implementation into Bento's polyfills.** Every provider would live in the isolate, so a Rust host could not build or inspect one without running JavaScript. Bento's Rust resolvers would also have to call into JavaScript for every probe under a mount. It fails the embedding goal.
- **Mount at host-chosen real paths, overlaying the disk.** This would shadow real paths, which Node's reserved namespace exists to prevent. It would also make guest code Bento-specific and blur which bytes come from disk.
- **Copy the project to a temporary directory and diff afterwards.** This is today's workaround. It writes to disk on every run, exposes temporary paths, and recovers deletions only by diffing.
- **Provide providers through `bento_workspace` overlays.** That conflates a runtime namespace with a tooling revision model that does not exist yet.
- **Put providers in `bento_host`, as ARCHITECTURE.md's table suggests.** Every host would link provider code alongside its grants. A separate crate keeps `bento_host` small and leaves room for feature selection.
- **Leave real-disk permissions out of `RealFsProvider` for host-built providers.** The host would then have no way to scope reads through an overlay's lower layer. One rule, that the caller's permissions apply, matches Node and keeps the model uniform.

## Open questions

- [NEEDS CLARIFICATION: Should `node:vfs` be available by default, as in Node 27 alpha and as written here, or only with `--experimental-vfs`, as in Node 26? If by default, is accepting and ignoring `--experimental-vfs` right, and does Bento need Node 27's `--no-experimental-vfs`?]
- [NEEDS CLARIFICATION: Mounting or composing a `VirtualProvider` implemented in JavaScript would need Bento's Rust resolvers to call into the isolate during module resolution. Is it acceptable to ship with that throwing `NotCapable`, as written here, or must full support be in this proposal despite the unproven re-entrancy?]
- [NEEDS CLARIFICATION: A guest operation denied through a mounted `RealFsProvider` throws `ERR_ACCESS_DENIED` here, where Node 26.13.1 reports `ENOENT`. This keeps the guides' promise not to disguise denials, but departs from proposal 0003's quality bar. Which should win?]
