---
id: guide.guest-book-embedding-example
title: Guest Book Embedding Example
describes: [proposal.0001]
---

# Guest Book Embedding Example

## What it does

`bento-guestbook-host` is a small Rust application that runs the Remix 3 guest book inside its own process. It is the worked example for embedding Bento: the same runtime, module loader, TypeScript transform, and Node compatibility that the `bento` command uses, driven from Rust instead of from a command line.

The example demonstrates four embedding features through one application:

- **A host module.** The host registers `host:guestbook` for its runtime instance before loading the application. The guest book's embedded entrypoint, `server.embedded.ts`, imports it and awaits `list()` and `append({ name, message })`. The module is ordinary embedding API: a name, and asynchronous Rust functions that take and return JSON values. Nothing in the runtime knows what a guest book is.
- **Rust-owned storage.** Entries live in a SQLite database that Rust opens through `rusqlite`, with the host's own table (`id`, `name`, `message`, `createdAt`). The guest never learns the database path, has no write grant, and cannot read the file through `node:fs`; it only sees the entries the host chooses to return.
- **Grants and lifecycle control.** The host decides what the guest may do: read the project and the package lookup locations above it, open network listeners, see `PORT`, `NODE_ENV`, and `DATABASE_URL`, and load the native compiler addons under the project's `node_modules`. The host also owns shutdown: Ctrl-C or `SIGTERM` to the host asks the runtime to stop, waits for the application's own `SIGTERM` handler, and reports the outcome. The runtime installs no OS signal handlers of its own.
- **Persistence across instances.** Because Rust owns the database, entries survive disposing and recreating the runtime and restarting the host process. `entries` reads them back without starting a runtime at all.

The stock `bento` command does not provide `host:guestbook`; `bento run server.embedded.ts` fails to resolve the import. The CLI entrypoint, `server.ts`, keeps the application's `node:sqlite` storage instead.

## How to use it

### Build

The example is a workspace member crate. Every command in this guide runs from the repository root.

```sh
cargo build --release -p bento_guestbook_host
```

The executable is `target/release/bento-guestbook-host`; Cargo does not put it on `PATH`, so the commands below name it by that path. Its build script exports the Node-API symbols that the guest book's native compiler packages (`oxc-transform`, `oxc-parser`, `oxc-minify`, `oxc-resolver`, `lightningcss`) resolve when they load.

### Prepare a project

The host runs a guest book project whose dependencies are installed. For the fixture in this repository, install them in place, still from the repository root:

```sh
pnpm --dir fixtures/remix-guest-book install --frozen-lockfile
```

The host does not install packages or run migrations. The application's own `db/` migrations belong to the `node:sqlite` path; the host creates its own table on first use.

### Serve

```sh
target/release/bento-guestbook-host serve --project fixtures/remix-guest-book --database /tmp/guest-book.db
```

| Flag | Meaning |
| --- | --- |
| `--project <dir>` | The guest book project: the fixture or a copy of it with installed dependencies. Required. |
| `--database <path>` | The SQLite file the host owns, outside every path the guest may read. Created, with its table, when it does not exist. Required. |
| `--port <N>` | The port the application listens on. Default `1612`, the application's own default. |
| `--read-only` | Refuse `append` at the Rust boundary. `list` still works. The database must already exist. |
| `--mode development\|production` | The `NODE_ENV` the guest sees. `development` logs each request and serves unminified assets with source maps; `production` serves minified, fingerprinted, immutable assets. Default `production`. |

The host starts `server.embedded.ts` with these grants and nothing more:

- **Read:** the project directory, plus Node's package lookup locations above it: `<ancestor>/node_modules` for the project and each of its ancestors up to `/`. The application's asset discovery probes those directories with `fs.realpathSync` while it resolves optional packages, and the runtime keeps a probe outside the grant a permission error rather than pretending the directory is absent. The host computes the list with `bento_runtime::package_lookup_paths` and grants it explicitly; the runtime adds no permissions on its own.
- **Write:** nothing. The application has nothing to write, because the host owns the database.
- **Net:** all, so the application can listen on its port.
- **Env:** `PORT`, `NODE_ENV`, and `DATABASE_URL`. The host sets the first two; it never sets `DATABASE_URL`, so the guest sees it as undefined and the database location stays on the Rust side.
- **Addons:** `<project>/node_modules`, where the native compiler packages live.

The read grant is why the database must lie outside the project and outside every `node_modules` directory beside or above it: a store under any of those paths would be readable through `node:fs` like any other file there. `serve` refuses such a `--database` before it opens anything, naming the granted path the store would fall under, and compares resolved paths so a symlink cannot hide the placement.

Launch the host without `NODE_PATH` in its environment. The guest book's native resolver reads that variable from the host process, not from the guest's environment, and a value pointing at another project's packages sends resolution outside the project and into a permission error under these grants.

Open `http://localhost:1612`. The page renders the saved entries and the sign form; the module scripts, import map, and stylesheet it references are served by the same instance. A valid submission answers `303 See Other` with `Location: /` and the entry appears on the next page; blank or over-long fields answer `400` with field errors and write nothing. Plain HTML submission works without browser JavaScript.

Press Ctrl-C (or send `SIGTERM`) to stop. The host takes Ctrl-C and `SIGTERM` over before it starts the runtime, so a signal that arrives while the application is still starting ends the instance the same way as one that arrives while it serves. The host asks the runtime to shut down, the application's `SIGTERM` handler closes its server and exits, and the host prints the outcome on one line:

