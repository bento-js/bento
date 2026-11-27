---
"bento": minor
---

Remove the built-in task runner. `bento run <path>` now always runs the source file at that path: it no longer looks up the top-level `tasks` field of `package.json`, and `bento run dev` reports `no script file at …` unless a file named `dev` exists. Move tasks to your package manager's `scripts` or the task runner you prefer, and have it call `bento run` for each source file. A `tasks` field left in `package.json` is ignored rather than validated. `BENTO_ENV_FILE_KEYS` is no longer set or read, so every variable a launch inherits overrides its `.env` files. Grant flags and an inherited `BENTO_GRANT_CEILING` are now checked before the script is looked up, and the `bento_task` crate and the target-selection API of `bento_workspace` are gone.
