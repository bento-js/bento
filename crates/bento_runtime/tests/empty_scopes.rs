mod support;

use std::collections::BTreeSet;
use std::net::TcpListener;
use std::path::Path;

use bento_host::{PermissionSet, Permissions, Scope};
use bento_runtime::{ErrorKind, RunOutcome, RuntimeConfig};
use tempfile::tempdir;

use support::{completed, config_with, project_access, run};

/// A restricted instance for `root` holding what `change` makes of [`support::config`]'s
/// permissions.
fn restricted(root: &Path, change: impl FnOnce(&mut PermissionSet)) -> RuntimeConfig {
    let mut permissions = project_access(root);
    change(&mut permissions);
    config_with(root, Permissions::restricted(permissions))
}

#[test]
fn empty_read_scope_denies_loading_the_entry_before_it_runs() {
    let root = tempdir().unwrap();
    let settings = restricted(root.path(), |set| set.fs_read = Scope::Only(BTreeSet::new()));
    let outcome = run(
        settings,
        "entry.mjs",
        "import { writeFileSync } from 'node:fs'; writeFileSync('entry-ran', 'unexpected');",
    );
    let RunOutcome::Failed(error) = outcome else {
        panic!("empty read scope loaded the entry: {outcome:?}");
    };
    assert_eq!(error.kind, ErrorKind::Permission, "{error:?}");
    assert!(!root.path().join("entry-ran").exists(), "denied entry ran");
}

#[test]
fn empty_write_scope_denies_native_file_writes() {
    let root = tempdir().unwrap();
    let settings = restricted(root.path(), |set| set.fs_write = Scope::Only(BTreeSet::new()));
    completed(run(
        settings,
        "write.mjs",
        r#"
import assert from 'node:assert/strict';
import { writeFileSync } from 'node:fs';
const denied = { code: 'ERR_ACCESS_DENIED', permission: 'FileSystemWrite' };
assert.throws(() => writeFileSync('unexpected', 'denied'), denied);
assert.throws(() => Deno.writeTextFileSync('unexpected-deno', 'denied'), denied);
"#,
    ));
    assert!(!root.path().join("unexpected").exists());
    assert!(!root.path().join("unexpected-deno").exists());
}

#[test]
fn withheld_network_denies_native_listening() {
    let root = tempdir().unwrap();
    let settings = restricted(root.path(), |set| set.net = false);
    completed(run(
        settings,
        "listen.mjs",
        r#"
import assert from 'node:assert/strict';
assert.throws(() => {
    const listener = Deno.listen({ hostname: '127.0.0.1', port: 0 });
    listener.close();
}, { code: 'ERR_ACCESS_DENIED', permission: 'Net' });
"#,
    ));
}

#[test]
fn withheld_network_denies_native_connections() {
    let root = tempdir().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let settings = restricted(root.path(), |set| set.net = false);
    completed(run(
        settings,
        "connect.mjs",
        &format!(
            r#"
import assert from 'node:assert/strict';
await assert.rejects(async () => {{
    const connection = await Deno.connect({{ hostname: '127.0.0.1', port: {port} }});
    connection.close();
}}, {{ code: 'ERR_ACCESS_DENIED' }});
"#
        ),
    ));
}

#[test]
fn empty_environment_scope_removes_every_value_at_startup() {
    let root = tempdir().unwrap();
    let mut settings = restricted(root.path(), |set| set.env = Scope::Only(BTreeSet::new()));
    settings.env.insert("BENTO_EMPTY_SCOPE_SECRET".into(), "hidden".into());
    completed(run(
        settings,
        "environment.mjs",
        r#"
import assert from 'node:assert/strict';
assert.equal(process.env.BENTO_EMPTY_SCOPE_SECRET, undefined);
assert.equal(Deno.env.get('BENTO_EMPTY_SCOPE_SECRET'), undefined);
assert.deepEqual(Object.keys(process.env), []);
assert.deepEqual({ ...Deno.env.toObject() }, {});
assert.notEqual((await Deno.permissions.query({ name: 'env', variable: 'BENTO_EMPTY_SCOPE_SECRET' })).state, 'granted');
Deno.env.set('BENTO_EMPTY_SCOPE_SECRET', 'set by the guest');
assert.equal(process.env.BENTO_EMPTY_SCOPE_SECRET, 'set by the guest');
"#,
    ));
}

#[test]
fn withheld_addons_deny_loading_before_dlopen() {
    let root = tempdir().unwrap();
    std::fs::write(root.path().join("denied.node"), "not a shared library").unwrap();
    let settings = restricted(root.path(), |set| set.addons = false);
    completed(run(
        settings,
        "addon.mjs",
        r#"
import assert from 'node:assert/strict';
import path from 'node:path';
assert.throws(() => process.dlopen({ exports: {} }, path.join(process.cwd(), 'denied.node')), {
    code: 'ERR_DLOPEN_DISABLED',
    message: 'Cannot load native addon because loading addons is disabled.',
});
"#,
    ));
}

#[test]
fn workers_inherit_empty_scopes_without_widening_them() {
    let root = tempdir().unwrap();
    std::fs::write(root.path().join("denied.node"), "not a shared library").unwrap();
    std::fs::write(
        root.path().join("worker.mjs"),
        r#"
import assert from 'node:assert/strict';
import { writeFileSync } from 'node:fs';
import { parentPort } from 'node:worker_threads';
import path from 'node:path';
assert.throws(() => writeFileSync('worker-write', 'denied'), { code: 'ERR_ACCESS_DENIED', permission: 'FileSystemWrite' });
assert.throws(() => {
    const listener = Deno.listen({ hostname: '127.0.0.1', port: 0 });
    listener.close();
}, { code: 'ERR_ACCESS_DENIED', permission: 'Net' });
assert.equal(Deno.env.get('BENTO_EMPTY_SCOPE_SECRET'), undefined);
assert.deepEqual({ ...Deno.env.toObject() }, {});
assert.throws(() => process.dlopen({ exports: {} }, path.join(process.cwd(), 'denied.node')), { code: 'ERR_DLOPEN_DISABLED' });
for (const name of ['write', 'net', 'env', 'ffi']) {
    assert.notEqual((await Deno.permissions.query({ name })).state, 'granted', name);
}
if (parentPort) parentPort.postMessage('denied'); else postMessage('denied');
"#,
    )
    .unwrap();
    for node in [false, true] {
        let mut settings = restricted(root.path(), |set| {
            set.fs_write = Scope::Only(BTreeSet::new());
            set.env = Scope::Only(BTreeSet::new());
            set.worker = true;
        });
        settings.env.insert("BENTO_EMPTY_SCOPE_SECRET".into(), "hidden".into());
        let source = if node {
            r#"
import assert from 'node:assert/strict';
import { Worker } from 'node:worker_threads';
const worker = new Worker(new URL('./worker.mjs', import.meta.url));
try {
    const answer = await new Promise((resolve, reject) => { worker.once('message', resolve); worker.once('error', reject); });
    assert.equal(answer, 'denied');
} finally { await worker.terminate(); }
"#
        } else {
            r#"
import assert from 'node:assert/strict';
const worker = new Worker(new URL('./worker.mjs', import.meta.url), { type: 'module' });
try {
    const answer = await new Promise((resolve, reject) => { worker.onmessage = event => resolve(event.data); worker.onerror = reject; });
    assert.equal(answer, 'denied');
} finally { worker.terminate(); }
"#
        };
        completed(run(settings, "entry.mjs", source));
        assert!(!root.path().join("worker-write").exists());
    }
}
