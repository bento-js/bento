---
id: proposal.0015
title: ZIP Archives as Virtual File Systems
authors: [Mark]
status: draft
pull-request: https://github.com/bento-js/bento/pull/280
issues: []
supersedes: []
---

# ZIP Archives as Virtual File Systems

## Summary

Bento serves ZIP archives as virtual file systems, as Node 26.13.1 does. This brings `vfs.ZipProvider`, the `node:zlib` classes it is built from (`ZipBuffer`, `ZipFile`, and `ZipEntry`), a Rust `ZipProvider`, and archive sources for `--vfs-load` and `start_vfs_load`. Without them, a ZIP-packaged program needs extracting before it can run.

## Motivation

A ZIP archive is the common way to carry a program or its assets as one file. Node 26 serves one directly, without extraction, in three ways:

- `node --vfs-load=app.zip` runs a program from an archive;
- `new vfs.ZipProvider(archive)` mounts an archive for `node:fs`, `require`, and `import`;
- since v26.8, `node:zlib` reads, edits, and writes archives through `ZipBuffer`, `ZipFile`, and `ZipEntry`.

Tests read fixtures from archives this way, tools inspect and patch them, and applications ship their assets in one.

Bento supports none of it. Proposal 0010 ([PR #270](https://github.com/bento-js/bento/pull/270)) gives Bento virtual file systems but leaves `vfs.ZipProvider` throwing `NotCapable`. Proposal 0014 ([PR #279](https://github.com/bento-js/bento/pull/279)) adds `--vfs-load` for directories, but treats archive files as invalid sources. Bento's `node:zlib`, from `deno_node` 0.197.0, has no ZIP classes at all.

An agent harness has the same need from Rust: hand the guest a project snapshot as one archive and read back what it changed, without extracting it to disk.

## Domain grounding

Observations below were made on macOS arm64 on 2026-11-27 with Node v26.13.1, with `--experimental-vfs` for `node:vfs` and `--vfs-load`, using probe scripts that exercised each claim directly.

### Established context

- **The ZIP format.** An archive is a sequence of member records (a local header followed by content). After them come a central directory listing every member and an end-of-central-directory record. ([PKWARE APPNOTE](https://pkware.cachefly.net/webdocs/casestudies/APPNOTE.TXT))
    - Readers locate the end record from the tail and treat the central directory as authoritative.
    - Zip64 structures extend sizes, offsets, and counts beyond 32 and 16 bits.
    - Common compression methods are `0` (stored), `8` (deflate), and `93` (Zstandard).
    - Names are bytes: UTF-8 when general-purpose bit 11 is set or a Unicode-path extra field is present, and historically CP437 otherwise.
    - A member's name can contain `..` or a leading `/`, the "Zip Slip" hazard for extractors. A small archive can declare enormous members, the "zip bomb".
- **Node's ZIP archive API.** Added to `node:zlib` in v26.8.0 at Stability 1.0, Early development. "Using any part of it... emits an experimental warning the first time." Observed: `ExperimentalWarning: The zlib ZIP archive API is an experimental feature and might change at any time`. Observed exports: `ZipBuffer`, `ZipEntry`, `ZipFile`, `createZipArchive`, `createZipArchiveSync`, `zipFiles`, `getMaxZipContentSize`, and `setMaxZipContentSize`. ([Node `node:zlib`](https://nodejs.org/api/zlib.html))
- **`ZipBuffer`** is "an in-memory, zero-copy view over the entries of a ZIP archive already held in a Buffer, TypedArray, DataView, or ArrayBuffer".
    - Edits are not written into the source; `toBuffer()` and `toBufferSync()` serialize the current entries into a fresh archive, switching to Zip64 when needed. Its `writable` is always `true`.
    - `add()`, `addSync()`, and `addEntry()` replace an entry of the same name, and `delete()` drops one.
    - `get()` throws `ERR_ZIP_ENTRY_NOT_FOUND`, and `comment` round-trips.
    - Construction throws `ERR_ZIP_INVALID_ARCHIVE` or `ERR_ZIP_UNSUPPORTED_FEATURE` for a malformed or unsupported archive. Observed for an empty buffer: `invalid ZIP archive: no end of central directory record found`.
- **`ZipFile`** is "a random-access view over the entries of a ZIP archive on disk". Only the tail and the central directory are read up front, and content is read lazily.
    - `ZipFile.open(path, { writable: true })` opens it read-write. `add` and `addEntry` then append a member where the central directory began and rewrite the directory after it, while `delete` rewrites only the directory.
    - These edits are not crash-atomic. Replaced and deleted members remain as dead space until `compact()` or `compactSync()` writes a fresh archive elsewhere.
    - Writing to an archive opened read-only throws `ERR_ZIP_NOT_WRITABLE`, and an oversized central directory throws `ERR_ZIP_ARCHIVE_TOO_LARGE`.
    - Every method has a synchronous form. A synchronous method called while an asynchronous edit is unsettled throws `ERR_INVALID_STATE`.
- **`ZipEntry`** is one member.
    - `ZipEntry.create()`, `createSync()`, `createStream()`, and `createSymlink()` build entries. The methods are `'deflate'` (default), `'store'`, or `'zstd'`, with stored output when compression would not help, and `mode` defaults to `0o644`.
    - `content()` and `contentSync()` verify CRC-32 by default, throwing `ERR_ZIP_ENTRY_CORRUPT`. They reject a declared size above `maxSize`, which defaults to `zlib.getMaxZipContentSize()` (256 MiB), with `ERR_ZIP_ENTRY_TOO_LARGE`. `contentIterator()` streams without that default ceiling.
    - `name` is returned verbatim and never normalized.
    - `modified` prefers NTFS, Info-ZIP extended, or Info-ZIP Unix timestamp extra fields over the 2-second MS-DOS field.
    - `method` is the raw number, as in `93` observed for a `'zstd'` entry.
- **`vfs.ZipProvider(source)`** exposes a `ZipBuffer` or `ZipFile` through the VFS API. ([Node `ZipProvider`](https://nodejs.org/api/vfs.html#class-zipprovider)) Observed:
    - Any other argument throws `ERR_INVALID_ARG_TYPE`, with `The "source" argument must be an instance of ZipBuffer or ZipFile. Received an instance of Buffer`.
    - `readdir` lists explicit and implicit directories alike; an archive made with `zip -D`, which has no directory entries, still lists `dir`, and `statSync('/dir').isDirectory()` is `true`.
    - Stats report `dev` `4085`. A file entry recording mode `0o644` or `0` reports `100644`, one recording `0o755` reports `100755`, and the root and implicit directories report `40755`. `readonly` is `false` over a `ZipBuffer`, and `supportsSymlinks` and `supportsWatch` are `false`.
    - Over a `ZipBuffer`, writing `/new.txt` and unlinking `/a.txt` show in `zipBuffer.toBufferSync()`. Renaming works, and `symlinkSync` throws `ERR_METHOD_NOT_IMPLEMENTED`.
    - Over a read-only `ZipFile`, writes fail with `EROFS`. Over a writable `ZipFile`, a written file is in the archive on disk after `closeSync()`.
    - A file opened for writing "only commits its content (as a new archive entry) when the handle is closed".
    - An archive with an encrypted member constructs, but reading the member throws `ERR_ZIP_UNSUPPORTED_FEATURE` with `unsupported ZIP feature: entry "a.txt" is encrypted`.
- **`--vfs-load` with an archive.** Observed:
    - A file whose bytes are a ZIP archive is mounted with a `ZipProvider` whatever its name; `app.bin` worked.
    - The mount is read-only, and a write fails with `EROFS`.
    - A file that is not an archive fails with `ERR_VFS_INVALID_TARGET`, with `<real path> is not a valid --vfs-load source: must be an existing file or directory`.
- **Upstream crates.**
    - `deno_node` 0.197.0's `node:zlib` polyfill implements streams and convenience functions, including Zstandard, but no ZIP classes.
    - Bento's lockfile already carries `flate2`, `zlib-rs`, `miniz_oxide`, and `zstd` 0.13.2. It carries no ZIP crate.

### Relevant constraints and principles

- [VISION.md](../VISION.md): "Compatibility is behavior, not a list of names." The API is Node's, so its errors, edge cases, and safety checks are part of the promise. "Pay only for what the host uses": a host that needs no archives should not have to link ZIP code.
- Proposal 0010 supplies the `VirtualProvider` trait, the providers' shared semantics, and the rule that real-disk operations are checked with the permissions of the guest that caused them. Proposal 0014 supplies source choice for `--vfs-load` and `start_vfs_load`, which this proposal extends to archives.
- [policy.0001](../policies/0001-prefer-voidzero-crates.md) governs parsing, transformation, resolution, and bundling. Archive reading and writing is none of those, so it does not decide the ZIP dependency.

### Quality bar

- For every `ZipProvider`, `ZipBuffer`, `ZipFile`, and `ZipEntry` call this design covers, Bento produces Node v26.13.1's observable result: entries, contents, metadata, error `code`, and message. The only exceptions are the differences listed in [Where Bento differs from Node](#where-bento-differs-from-node).
- Archives Bento writes are read correctly by Node and by `unzip`, and archives they write are read correctly by Bento.
- No member content is allocated beyond the declared-size ceiling before the check that rejects it.

### Remaining uncertainty

- The ZIP archive API is at Stability 1.0, Early development, so later Node releases may change it. Bento pins to Node v26.13.1.
- Whether the candidate ZIP crate supports Node's in-place append to a writable archive directly, or Bento must write that path itself, is an implementation question that does not change the behavior below.

## Existing baseline

Bento has no ZIP support. `node:zlib` is `deno_node`'s polyfill, and proposal 0010 makes `vfs.ZipProvider` throw `NotCapable`. Proposal 0014 rejects every file given as a `--vfs-load` source.

Worth keeping:

- `deno_node`'s existing `node:zlib` streams and functions stay as they are. The ZIP classes are added beside them, the way proposal 0010 adds `node:vfs`.
- The deflate and Zstandard crates already in the lockfile are reused.

## Proposed solution

`bento_vfs` gains a ZIP module, behind a `zip` cargo feature enabled by default. It reads and writes archives in memory and on disk with Node's semantics, and implements proposal 0010's `VirtualProvider` over them.

`node:zlib` gains `ZipBuffer`, `ZipFile`, and `ZipEntry` as handles over that module, and `vfs.ZipProvider` accepts them. Proposal 0014's source choice recognizes archive files by their bytes and mounts them read-only.

```text
bento run --vfs-load=app.zip → runs app.zip's package.json "main" from /dev/null/vfs/0; writes through the mount fail with EROFS
new vfs.ZipProvider(new zlib.ZipBuffer(bytes)) → a writable VFS whose edits appear in zipBuffer.toBufferSync()
new vfs.ZipProvider(zlib.ZipFile.openSync('assets.zip')) → a read-only VFS; writes fail with EROFS
```

```rust
let snapshot = ZipProvider::from_bytes(std::fs::read("project.zip")?)?;
config.mounts = vec![Arc::new(snapshot.clone())];
// ... run the guest ...
std::fs::write("project-after.zip", snapshot.to_bytes()?)?;
```

## Detailed design

### Vocabulary

- An **archive** is a ZIP byte sequence.
- An **entry** is one member of an archive: a file, a directory (its name ends in `/`), or a symbolic link (its Unix mode marks it as one).
- An **in-memory archive** is held in memory and serialized on request; this is Node's `ZipBuffer`.
- An **archive file** is read from disk lazily and, when **writable**, edited in place; this is Node's `ZipFile`.

### `node:zlib` ZIP classes

- `node:zlib` gains `ZipBuffer`, `ZipFile`, `ZipEntry`, `getMaxZipContentSize`, and `setMaxZipContentSize`.
- Each member Node 26.13.1 documents for these five behaves as documented and as Domain grounding records. That covers construction, the synchronous and asynchronous methods, iteration order, replacement by name, `comment`, `writable`, `toBuffer`, `compact`, `stream`, content verification and ceilings, and `ZipEntry`'s properties and static methods.
- `createZipArchive`, `createZipArchiveSync`, and `zipFiles` are not added; see the open questions.
- Supported methods are stored, deflate, and Zstandard. Zip64 archives are read, and Zip64 structures are written whenever a count, size, or offset needs them.
- Names are decoded as Node decodes them: Unicode-path extra field, then UTF-8 by flag or validity, then CP437. They are returned verbatim.
- Unsupported features, such as encryption, raise `ERR_ZIP_UNSUPPORTED_FEATURE` when the affected entry is read, with Node's message.
- The errors are Node's: `ERR_ZIP_INVALID_ARCHIVE`, `ERR_ZIP_UNSUPPORTED_FEATURE`, `ERR_ZIP_ENTRY_NOT_FOUND`, `ERR_ZIP_ENTRY_TOO_LARGE`, `ERR_ZIP_ENTRY_CORRUPT`, `ERR_ZIP_NOT_WRITABLE`, `ERR_ZIP_ARCHIVE_TOO_LARGE`, and `ERR_INVALID_STATE`, each with Node's message.
- Writable archive files are edited in place as Node edits them, including dead space and the lack of crash atomicity, which the guide states.
- Bento emits no `ExperimentalWarning` for the ZIP archive API, as proposal 0010 decides for `node:vfs`.
- `ZipFile.open` and `openSync` are file system operations of the guest on the archive's real path. They need read permission, and write permission when `writable` is `true`, under proposal 0003's rules. A `ZipBuffer` touches no file.

### `vfs.ZipProvider`

- `new vfs.ZipProvider(source)` takes a `ZipBuffer` or a `ZipFile`. Anything else throws `ERR_INVALID_ARG_TYPE`, with `The "source" argument must be an instance of ZipBuffer or ZipFile. Received <description>`. This replaces proposal 0010's `NotCapable`.
- **Directories** are explicit directory entries and every name prefix ending in `/`. Both are listed by `readdir`, including recursively, and `stat` reports them as directories.
- **Stats** are proposal 0010's synthetic stats, with `dev` `4085`. A file reports mode `0o100000` plus the entry's permission bits, or `0o100644` when the entry records `0`. The root and implicit directories report `0o40755`, and a directory entry reports `0o40000` plus its permission bits.
- **Reads** decompress the entry. An encrypted entry fails with `ERR_ZIP_UNSUPPORTED_FEATURE`.
- **Writes.** A file opened for writing becomes an entry, replacing one of the same name, when its handle closes.
    - Over a `ZipBuffer`, the entry lives in memory and appears in `toBuffer()`.
    - Over a writable `ZipFile`, it is appended to the archive on disk.
- **Other changes.**
    - `mkdir` adds a directory entry, and `unlink` and `rmdir` delete entries.
    - `rename` adds the target and deletes the source.
    - `symlink` throws `ERR_METHOD_NOT_IMPLEMENTED`.
- **Flags.** `readonly` is the opposite of the archive's `writable`, and writes to a read-only archive fail with `EROFS`. `supportsSymlinks` and `supportsWatch` are `false`.

### Rust `ZipProvider`

- `ZipProvider::from_bytes(bytes: Vec<u8>) -> io::Result<ZipProvider>` is an in-memory archive, always writable. `to_bytes() -> io::Result<Vec<u8>>` serializes its current entries.
- `ZipProvider::open(path, writable: bool) -> io::Result<ZipProvider>` opens an archive file. The file closes when the last clone is dropped.
- `ZipProvider` implements proposal 0010's `VirtualProvider` with the semantics above. Malformed archives fail with an `io::Error` of kind `InvalidData`, which carries Node's `ERR_ZIP_INVALID_ARCHIVE` message.
- Clones share state, like proposal 0010's providers. A `ZipProvider` works in `RuntimeConfig.mounts`, in a `ComposableProvider`, and as a load.

### Archive sources

This section extends proposal 0014's provider choice.

- A regular file whose bytes are a ZIP archive, meaning they contain an end-of-central-directory record, is served by a read-only `ZipProvider` over the file, whatever its name. Every other file remains an invalid source with proposal 0014's message.
- Writes through an archive load mount fail with `EROFS`.
- Under `--permission`, `--allow-fs-read` must cover the archive, as for a directory source.
- `bento run --watch` restarts when the archive file changes.
- `VfsLoad::source(path)` applies the same rules, and `process.argv[1]` is the archive's real path.

### Where Bento differs from Node

These are the only intended differences from Node v26.13.1:

- **No warning.** Bento prints no `ExperimentalWarning` for the ZIP archive API.
- **Compressed bytes.** Archives Bento writes are valid, and every entry's content, name, mode, time, and comment round-trip. The compressed bytes need not match Node's, because the deflate and Zstandard encoders differ.
- **Missing helpers.** `createZipArchive`, `createZipArchiveSync`, and `zipFiles` are absent, pending the open question.

### Documentation

- The virtual-file-system guide from proposal 0010 gains a section on archives. It covers `ZipProvider`, archive sources for `--vfs-load`, the read-only archive mount, and the safety notes on verbatim names, declared sizes, and in-place edits.
- [Node Compatibility](../guides/node-compatibility.md) lists the `node:zlib` ZIP classes.

## Compatibility

- **Guest code.** The change is additive: `node:zlib` gains five names, and `vfs.ZipProvider` stops throwing `NotCapable`.
- **CLI.** `--vfs-load` accepts archive files that proposal 0014 rejected.
- **Dependencies.** `ZipProvider` and the `node:zlib` classes depend on proposal 0010. Archive sources depend on proposal 0014 as well.
- Nothing stored needs migrating.

## Implications on adoption

- The workspace gains one dependency for the ZIP format; the `zip` crate is the candidate. It reuses the deflate and Zstandard crates already locked.
- The `zip` feature lets embedded builds leave ZIP support out.
- Nothing changes for users who do not use archives, and no version floor changes.

## Scope

- The ZIP module of `bento_vfs` behind the `zip` feature, covering reading, writing, in-place append, Zip64, stored, deflate, and Zstandard.
- `node:zlib`'s `ZipBuffer`, `ZipFile`, `ZipEntry`, `getMaxZipContentSize`, and `setMaxZipContentSize`.
- `vfs.ZipProvider` and the Rust `ZipProvider`.
- Archive sources for `--vfs-load` and `VfsLoad::source`, including `--watch`.
- Tests derived from the Detailed design. Node expectations come from Domain grounding's observations, and cross-reading tests cover Node and `unzip`.
- The guide updates and changeset named above.

### Out of scope

- Single executable applications' `"vfsArchive"`: [#241](https://github.com/bento-js/bento/issues/241).
- `createZipArchive`, `createZipArchiveSync`, and `zipFiles`, unless the open question brings them in.
- Encrypted and multi-disk archives, which Node does not support either.
- Extracting archives to disk: Node's API never writes members out, and neither does Bento's.

## Preview

- **Artifact.** The installable `bento` archive from the pull request's macOS CI job (`bento-macos27-arm64-prompt`).
- **Exercising it.** The human runs scripts that build, edit, and read archives through `node:zlib` and mount them with `vfs.ZipProvider`, and `bento run --vfs-load=app.zip` against a packaged program. They compare each result with `node --experimental-vfs` and open Bento's output archives with `unzip`.
- **Reason.** Bento publishes no packages, and the CLI exercises both the JavaScript API and the archive source path.
- **Cleanup.** None beyond the CI artifact.

## Policies and decisions checked

- [policy.0001](../policies/0001-prefer-voidzero-crates.md) — Not engaged. Archive handling is not one of its roles, and no Deno or SWC parsing crate is added.
- `decisions/` — No decision records exist yet.
- Proposals 0010 and 0014 (in flight) — Builds on both: on 0010's provider trait, synthetic stats, and permission rule, and on 0014's source choice and messages.
- Proposal 0003 (in flight) — `ZipFile.open` and archive sources use its file permissions.
- [VISION.md](../VISION.md) — Honored: ZIP support is feature-selected, and the behavior is measured against Node's.

## Future directions

- A self-contained `bento` binary (#241) could embed an application archive and mount it as the load mount, as Node's `"vfsArchive"` does.
- `createZipArchive`, `createZipArchiveSync`, and `zipFiles` would complete Node's ZIP archive API, if not added here.

## Alternatives considered

- **Ship `ZipProvider` for `--vfs-load` and Rust only, without the `node:zlib` classes.** It is smaller, but `new vfs.ZipProvider(...)` would still throw, because its argument must be a `ZipBuffer` or a `ZipFile`. This is the subject of the open question.
- **Extract archives to a temporary directory and serve them with `RealFsProvider`.** It writes to disk, exposes temporary paths, and does not match Node's read-only archive mount or its in-place edits.
- **Port Node's JavaScript ZIP implementation.** A Rust host could not use it without running JavaScript, and it would duplicate the provider that archive sources need in Rust.
- **A separate `bento_zip` crate.** A feature inside `bento_vfs` keeps the provider beside the trait it implements and still lets builds omit it.

## Open questions

- [NEEDS CLARIFICATION: Should this proposal implement Node's ZIP archive classes as written: `ZipBuffer`, `ZipFile`, the `ZipEntry` they return, and the content-size ceiling? Or should it implement the whole ZIP archive API, adding `createZipArchive`, `createZipArchiveSync`, and `zipFiles`? Or none of the JavaScript API, shipping ZIP only for `--vfs-load`, `start_vfs_load`, and Rust while `new vfs.ZipProvider(...)` keeps throwing `NotCapable`?]
