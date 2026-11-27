---
"bento": minor
---

Adopt Node's permission model. `bento run` is now unrestricted unless the command passes `--permission`, and Node's flags say what a restricted launch may do: `--allow-fs-read`, `--allow-fs-write`, `--allow-env`, `--allow-net`, `--allow-child-process`, `--allow-worker`, `--allow-addons`, `--allow-wasi`, `--allow-ffi`, `--allow-inspector`, `--allow-openssl-store`, `--allow-fs-vfs`, and Bento's `--allow-language-model`. Denials throw Node's `ERR_ACCESS_DENIED` and `ERR_DLOPEN_DISABLED` errors, `process.permission.has()` and `drop()` work as in Node, system information is always available, and guest code may start subprocesses.

Breaking: `--allow-read`, `--allow-write`, `--allow-net=HOST`, `--allow-addons=PATH`, and `--allow-language-model=BOOL` are gone; rewrite commands with the table in the Running with Bento guide, and add `--permission` to any command that relied on a bare `bento run` to contain code. Nested launches read their ceiling from `BENTO_PERMISSION_CEILING`. Rust hosts pass a `Permissions` value to `RuntimeConfig::new`, which replaces `Grants`, `GrantRequest`, and `RuntimeConfig.grants`.
