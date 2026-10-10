# Attribution and provenance

This fixture is derived from Mark Malstrom's Remix 3 guest book at `~/Developer/Playgrounds/remix-guest-book-bento`, commit `327a0eb08a20a0bf39437cb19e1c02ca9c0ffd59` ("Initial commit"). That application follows the layout and conventions of Remix's [bookstore demo](https://github.com/remix-run/remix/tree/main/demos/bookstore) and the `remix new` starter. The Remix packages it depends on are published by Shopify under the MIT license; their license texts ship inside the installed packages and are not copied here.

Only the source checkout's tracked files were imported. Its environment files, database, dependency installation, and generated artifacts were not copied, and the original checkout was not modified.

SHA-256 of the imported files whose provenance proposal.0001 records:

| File             | SHA-256                                                            |
| ---------------- | ------------------------------------------------------------------ |
| `package.json`   | `2f8e8a275157828fe5d933a2f7f50fc8ee601d708b989d14d380db6fb832a250` |
| `tsconfig.json`  | `85f42c638d22c76bf5d945d54b65d712724b81df22d6447eb9a8d4dc731834f6` |
| `pnpm-lock.yaml` | `ee614312c4e9fb2d9ccc5da0b90004e59419915b4f3524096ac4849c87c24fba` |
| `server.ts`      | `4ea6955ccd2afc5492623d229559d8d314cc0dd75f3f0eee735b95a34fb02f58` |

Later commits change this copy only where proposal.0001 allows: the storage boundary, the host entrypoints, and Bento launch configuration. The pinned `remix@3.0.0` dependency and its lockfile graph stay as imported.
