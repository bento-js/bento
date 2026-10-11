# Remix 3 Guest Book

A small [Remix 3](https://remix.run) guest book that runs TypeScript directly on Node.js with no bundler. It follows the layout and conventions of the official [bookstore demo](https://github.com/remix-run/remix/tree/main/demos/bookstore) and the `remix new` starter. Server modules load through [`remix/node-tsx`](https://api.remix.run/api/remix/node-tsx/overview), and [`remix/assets`](https://api.remix.run/api/remix/assets/overview) compiles browser modules on demand.

| Runtime | Package manager | Database      | Toolchain                                                                                         |
| ------- | --------------- | ------------- | ------------------------------------------------------------------------------------------------- |
| Node.js | pnpm            | `node:sqlite` | [Oxc](https://oxc.rs/) (`oxc-transform` via `remix/assets` & `remix/node-tsx`, `oxfmt`, `oxlint`) |

## Getting Started

```sh
pnpm install
node --run db:migrate   # create and migrate the SQLite database
node --run dev          # start the dev server on http://localhost:1612
```

## Configuration

The app and the `remix db` CLI read the same environment variables:

| Variable       | Default      | Used by                                                |
| -------------- | ------------ | ------------------------------------------------------ |
| `DATABASE_URL` | `db/data.db` | `app/env.ts` and `db.adapter.filename` in `remix.json` |
| `PORT`         | `1612`       | `server.ts` and `server.embedded.ts`                   |

Relative database paths resolve from the project root. If you change the default, change it in both `app/env.ts` and `remix.json`.

## Commands

```sh
node --run dev          # dev server; node --watch restarts it on file changes
node --run start        # production-mode server (minified, fingerprinted assets)
node --run test         # remix test (server tests use an in-memory database)
node --run db:migrate   # apply pending migrations
node --run db:rollback  # revert the latest migration
node --run db:status    # list applied and pending migrations
node --run db:reset     # wipe the database and re-run all migrations
node --run fmt          # format with Oxfmt
node --run lint         # lint with Oxlint
node --run check        # check formatting, linting, and typechecking
node --run check:fix    # format, lint, and typecheck
node --run typecheck    # tsc
```

`npx remix doctor` checks that every route map has its controller, and `npx remix routes` prints the route tree with each owner.

## Running with Bento

[Bento](https://github.com/markmals/bento) runs the same TypeScript source directly, without `--import remix/node-tsx`. It reads the top-level `tasks` in `package.json`; the Node `scripts` above stay as they are, and Bento never runs them or their `pre`/`post` hooks.

```sh
bento install --frozen-lockfile
bento run --allow-read --allow-write=./db --allow-net --allow-env=PORT,DATABASE_URL,NODE_ENV --allow-addons=./node_modules dev
bento run --allow-read --allow-write=./db --allow-net --allow-env=PORT,DATABASE_URL,NODE_ENV --allow-addons=./node_modules start
bento run --allow-read --allow-write=./db --allow-net --allow-env=PORT,DATABASE_URL,NODE_ENV --allow-addons=./node_modules server.ts
```

- `bento install --frozen-lockfile` installs `dependencies` and `devDependencies` exactly as `pnpm-lock.yaml` locks them, with the pnpm 12 toolchain that `devEngines` declares, and leaves the lockfile unchanged. pnpm and Node must be on `PATH` to install.
- `dev` runs the `db:migrate` task once, then `bento run --watch server.ts`. Watch mode sets `NODE_ENV=development` unless the environment or `.env` already sets it, so assets keep their source paths and come with source maps. Editing a module the server imports, such as `app/actions/guest-book/page.tsx`, or a browser module under `app/actions/public/` restarts the server, and the next request shows the change. Saving an entry writes the database, which never restarts the server, and entries survive restarts. A syntax error is reported at its line, and the server stays down until you fix it.
- `start` runs `bento run server.ts`. Anything other than `NODE_ENV=development` serves minified assets under fingerprinted URLs, as `node --run start` does.
- `server.ts` starts the server directly. Create the database first with `bento run db:migrate`, or let `dev` do it.

Running the server needs only Bento, not Node, Bun, Deno, or pnpm. Launch it without `NODE_PATH` set: the native Oxc resolver that `remix/assets` loads reads `NODE_PATH` from the Bento process and would look for packages outside the project.

Ctrl-C reaches the server however you start it. `server.ts` hears it itself: it closes its server and database and exits with `0`. Run directly, `bento` exits with that code. Through `dev` or `start`, Bento forwards Ctrl-C to everything the task started, prints that the task was cancelled, and exits with the code the server's `bento` ended with, `0` here.

Bento loads `.env`, then `.env.<mode>`, then `.env.local` from the project root, beneath variables you set yourself. `dev` itself has no mode, so it and the `db:migrate` task it runs read only `.env`; the watch launch it starts selects the `development` mode and so also reads `.env.development`. With `PORT` in `.env` and `DATABASE_URL` in `.env.development`, `dev` migrates the default `db/data.db` but serves the database `.env.development` names, so migrate that one first with the mode set. `dev` then serves on the `.env` port from that database:

```sh
NODE_ENV=development bento run db:migrate
bento run --allow-read --allow-write=./db --allow-net --allow-env=PORT,DATABASE_URL,NODE_ENV --allow-addons=./node_modules dev
```

Bento grants nothing by default. These flags give the app exactly what it uses:

| Grant                                    | Why                                                                                                 |
| ---------------------------------------- | --------------------------------------------------------------------------------------------------- |
| `--allow-read`                           | Source, configuration, `node_modules`, and ancestors' `node_modules`, which `remix/assets` searches |
| `--allow-write=./db`                     | The SQLite database and its journal files                                                           |
| `--allow-net`                            | Listening on `PORT`                                                                                 |
| `--allow-env=PORT,DATABASE_URL,NODE_ENV` | The variables in [Configuration](#configuration) and the asset mode                                 |
| `--allow-addons=./node_modules`          | The native Oxc and Lightning CSS compilers that `remix/assets` loads                                |

With only `--allow-read=.`, the asset pipeline's search outside the project is a permission denial and the server does not start. Without `--allow-addons`, startup fails with a permission denial that names the addon, before any native code loads. A `DATABASE_URL` outside `./db` needs its own write grant. Native addons run as native code in the Bento process, outside the reach of the other grants, so grant them only to dependencies you trust.

A task's nested `bento` commands inherit these grants as a ceiling: `bento run --watch server.ts` inside `dev` gets the same access without repeating the flags and cannot ask for more. Task commands themselves are trusted host commands, not guest code. Package tools such as `remix db migrate`, `oxfmt`, `oxlint`, and `tsc` run from `node_modules/.bin` under Node, as they do with `node --run`, so the tasks that run them need Node on `PATH` but no grants. `bento run check` runs `oxfmt --check`, `oxlint`, and `tsc` in order and stops at the first that fails; `bento run check:fix` runs the `fmt`, `lint`, and `typecheck` tasks in order.

Under Bento, `server.ts` stores entries with `node:sqlite` through `remix/data-table/sqlite`, the same as under Node. A Rust application embedding Bento runs `server.embedded.ts` instead: it serves the same router, but the embedding host owns the storage and exposes it through a private `host:guestbook` module, so this entrypoint never opens a SQLite file. The stock `bento` CLI does not provide that module, so `bento run server.embedded.ts` fails to resolve it.

## Project Layout

- `app/routes.ts` is the typed route contract. Server code and browser modules both use it to build URLs.
- `app/router.ts` exports `createAppRouter({ guestBook })`, which installs the middleware and maps each route map to its controller. `server.ts` passes in a store over the real database, `server.embedded.ts` passes in the embedding host's store, and tests pass in one over an in-memory database.
- `app/data/guest-book-store.ts` is the storage boundary: a `GuestBookStore` lists every entry oldest first and appends new ones. Entries have the same shape from every store, with `createdAt` as the UTC text SQLite's `CURRENT_TIMESTAMP` produces (`YYYY-MM-DD HH:MM:SS`).
    - `data-table-store.ts` reads and writes the `guest_book` table (`schema.ts`) with `remix/data-table`.
    - `host-store.ts` forwards to `host:guestbook` (declared in `host-guestbook.d.ts`) and converts its RFC 3339 timestamps with `created-at.ts`.
- `app/actions/` mirrors the route map:
    - `controller.ts` owns the root leaves (the `/assets/*` handler).
    - `guest-book/controller.tsx` owns `routes.guestBook`. It renders the page from `context.guestBook`, validates submissions with `schema.ts`, re-renders invalid submissions with field errors (`400`), and redirects with `303` after a successful save.
    - Route-local UI (`page.tsx`, `entry-list.tsx`, `sign-form.tsx`) lives next to its controller.
- Colocated `public/` directories hold browser-reachable source. `remix.json` serves only those files plus `app/routes.ts`.
    - `app/actions/public/` contains the browser runtime entry and the preflight stylesheet.
    - `app/actions/guest-book/public/` contains the hydrated `MessageField` (character counter) and the `SignButton` (pending state).
    - `app/ui/public/` contains the design tokens (`theme.tsx`) and the app-owned style mixins `button()`, `inputStyle`, and the field group styles. They replace the styled modules that `@remix-run/ui` removed.
- `app/ui/document.tsx` is the shared HTML document. It renders the theme, the stylesheet, the import map, and the module script.
- `db/migrations/` holds the SQL migrations that `remix db` applies.
