# Bento patch to deno_core 0.412.0

This directory is the `deno_core` 0.412.0 crate exactly as published on crates.io (MIT, © the Deno authors; `LICENSE.md` copied from Deno v2.9.7), wired into the workspace through `[patch.crates-io]` in the root `Cargo.toml`. One behavior differs from upstream.

## Change

`ops_builtin.rs`: `op_import_sync` and `op_import_sync_with_source` back CommonJS `require()` of an ES module. Upstream, when the module's evaluation throws, or the module is already in the `Errored` state, the op converts the exception into a serialized `JsError` and throws a new generic error built from its text. Bento's ops instead rethrow the module's own exception object via `scope.throw_exception(module.get_exception())` (helper `rethrow_module_exception`). Cycle, top-level-await, and loading errors are unchanged.

## Why

Node rethrows the original exception from `require(esm)`. The serialized copy loses the error's class. For example, a Node-API addon that is denied loading raises a `NotCapable` permission error, and after `require()` it reads as an ordinary uncaught error, so a runtime cannot report a permission failure as one. Replacing the op from outside the crate through op middleware is not possible because it depends on `pub(crate)` module-map internals.

## Updating

When the pinned Deno crate set changes, replace this directory with the new published `deno_core`, reapply the change (search for `Bento patch`), or delete the patch if upstream rethrows the original exception.
