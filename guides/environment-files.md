---
id: guide.environment-files
title: Environment Files
describes: [proposal.0001, proposal.0002, proposal.0004]
---

# Environment Files

## What it does

Before `bento run` starts a script, it loads the project's `.env` files into that launch's environment, beneath the variables `bento` was started with. Bento reads the files your project already has; nothing needs converting to a Bento-specific format.

The project is the nearest directory, from the script's own directory upward, that holds a `package.json`. Bento reads `.env`, `.env.<mode>`, and `.env.local` from that directory, so running a script from another directory still loads its own project's files. If no `package.json` is found, Bento reports an error.

## How to use it

### Write `.env` files

- Write one `NAME=value` per line. Blank lines and lines starting with `#` are ignored, and `export NAME=value` is accepted.
- Unquoted values are trimmed, and a `#` starts a comment. Quote any value that contains `#`.
- `'single'` and `` `backtick` `` quotes keep everything inside them literally.
- `"double"` quotes understand `\n`, `\r`, `\t`, `\"`, and `\\`. Any other backslash stays as written.
- A quoted value may span several lines.
- Nothing is expanded or executed: `$HOME`, `${HOME}`, `$(command)`, and backticks stay literal text.
- A malformed line stops the launch with the file and line number, as in `.env:2: quoted value is never closed`. The value on that line, and the rest of the file, are never printed, because environment files often hold secrets.

### Mode and precedence

Bento picks the mode once per launch:

1. the `NODE_ENV` in the environment `bento` was started with;
2. otherwise the `NODE_ENV` in `.env`;
3. otherwise the launch default: `development` for `bento run --watch`, and no mode for `bento run`.

Then it builds the environment from these layers, each overriding the ones above it:

| Layer | Source                                            |
| ----- | ------------------------------------------------- |
| 1     | `.env`                                            |
| 2     | `.env.<mode>`, only when there is a mode          |
| 3     | `.env.local`                                      |
| 4     | Variables in the environment `bento` started with |

Missing files are skipped. A `NODE_ENV` set inside `.env.<mode>` or `.env.local` changes that variable but not which files load.

The effective `NODE_ENV` is part of the launch environment. A watch launch gets `NODE_ENV=development` even when the project has no `.env` files at all. The default never replaces a value that you or a file set.

### Start `bento` from another program

Every `bento run` loads the files for itself, whoever starts it: your shell, a package script, a task runner, or a shell script. Every variable the starting program passes counts as one you set and overrides every file, even when that program loaded the value from the same `.env` file. A program that runs `bento run --watch server.ts` with `DATABASE_URL` already in its environment therefore keeps that `DATABASE_URL`, whatever `.env.development` says. To let the mode file decide, do not export the variable to `bento`.

### What the script sees

Loading `.env` files never changes the environment of the process that started Bento or of a Rust host embedding it. A loaded value is not automatically visible to guest code either: a script sees only the variables `--allow-env` grants, whether they came from your environment or from a file. [Running with Bento](running-with-bento.md#grant-capabilities) describes the grant.

Bento reads `.env` files when a launch starts. To pick up a change, restart the launch; watch mode does not reload them.

## Worked example

The project holds `.env` containing `PORT=1612` and `DATABASE_URL=db/data.db`, and `.env.development` containing `DATABASE_URL=db/development.db`. Nothing is exported, and each command passes `--allow-env=PORT,DATABASE_URL,NODE_ENV` so the script can see the three variables:

1. `bento run server.ts` has no mode, because `bento run` has no default and neither your environment nor `.env` sets `NODE_ENV`. The script sees `PORT=1612` and `DATABASE_URL=db/data.db`, and no `NODE_ENV`.
2. `bento run --watch server.ts` selects the `development` mode, loads `.env` and then `.env.development`, and starts the script with `PORT=1612`, `DATABASE_URL=db/development.db`, and `NODE_ENV=development`.
3. `DATABASE_URL=db/mine.db bento run --watch server.ts` keeps `DATABASE_URL=db/mine.db`, because a value you set always beats the files.
4. `NODE_ENV=test bento run server.ts` selects the `test` mode and loads `.env.test` after `.env`, if the project has one.

## Limits and edge cases

- Bento does not reload `.env` files while a launch runs, including in watch mode.
- A mode is used as part of a file name, so a `NODE_ENV` containing `/` is rejected. An empty `NODE_ENV` that you set means no mode, and Bento leaves it empty.
- Bento needs every environment variable it starts with to be valid UTF-8, and reports the first one that is not by name.
