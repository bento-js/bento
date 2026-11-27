---
id: guide.tasks-and-environment
title: Tasks and Environment
describes: [proposal.0001, proposal.0002, proposal.0003]
---

# Tasks and Environment

## What it does

`bento run <name>` runs a task declared in the top-level `tasks` field of your project's `package.json`, after the tasks it depends on. Before Bento starts a task or a script, it loads the project's `.env` files into that launch's environment. Both features read the files your project already has; nothing needs converting to a Bento-specific format.

A launch without permission flags is unrestricted, as in Node, so running code you do not trust requires `--permission`. For a task, `--permission` and the `--allow-*` flags restrict the `bento` launches its commands start, never the commands themselves. [Running with Bento](running-with-bento.md#control-what-code-may-do) lists the flags; this guide covers what they do to the environment and to nested launches.

For a task, Bento finds the project by looking for the nearest `package.json` in the current directory or one of its parents. For a script, it starts from the script's own directory instead. The directory holding that `package.json` is the project root: tasks run there, and `.env` files and `node_modules/.bin` are read from there. If no `package.json` is found, Bento reports an error.

`bento run <target>` treats the target as a file when it contains a path separator (`./server.ts`, `scripts/seed.ts`) or when it has a file extension and that file exists (`server.ts`). Otherwise an exact task name wins over a file of the same name. A target that is neither a task nor a file is an error; Bento never runs it as a shell command.

## How to use it

### Declare tasks

| Shape | Example | Meaning |
| --- | --- | --- |
| A string | `"lint": "oxlint --fix"` | One command. |
| An array of strings | `"check": ["oxfmt --check", "oxlint", "tsc"]` | Commands that run one after another. The first failure stops the rest. |
| An object | `"dev": { "dependsOn": ["db:migrate"], "command": "bento run --watch server.ts" }` | `dependsOn` lists tasks to run first; `command` is one command string. Either may be left out, but not both. |

An object with only `dependsOn` groups other tasks: it succeeds when they all succeed.

Bento checks the whole selected graph before it runs anything: the shape of every task in it, every dependency name, and cycles. It rejects empty commands, empty arrays, an object with neither a `command` nor a dependency, a `command` that is not a string, a `dependsOn` that is not a list of task names, and object fields other than `command` and `dependsOn`. Errors name the task, the missing dependency, or the cycle (`a → b → a`). Tasks outside the selected graph are not checked.

`scripts` are not tasks. Bento never converts them and never runs npm-style `pre` and `post` hooks, so a `predev` script does not run before the `dev` task.

### Run them in order

Dependencies run depth-first in the order they are declared, then the task's own command or commands. A task reached through several dependencies runs once per invocation. Tasks never run in parallel.

If a command fails, nothing after it runs: not the rest of its array, not the tasks that depend on it, and not any later task. Bento reports the task, the command, and the exit code or the signal that ended it.

### Pass arguments

Arguments after `--` are appended to the selected task's own command, shell-quoted, so spaces, quotes, `$`, and `;` arrive exactly as you typed them. Dependencies never receive them. A task with only `dependsOn` has no command to receive them, and a task with an array of commands would leave the target ambiguous, so both reject arguments.

### What a command runs with

Each command runs with `/bin/sh -c` in the project root, so inline assignments such as `NODE_ENV=test remix test`, quoting, and operators like `&&`, `||`, `;`, and `|` behave as they do in your shell. `PATH` starts with `<root>/node_modules/.bin`, then the directory of the running `bento` executable, then the inherited `PATH`, so installed package tools are found by name. Ahead of all of these, Bento adds a private directory holding only a link to itself, so `bento` in a command always means the running `bento`, even when the project installs a `bento` of its own in `node_modules/.bin`. The command receives the launch environment described below, and its stdout and stderr pass straight through.

Running a task is a trusted operation on your behalf. Its commands, and the executables they start, run with your user's full authority whatever permission flags the launch has. Package tools such as `oxfmt`, `tsc`, or `remix db migrate` may run under Node.

### Cancel and clean up

Each command runs in its own process group. When Bento receives an interrupt (Ctrl-C) or a termination signal, it forwards that signal to the whole group, including servers started by nested commands. Processes that are still running after a 10-second grace period are killed.

A command finishes when its shell exits. If the shell leaves processes running in the background, as `server &` does, Bento sends them a termination signal, kills any that remain after the same grace period, and then reports the shell's exit status. If Bento is interrupted or terminated while it waits for them, it forwards that signal to them as well and reports the task as cancelled instead; whatever remains is still killed when the original grace period ends. Either way, Bento starts the next command, or returns, only after every process in the group has exited, so no orphaned server keeps holding its port.

### Nest `bento` commands

A task command that runs `bento` again receives two variables from the launch that started the task:

- `BENTO_PERMISSION_CEILING` carries that launch's permissions, unrestricted or restricted. The nested launch can never hold more.
- `BENTO_ENV_FILE_KEYS` records which variables Bento loaded from `.env` files, with a fingerprint of each loaded value. The nested launch treats those variables as file values it may load again for its own mode, rather than as values you exported. A variable the task command assigns for the nested launch is left out, so it stays explicit even when it repeats the value from `.env`: `DATABASE_URL=base bento run --watch server.ts` keeps `DATABASE_URL=base`. A variable whose value no longer matches its fingerprint is explicit too.

These variables describe the trusted host's policy; they are not authority that guest code can claim.

The ceiling decides what a nested launch holds:

- Without `--permission`, the nested launch holds exactly its ceiling's permissions. Under an unrestricted launch it is unrestricted; under a restricted one it is restricted the same way. This is how the guest book's `dev` task passes its launch's flags to `bento run --watch server.ts` without repeating them.
- With `--permission`, the nested launch holds exactly what its own flags and its entrypoint allow, the same as a launch from your shell. Each flag must fit within the ceiling: a flag that asks for more fails before any guest code runs, naming the flag and the inherited limit, and the task fails with it. Permissions its flags do not mention are denied even when the ceiling holds them, so `bento run --permission --allow-net server.ts` under a launch that allowed reading files cannot read anything but its entrypoint.
- The nested launch's entrypoint is readable without a flag only when the ceiling's read permission covers it.

Every permission follows these rules, the local language model included. A process that a restricted guest starts receives the guest's permissions at that moment in `BENTO_PERMISSION_CEILING` too, so a `bento` it runs is bounded the same way. A launch started from your shell has no ceiling.

Bento reads a command's own assignments from its text, following the shell's scoping rules:

- `NAME=value` words before a command apply to that command alone. `DATABASE_URL=base true; bento run --watch server.ts` leaves `DATABASE_URL` to the nested launch's mode file, because the assignment never reached `bento`.
- A `NAME=value` statement on its own, and the `NAME=value` arguments of `export` and `readonly`, apply to every command in the task command.
- The `NAME=value` operands of `env`, after its options such as `-i`, `-u NAME`, and `--`, apply to the command `env` runs: `env -u UNUSED DATABASE_URL=base bento run --watch server.ts` keeps `DATABASE_URL=base`. An `env -S` string is split into words the way `env` splits it, so `env -S 'DATABASE_URL=base bento run --watch server.ts'` keeps it too. When `env` runs no command, as in `env DATABASE_URL=base > /dev/null`, it prints the environment and its assignments reach nothing.
- `command`, `builtin`, `exec`, and `nohup` run the command that follows their options with the same environment, so Bento reads that command as well: `nohup env DATABASE_URL=base bento run --watch server.ts` keeps `DATABASE_URL=base`. `command` and `builtin` run a builtin in the shell itself, so `command export DATABASE_URL=base` assigns for every later command just as `export` does. `exec`, `nohup`, and `env` start a program found on `PATH` instead, where no name is a builtin: `nohup export DATABASE_URL=base` assigns nothing, and every name there other than `env` and `nohup`, even `test` or `command`, is a program that may start `bento`, so `env DATABASE_URL=base command true` and `DATABASE_URL=base exec test` count. With `-v` or `-V`, `command` only describes the command that follows and runs nothing, so `command -v export DATABASE_URL=base` assigns nothing, and `DATABASE_URL=base command -v bento` reaches nothing.
- A path to `env` or `nohup`, such as `/usr/bin/env` or `./bin/env`, counts as that utility, because each is also a program: `/usr/bin/env DATABASE_URL=base bento run --watch server.ts` keeps `DATABASE_URL=base` just as `env` does. `command` and `exec` are shell builtins, so a path such as `./bin/command` names some other program, which may start `bento` whatever its arguments: `DATABASE_URL=base ./bin/command true` counts.
- A backslash that ends a line joins that line to the next before Bento reads the words, as the shell does, so a command split across lines reads as one: in `package.json`, `"env \\\n  DATABASE_URL=base bento run --watch server.ts"` keeps `DATABASE_URL=base`. The lines join inside double quotes too; inside single quotes the backslash and the newline stay as written.
- Quotes are removed first, so `env "DATABASE_URL=base" …` and `export 'DATABASE_URL'=base` count, while `echo DATABASE_URL=base`, a comment, and text inside quotes do not. Redirections such as `> /dev/null` and `2>&1` are not words of the command. A here-document's body is its command's input, not command text, so a `DATABASE_URL=base` line between `cat <<EOF` and `EOF` does not count. The shell does run a command substitution, `$(…)` or backticks, in a body whose delimiter is unquoted, so Bento reads each line of that body as command text, and, since it does not find where a substitution ends, the text from each substitution to the body's end: a body line `$(DATABASE_URL=base bento run --watch server.ts)` keeps `DATABASE_URL=base`, even after a `#` or inside quotes, which in a body are only text, and a `DATABASE_URL=base` line in the same body counts too. A backslash before `$(` or a backtick escapes it, as in `\$(date)`, so it runs nothing, and a body with no other substitution stays input.

Bento cannot see which program a command starts, so it assumes any program may be, or may start, a nested `bento`: `DATABASE_URL=base remix dev` keeps the value explicit for a `bento` that `remix` starts. Only shell builtins that start no program, such as `true`, `false`, `echo`, `printf`, `test`, `cd`, `read`, and `wait`, are known to reach nothing, and only by their bare names where the shell finds them: a path such as `./test`, or a `test` that `env`, `exec`, or `nohup` finds on `PATH`, may be a project script in `node_modules/.bin` that starts `bento`, so `DATABASE_URL=base ./test` and `env DATABASE_URL=base test` count. A prefix assignment on a special builtin such as `:` or `exec` persists in the shell, as POSIX specifies, and counts.

### Write `.env` files

- Write one `NAME=value` per line. Blank lines and lines starting with `#` are ignored, and `export NAME=value` is accepted.
- Unquoted values are trimmed, and a `#` starts a comment. Quote any value that contains `#`.
- `'single'` and `` `backtick` `` quotes keep everything inside them literally.
- `"double"` quotes understand `\n`, `\r`, `\t`, `\"`, and `\\`. Any other backslash stays as written.
- A quoted value may span several lines.
- Nothing is expanded or executed: `$HOME`, `${HOME}`, `$(command)`, and backticks stay literal text.
- A malformed line stops the launch with the file and line number. The value on that line is never printed.

### Mode and precedence

Bento picks the mode once per launch:

1. the `NODE_ENV` you set in the environment Bento started with;
2. otherwise the `NODE_ENV` in `.env`;
3. otherwise the launch default: `development` for `bento run --watch`, and no mode for anything else.

Then it builds the environment from these layers, each overriding the ones above it:

| Layer | Source                                                                         |
| ----- | ------------------------------------------------------------------------------ |
| 1     | `.env`                                                                         |
| 2     | `.env.<mode>`, only when there is a mode                                       |
| 3     | `.env.local`                                                                   |
| 4     | Variables you set in the environment, and inline assignments in a task command |

Missing files are skipped. A `NODE_ENV` set inside `.env.<mode>` or `.env.local` changes that variable but not which files load.

The effective `NODE_ENV` is part of the launch environment. A watch launch gets `NODE_ENV=development` even when the project has no `.env` files at all. The default never replaces a value that you or a file set.

Loading `.env` files never changes the environment of the process that started Bento or of a Rust host embedding it. An unrestricted guest sees the whole prepared environment, and task commands always receive all of it. A restricted guest sees what the next section describes.

Bento reads `.env` files when a command starts. To pick up a change, restart the command; watch mode does not reload them.

### Environment in a restricted launch

When a launch with `--permission` starts, Bento removes every variable that `--allow-env` does not match, except:

- the variables Node always keeps because Node and its dependencies read them, such as `PATH`, `HOME`, `TMPDIR`, `TZ`, `LANG`, the `LC_*` locale variables, `NODE_OPTIONS`, and `NODE_PATH`. The full list is Node's, from [`src/permission/env_permission.cc`](https://github.com/nodejs/node/blob/v27.0.0-alpha.0.2.0/src/permission/env_permission.cc) at v27.0.0-alpha.0.2.0;
- the values Bento prepared for the launch: those read from the project's environment files, and the watch-mode `NODE_ENV` default. They are Bento's counterpart of the values Node keeps from the files its `--env-file` option names.

When a variable comes from both an environment file and the inherited environment and `--allow-env` does not match it, the inherited value is removed and the file's value is kept. An inherited `NODE_ENV` is removed unless `--allow-env` matches it, as in Node, though it still chooses the mode file.

`--allow-env` takes `*`, a variable name, or a name prefix followed by `*`, repeated or separated by commas: `--allow-env=PORT,DATABASE_URL` keeps two variables, and `--allow-env='APP_*'` keeps every variable whose name starts with `APP_`.

Reading a removed variable returns `undefined`. The first read of each removed variable emits a process warning with Node's text:

```text
The permission model removed the environment variable "API_TOKEN" at startup. Use --allow-env to manage permissions.
```

Guest code may set, change, and delete any variable while it runs, whether or not `--allow-env` matches it. Those changes affect only that launch's environment, never the `bento` process's own. A subprocess the guest starts receives the guest's environment after the removal, while task commands always receive the whole prepared environment.

`process.permission.has("env", "PORT")` answers for one variable. `process.permission.drop("env", "API_TOKEN")` removes that variable from the environment of the thread that calls it for the rest of the run, and `process.permission.drop("env")` removes every variable except the always-kept ones. As in Node, a worker's environment is a copy of its creator's, and a drop in one thread leaves the others' environments alone. A name passed to `drop` cannot contain `*`: `drop("env", "APP_*")` throws a `TypeError` with code `ERR_INVALID_ARG_VALUE`.

## Worked example

The guest book declares `"dev": { "dependsOn": ["db:migrate"], "command": "bento run --watch server.ts" }` and `"check:fix": { "dependsOn": ["fmt", "lint", "typecheck"] }`, and its `scripts` also contain a `predev`.

`bento run check:fix` runs `oxfmt`, then `oxlint --fix`, then `tsc`, and stops at the first failure.

Suppose the project has `.env` containing `DATABASE_URL=base` and `.env.development` containing `DATABASE_URL=dev`, and nothing is exported:

1. `bento run dev` is not a watch launch and `.env` has no `NODE_ENV`, so it has no mode. Its environment has `DATABASE_URL=base` and no `NODE_ENV`. The `predev` script does not run.
2. `db:migrate` runs `remix db migrate` once, with `DATABASE_URL=base`.
3. The `dev` command `bento run --watch server.ts` inherits `DATABASE_URL=base`, and `BENTO_ENV_FILE_KEYS` marks that value as loaded from a file. As a watch launch, it selects the `development` mode, loads `.env` and then `.env.development`, and starts the server with `DATABASE_URL=dev` and `NODE_ENV=development`.

Running `DATABASE_URL=mine bento run dev` instead keeps `DATABASE_URL=mine` at both levels, because a value you set always beats the files.

In the same project, a script `show-env.ts` containing `console.log(process.env.PORT, process.env.DATABASE_URL, process.env.API_TOKEN, process.env.HOME !== undefined)` shows what a restricted launch keeps:

```sh
PORT=3000 API_TOKEN=secret bento run --permission --allow-env=PORT show-env.ts
```

It prints `3000 base undefined true` and warns that the permission model removed `API_TOKEN`. `--allow-env` matches `PORT`, `DATABASE_URL` comes from `.env`, `API_TOKEN` was inherited and is not matched, and `HOME` is always kept. Without `--permission`, the same command prints `3000 base secret true`.

`bento run test -- --reporter "dot matrix"` runs `NODE_ENV=test remix test --reporter 'dot matrix'`, while `bento run check:fix -- --verbose` fails because `check:fix` has no command of its own.

## Limits and edge cases

- There is no parallel task scheduling, task caching, or hot reloading of `.env` files.
- Bento reads a task command's text statically, without running it, and recognizes assignments only in the forms this guide lists. Other shell constructs may not be recognized: an assignment inside `sh -c '…'` or `eval`, one handed to a program that runs another command, as in `nice env DATABASE_URL=base bento run --watch server.ts`, one made by `.` sourcing a file or by a script the command runs, or one made through a `${NAME}` substitution. Where the text it does recognize cannot decide, Bento keeps your value: a bare assignment or `export` counts wherever it appears in the command, even after the nested launch, and a prefix assignment on any program counts because the program might start `bento`.
- That boundary rarely matters. A value that differs from the one the outer launch loaded from a file is always explicit in a nested launch, because it no longer matches its fingerprint in `BENTO_ENV_FILE_KEYS`. Only an assignment of exactly the file's value relies on Bento reading the command text: when Bento does not recognize it, the nested launch treats the value as loaded from a file and may replace it with its own mode file's value. When that case matters, write the assignment in a form this guide lists, or set the variable with `export` in the task command before the nested launch; a value that differs from the file's needs neither.
- A mode is used as part of a file name, so a `NODE_ENV` containing `/` is rejected. An empty `NODE_ENV` that you set means no mode, and Bento leaves it empty.
- When a signal does not stop a process, the 10-second grace period ends with a kill, which skips any further cleanup inside that process.
