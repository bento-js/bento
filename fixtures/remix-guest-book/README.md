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
| `PORT`         | `1612`       | `server.ts`                                            |

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

## Project Layout

- `app/routes.ts` is the typed route contract. Server code and browser modules both use it to build URLs.
- `app/router.ts` exports `createAppRouter({ db })`, which installs the middleware and maps each route map to its controller. `server.ts` passes in the real database. Tests pass in an in-memory one.
- `app/actions/` mirrors the route map:
    - `controller.ts` owns the root leaves (the `/assets/*` handler).
    - `guest-book/controller.tsx` owns `routes.guestBook`. It renders the page, validates submissions with `schema.ts`, re-renders invalid submissions with field errors (`400`), and redirects with `303` after a successful save.
    - Route-local UI (`page.tsx`, `entry-list.tsx`, `sign-form.tsx`) lives next to its controller.
- Colocated `public/` directories hold browser-reachable source. `remix.json` serves only those files plus `app/routes.ts`.
    - `app/actions/public/` contains the browser runtime entry and the preflight stylesheet.
    - `app/actions/guest-book/public/` contains the hydrated `MessageField` (character counter) and the `SignButton` (pending state).
    - `app/ui/public/` contains the design tokens (`theme.tsx`) and the app-owned style mixins `button()`, `inputStyle`, and the field group styles. They replace the styled modules that `@remix-run/ui` removed.
- `app/ui/document.tsx` is the shared HTML document. It renders the theme, the stylesheet, the import map, and the module script.
- `db/migrations/` holds the SQL migrations that `remix db` applies.