| Printed | Meaning | Host exit status |
| --- | --- | --- |
| `shutdown: graceful (exit code N)` | The application exited itself during the shutdown, with `N`. The guest book's handler exits with `0`. | `N` |
| `shutdown: graceful (closed by the host on SIGTERM)` | The application had no handler for the signal, so the runtime closed its listeners and cancelled its work. | `143` after `SIGTERM`, `130` after Ctrl-C, as a process ended by that signal reports |
| `shutdown: interrupted` | JavaScript had to be terminated at the five-second deadline. | `1` |
| `shutdown: timed out` | Native code did not yield by the deadline. | `1` |

After a graceful shutdown the port is free to bind again immediately, and the whole shutdown completes within the five-second deadline.

### Inspect the store

```sh
target/release/bento-guestbook-host entries --database /tmp/guest-book.db
```

This reads the Rust-owned table directly and prints one JSON object per line, oldest first, with the same fields the host module returns to the guest:

```json
{
    "id": 1,
    "name": "Ada Lovelace",
    "message": "Hello from 1843",
    "createdAt": "2026-10-10T18:30:05Z"
}
```

No runtime starts, so this is how a reviewer confirms that persistence is Rust's: sign the guest book, stop the host, and the entries are still in the file. Starting the host again on the same `--database` serves them. An empty store prints nothing; a path with no database is an error, and `entries` never creates one.

### Demonstrate read-only

```sh
target/release/bento-guestbook-host serve --project fixtures/remix-guest-book --database /tmp/guest-book.db --read-only
```

The page still lists every entry. Submitting a valid signature does not redirect: `append` fails inside Rust with a `PermissionDenied` error before SQLite is touched, the rejection reaches the application as a JavaScript `Error` named `PermissionDenied`, and the application's request listener answers `500 Internal Server Error` while logging the error, with that name, to the host's standard error. The database file is byte-for-byte unchanged, which `entries` and a file hash both confirm.

Read-only is enforced where the operation happens, not by hiding the function: the guest can still call `append`, and the host refuses it.

### Verify

```sh
cargo test -p bento_guestbook_host
```

The tests exercise the real guest book, in-process and as the spawned binary, in both modes: rendering and assets, signing and validation, persistence across instance disposal and host restarts, `--read-only`, instance isolation, signal shutdown with the port rebinding, and the guest's inability to read the store. They run against a disposable copy of the fixture under `/tmp/bento-host/fixture`, mirrored with `rsync` and installed with `pnpm install --frozen-lockfile` on first use, and launch every guest without `NODE_PATH`.

## Worked example

```text
$ target/release/bento-guestbook-host serve --project fixtures/remix-guest-book --database /tmp/guest-book.db --port 1612
Server running at http://localhost:1612

# In a browser: sign as "Ada Lovelace" with the message "Hello from 1843", see the entry listed.
^C
shutdown: graceful (exit code 0)

$ target/release/bento-guestbook-host entries --database /tmp/guest-book.db
{"id":1,"name":"Ada Lovelace","message":"Hello from 1843","createdAt":"2026-10-10T18:30:05Z"}

$ target/release/bento-guestbook-host serve --project fixtures/remix-guest-book --database /tmp/guest-book.db --read-only
Server running at http://localhost:1612

# The browser still shows Ada's entry; signing again answers 500, the host's stderr shows
# PermissionDenied: the guest book is read-only, and the file is unchanged.
```

## How this generalizes

A host module is the embedding API, not guest-book support in the runtime. Any Rust application registers its own modules the same way: choose a `host:` name, supply asynchronous functions over JSON values, and hand the registration to the runtime configuration before loading the entry module. The runtime resolves the specifier only for the instance that registered it; another instance in the same process, or the `bento` command, gets a resolution error with no package or network fallback, while the registering instance keeps serving.

Errors cross the boundary as rejected promises carrying the Rust error's name, so a guest can distinguish a refused operation from a crashed one. Grants are enforced at the native boundary regardless of what a module exposes: handing the guest `host:guestbook` does not let it open the database file, and would not even if the module returned the path.

Replacing the storage behind `host:guestbook` changes only this example. The runtime, the resolver, the transforms, and the application's router, validation, rendering, and browser components stay the same.

## Limits and edge cases

- Building the example needs the pinned Rust toolchain. Preparing the project needs pnpm and Node, which the fixture's package tools use; serving it afterwards needs only the built executable, the project files, and its installed dependencies. The tests also need `rsync`.
- `--read-only` opens the database read-only and therefore needs it to exist. Run `serve` without the flag once, or point at a store `entries` can already read.
- The database must be outside every granted read path: the project directory and each `node_modules` directory beside or above it. `serve` refuses a `--database` whose resolved path lies under one of them, and the examples above use `/tmp/guest-book.db` for that reason.
- Granting the package lookup locations grants whatever they resolve to. An ancestor whose `node_modules` is a symlink, as this repository's worktrees link theirs from the main checkout, makes the link's whole target tree readable; an ancestor other users can write to, such as a shared temporary directory, lets them place a readable `node_modules` there. Serve from a project whose ancestors you trust.
- The host's table is its own. It does not read the application's `db/` migrations or the `guest_book` table that the `node:sqlite` path creates, and the two storage paths do not share a file.
- The host module accepts entries, not SQL: `append` takes a `name` and a `message`; the host assigns `id` and `createdAt`. Validation of the fields stays in the application.
- Enabling the native compiler addons runs trusted native code in the host process. The grants contain the application's JavaScript, not an arbitrary addon, and native code reads the host's own environment, which is why `NODE_PATH` must not be set.
- Shutdown that reaches the five-second deadline is reported as `interrupted`, never as graceful, and the interrupted instance is disposed rather than reused.
