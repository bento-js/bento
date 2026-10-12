---
id: guide.installing-dependencies
title: Installing Dependencies
describes: [proposal.0001]
---

# Installing Dependencies

## What it does

`bento install` installs a project's `dependencies` and `devDependencies` by running the project's declared pnpm toolchain in the project root. Bento does not implement a second package manager or lockfile writer: pnpm reads and writes `pnpm-lock.yaml`, including the document that pins pnpm's own version, and applies `pnpm-workspace.yaml`.

Installing is a separate step from running. `bento run` never installs packages, resolves new versions, or downloads modules.

## How to use it

1. Declare pnpm in `package.json`, if the project does not already:

    ```json
    "devEngines": { "packageManager": { "name": "pnpm", "version": "12" } }
    ```

2. Make sure `pnpm` is on your `PATH`. Bento looks it up the way your shell does: a relative entry such as `./tools` is taken from the directory you run `bento install` in, and an empty entry means that directory itself. Bento runs the `pnpm` it finds there, and pnpm switches to the version the project pins, which may need registry access the first time.
3. Run `bento install` anywhere inside the project. Bento finds the nearest `package.json`, runs `pnpm install` in that directory, shows pnpm's output, and exits with pnpm's exit status when the installation fails.
4. Use `bento install --frozen-lockfile` to reproduce exactly the locked dependency graph. If `package.json` and `pnpm-lock.yaml` disagree, the installation fails and `pnpm-lock.yaml` is left byte-for-byte unchanged.

Without `--frozen-lockfile`, pnpm's normal behavior applies: it updates `pnpm-lock.yaml` to match `package.json`. That includes pnpm's own rule that a lockfile is frozen by default when the `CI` environment variable is set.

Bento stops before running anything when `devEngines.packageManager` is missing or names another package manager, and tells you what to declare. When `pnpm` is not on `PATH`, it reports pnpm as a missing prerequisite.

### Development dependencies

`devDependencies` are always installed, even when `NODE_ENV=production` is set in your environment: Bento runs pnpm with `--production=false`. Project tasks rely on them for formatting, linting, type checking, and migrations.

### Build scripts

Bento never approves dependency build scripts. It passes no flag or environment variable that allows them, so pnpm applies the project's own policy, the `allowBuilds` entries in `pnpm-workspace.yaml`. A dependency whose build script is not approved does not run it, and pnpm 12 fails the installation and names the package. If you trust that package, approve it in `pnpm-workspace.yaml`, for example with `pnpm approve-builds`, and install again.

### Running without the installer

Once dependencies are installed, `bento run` and an embedding Rust host execute the project without pnpm, Node, Bun, or Deno on the `PATH`. A dependency that is not installed fails at the import that needs it; run `bento install` and try again.

## Worked example

The guest-book fixture declares `"devEngines": { "packageManager": { "name": "pnpm", "version": "12", "onFail": "download" } }` and its `pnpm-lock.yaml` pins pnpm 12.11.2.

```sh
cd fixtures/remix-guest-book
bento install --frozen-lockfile
```

pnpm switches to 12.11.2 and installs `remix@3.0.0` with the formatter, linter, and type checker from `devDependencies`, exactly as locked. If someone had added a dependency to `package.json` without updating the lockfile, the same command would fail with pnpm's outdated-lockfile error and leave `pnpm-lock.yaml` untouched.

## Limits and edge cases

- pnpm is the only supported package manager, declared as a single `devEngines.packageManager` object.
- Installing may need Node, pnpm, and registry access. Only running the installed project is free of them.
- `bento install` passes pnpm the environment it was started with. It does not load `.env` files, and pnpm settings from `.npmrc` files or `pnpm_config_*` variables still apply.
