mod support;

use std::path::{Path, PathBuf};

use bento_host::{EnvPattern, PathPattern, PermissionSet, Permissions, Scope};
use bento_runtime::{ErrorKind, RunOutcome};
use support::{completed, config_with, project, project_access, run};

/// Guest helpers: assert that `action` throws Node's denial for `permission`.
const DENIED: &str = r#"
import assert from 'node:assert/strict';
const flags = {
    FileSystemRead: '--allow-fs-read',
    FileSystemWrite: '--allow-fs-write',
    Net: '--allow-net',
    ChildProcess: '--allow-child-process',
    WorkerThreads: '--allow-worker',
};
function assertDenial(error, permission, resource) {
    assert.equal(error.code, 'ERR_ACCESS_DENIED', String(error));
    assert.equal(error.permission, permission);
    assert.equal(error.resource, resource);
    assert.equal(error.message, `Access to this API has been restricted. Use ${flags[permission]} to manage permissions.`);
    assert.equal(error.name, 'Error');
    assert.deepEqual(Object.keys(error), ['code', 'permission', 'resource']);
}
function denied(action, permission, resource) {
    let error;
    try { action(); } catch (caught) { error = caught; }
    assert.ok(error, 'the operation was not denied');
    assertDenial(error, permission, resource);
}
async function rejected(promise, permission, resource) {
    let error;
    try { await promise; } catch (caught) { error = caught; }
    assert.ok(error, 'the operation was not denied');
    assertDenial(error, permission, resource);
}
"#;

fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap()
}

fn pattern(value: &str, base: &Path) -> PathPattern {
    PathPattern::resolve(value, base).unwrap()
}

fn names(patterns: &[&str]) -> Scope<EnvPattern> {
    Scope::only(patterns.iter().map(|pattern| EnvPattern::parse(pattern).unwrap()))
}

fn restricted(root: &Path, set: PermissionSet) -> bento_runtime::RuntimeConfig {
    config_with(root, Permissions::restricted(set))
}

#[test]
fn an_unrestricted_instance_reads_writes_and_starts_anything_without_process_permission() {
    let directory = project();
    let elsewhere = tempfile::tempdir().unwrap();
    let mut settings = config_with(directory.path(), Permissions::unrestricted());
    settings.env.insert("DATABASE_URL".into(), "sqlite:db".into());
    let outside = canonical(elsewhere.path()).join("written.txt");
    completed(run(
        settings,
        "unrestricted.mjs",
        &format!(
            r#"
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import {{ spawnSync }} from 'node:child_process';
import {{ Worker }} from 'node:worker_threads';
assert.equal(process.permission, undefined);
assert.ok(fs.readFileSync('/etc/hosts').length > 0);
fs.writeFileSync({outside:?}, 'outside');
assert.equal(process.env.DATABASE_URL, 'sqlite:db');
assert.equal(typeof os.homedir(), 'string');
assert.equal(typeof os.networkInterfaces(), 'object');
assert.equal(spawnSync('/bin/echo', ['hello'], {{ encoding: 'utf8' }}).stdout, 'hello\n');
const worker = new Worker('require("node:worker_threads").parentPort.postMessage(42)', {{ eval: true }});
assert.equal(await new Promise((resolve) => worker.once('message', resolve)), 42);
await worker.terminate();
"#
        ),
    ));
    assert_eq!(std::fs::read_to_string(outside).unwrap(), "outside");
}

#[test]
fn a_restricted_instance_denies_files_it_was_not_allowed_with_nodes_error() {
    let directory = project();
    let root = canonical(directory.path());
    std::fs::create_dir(root.join("allowed")).unwrap();
    std::fs::write(root.join("allowed/data.txt"), "data").unwrap();
    std::fs::write(root.join("secret.txt"), "secret").unwrap();
    std::fs::write(root.join("secret.mjs"), "export default 1;").unwrap();
    let settings = restricted(
        &root,
        PermissionSet {
            fs_read: Scope::only([pattern("denials.mjs", &root), pattern("allowed", &root)]),
            ..PermissionSet::default()
        },
    );
    completed(run(
        settings,
        "denials.mjs",
        &format!(
            r#"{DENIED}
import fs from 'node:fs';
assert.equal(fs.readFileSync('allowed/data.txt', 'utf8'), 'data');
denied(() => fs.readFileSync('secret.txt'), 'FileSystemRead', 'secret.txt');
denied(() => fs.readFileSync({secret:?}), 'FileSystemRead', {secret:?});
denied(() => fs.writeFileSync('allowed/new.txt', 'x'), 'FileSystemWrite', 'allowed/new.txt');
await rejected(fs.promises.readFile('secret.txt'), 'FileSystemRead', 'secret.txt');
await rejected(import('./secret.mjs'), 'FileSystemRead', {secret_module:?});
denied(() => Deno.readTextFileSync('secret.txt'), 'FileSystemRead', 'secret.txt');
"#,
            secret = root.join("secret.txt"),
            secret_module = root.join("secret.mjs"),
        ),
    ));
}

#[test]
fn file_patterns_match_as_nodes_flag_values_do() {
    let directory = project();
    let root = canonical(directory.path());
    let mut access = project_access(&root);
    access.fs_write =
        Scope::only([pattern("build/*", &root), pattern("cache", &root), pattern("test*", &root)]);
    completed(run(
        restricted(&root, access),
        "patterns.mjs",
        r#"
import assert from 'node:assert/strict';
import fs from 'node:fs';
const refused = (action) => assert.throws(action, { code: 'ERR_ACCESS_DENIED', permission: 'FileSystemWrite' });
fs.mkdirSync('build');
fs.writeFileSync('build/app.js', 'built');
fs.mkdirSync('cache');
refused(() => fs.writeFileSync('cache/entry', 'x'));
fs.writeFileSync('test2.txt', 'x');
fs.mkdirSync('testing');
fs.writeFileSync('testing/file', 'x');
refused(() => fs.writeFileSync('tes.txt', 'x'));
"#,
    ));
}

#[test]
fn a_link_inside_an_allowed_directory_cannot_reach_outside_it() {
    let directory = project();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("secret.txt"), "secret").unwrap();
    let root = canonical(directory.path());
    std::os::unix::fs::symlink(outside.path(), root.join("escape")).unwrap();
    completed(run(
        restricted(&root, project_access(&root)),
        "links.mjs",
        r#"
import assert from 'node:assert/strict';
import fs from 'node:fs';
assert.throws(() => fs.readFileSync('escape/secret.txt'), { code: 'ERR_ACCESS_DENIED', permission: 'FileSystemRead', resource: 'escape/secret.txt' });
assert.equal(process.permission.has('fs.read', 'escape/secret.txt'), false);
"#,
    ));
}

#[test]
fn process_permission_answers_as_node_does() {
    let directory = project();
    let root = canonical(directory.path());
    std::fs::create_dir(root.join("allowed")).unwrap();
    let mut settings = restricted(
        &root,
        PermissionSet {
            fs_read: Scope::only([pattern("has.mjs", &root), pattern("allowed", &root)]),
            env: names(&["APP_*"]),
            net: true,
            ..PermissionSet::default()
        },
    );
    settings.env.insert("APP_PORT".into(), "3000".into());
    completed(run(
        settings,
        "has.mjs",
        &format!(
            r#"
import assert from 'node:assert/strict';
const {{ has }} = process.permission;
const descriptor = Object.getOwnPropertyDescriptor(process, 'permission');
assert.equal(descriptor.enumerable, true);
assert.equal(descriptor.configurable, false);
assert.deepEqual(Object.keys(process.permission), ['has', 'drop']);
assert.equal(has('fs.read'), false);
assert.equal(has('fs.read', 'allowed/file'), true);
assert.equal(has('fs.read', {allowed:?}), true);
assert.equal(has('fs.read', new URL({allowed_url:?})), true);
assert.equal(has('fs.read', new TextEncoder().encode({allowed:?})), true);
assert.equal(has('fs.read', '/etc/hosts'), false);
assert.equal(has('fs.write', 'allowed/file'), false);
assert.equal(has('fs', 'allowed/file'), false);
assert.equal(has('net'), true);
assert.equal(has('net', 'ignored'), true);
for (const scope of ['child', 'worker', 'addon', 'wasi', 'ffi', 'inspector', 'openssl.store', 'language-model', 'fs.vfs', 'bogus']) {{
    assert.equal(has(scope), false, scope);
}}
assert.equal(has('env'), false);
assert.equal(has('env', 'APP_HOST'), true);
assert.equal(has('env', 'HOME'), true);
assert.equal(has('env', 'DATABASE_URL'), false);
assert.throws(() => has(), {{ code: 'ERR_INVALID_ARG_TYPE', message: 'The "scope" argument must be of type string. Received undefined' }});
assert.throws(() => has('fs.read', 5), {{ code: 'ERR_INVALID_ARG_TYPE', message: 'The "reference" argument must be of type string. Received type number (5)' }});
"#,
            allowed = root.join("allowed/file"),
            allowed_url = format!("file://{}", root.join("allowed/file").display()),
        ),
    ));
}

#[test]
fn dropping_removes_exactly_what_it_names_and_reaches_every_worker() {
    let directory = project();
    let root = canonical(directory.path());
    std::fs::create_dir_all(root.join("existing/sub")).unwrap();
    std::fs::write(root.join("existing/sub/file.txt"), "file").unwrap();
    let mut settings = restricted(
        &root,
        PermissionSet {
            fs_read: Scope::only([
                pattern("drop.mjs", &root),
                pattern("existing", &root),
                pattern("existing/sub", &root),
            ]),
            env: names(&["APP_*"]),
            net: true,
            worker: true,
            ..PermissionSet::default()
        },
    );
    settings.env.insert("APP_A".into(), "a".into());
    settings.env.insert("APP_B".into(), "b".into());
    completed(run(
        settings,
        "drop.mjs",
        r#"
import assert from 'node:assert/strict';
import { Worker } from 'node:worker_threads';
const { has, drop } = process.permission;
const worker = new Worker(`
    const { parentPort } = require('node:worker_threads');
    parentPort.on('message', () => parentPort.postMessage([
        process.permission.has('net'),
        process.permission.has('fs.read', 'existing/other'),
        process.permission.has('env', 'APP_A'),
    ]));
`, { eval: true });
drop('fs.read', 'existing/sub');
assert.equal(has('fs.read', 'existing/sub/file.txt'), true, 'still covered by existing');
drop('fs.read', 'existing');
assert.equal(has('fs.read', 'existing/other'), false);
assert.equal(has('fs.read', 'existing/sub/file.txt'), false);
drop('fs.read', 'never-allowed');
drop('net', 'any reference drops the whole scope');
assert.equal(has('net'), false);
drop('env', 'APP_A');
assert.equal(process.env.APP_A, undefined);
assert.equal(has('env', 'APP_A'), false);
assert.equal(process.env.APP_B, 'b');
assert.throws(() => drop('env', 'APP_*'), { name: 'TypeError', code: 'ERR_INVALID_ARG_VALUE', message: 'Wildcards are not supported when dropping environment variables' });
worker.postMessage('report');
const seen = await new Promise((resolve) => worker.once('message', resolve));
// A worker's process.env is its own copy, as in Node; its permissions are its creator's.
assert.deepEqual(seen, [false, false, false]);
await worker.terminate();
"#,
    ));
}

#[test]
fn a_restricted_instance_removes_uncovered_variables_and_warns_on_their_first_read() {
    let directory = project();
    let root = canonical(directory.path());
    let mut settings =
        restricted(&root, PermissionSet { env: names(&["APP_*"]), ..project_access(&root) });
    for (name, value) in [
        ("APP_PORT", "3000"),
        ("DATABASE_URL", "secret"),
        ("NODE_ENV", "production"),
        ("HOME", "/home/guest"),
        ("LC_ALL", "C"),
        ("FROM_FILE", "kept"),
    ] {
        settings.env.insert(name.into(), value.into());
    }
    settings.kept_env.insert("FROM_FILE".into());
    completed(run(
        settings,
        "environment.mjs",
        r#"
import assert from 'node:assert/strict';
const warnings = [];
process.on('warning', (warning) => warnings.push(warning.message));
assert.deepEqual(Object.keys(process.env).sort(), ['APP_PORT', 'FROM_FILE', 'HOME', 'LC_ALL']);
assert.equal(process.env.DATABASE_URL, undefined);
assert.equal(process.env.DATABASE_URL, undefined);
assert.equal(process.env.NODE_ENV, undefined);
process.env.DATABASE_URL = 'set at runtime';
assert.equal(process.env.DATABASE_URL, 'set at runtime');
process.env.NEW_VARIABLE = 'new';
assert.equal(Deno.env.get('NEW_VARIABLE'), 'new');
await new Promise((resolve) => setTimeout(resolve, 10));
assert.deepEqual(warnings, [
    'The permission model removed the environment variable "DATABASE_URL" at startup. Use --allow-env to manage permissions.',
    'The permission model removed the environment variable "NODE_ENV" at startup. Use --allow-env to manage permissions.',
]);
"#,
    ));
}

#[test]
fn child_processes_are_denied_unless_allowed_and_inherit_the_instance() {
    let directory = project();
    let root = canonical(directory.path());
    completed(run(
        restricted(&root, project_access(&root)),
        "denied.mjs",
        &format!(
            r#"{DENIED}
import {{ spawn, spawnSync, execSync, execFile }} from 'node:child_process';
import childProcess from 'node:child_process';
denied(() => spawnSync('/bin/echo'), 'ChildProcess', '/bin/echo');
denied(() => spawn('ls', ['-l']), 'ChildProcess', 'ls');
denied(() => execSync('echo hi'), 'ChildProcess', '/bin/sh');
denied(() => execFile('/bin/ls'), 'ChildProcess', '/bin/ls');
denied(() => childProcess.spawnSync('/bin/echo'), 'ChildProcess', '/bin/echo');
denied(() => new Deno.Command('/bin/echo').outputSync(), 'ChildProcess', '/bin/echo');
"#
        ),
    ));
    let mut settings = restricted(
        &root,
        PermissionSet { child_process: true, env: names(&["APP_*"]), ..project_access(&root) },
    );
    settings.env.insert("APP_NAME".into(), "guest".into());
    settings.env.insert("DATABASE_URL".into(), "secret".into());
    completed(run(
        settings,
        "allowed.mjs",
        &format!(
            r#"
import assert from 'node:assert/strict';
import {{ spawnSync }} from 'node:child_process';
const script = 'printf "%s|%s|%s" "$APP_NAME" "${{DATABASE_URL-removed}}" "$PWD"; test -n "$BENTO_PERMISSION_CEILING"';
const result = spawnSync('/bin/sh', ['-c', script], {{ encoding: 'utf8' }});
assert.equal(result.status, 0, result.stderr);
assert.equal(result.stdout, 'guest|removed|' + {root:?});
const output = new Deno.Command('/bin/sh', {{ args: ['-c', 'printf %s "$APP_NAME"'] }}).outputSync();
assert.equal(new TextDecoder().decode(output.stdout), 'guest');
const ceiling = JSON.parse(spawnSync('/bin/sh', ['-c', 'printf %s "$BENTO_PERMISSION_CEILING"'], {{ encoding: 'utf8' }}).stdout);
assert.equal(ceiling.restricted, true);
"#,
            root = root.display().to_string(),
        ),
    ));
}

#[test]
fn an_unrestricted_instance_hands_no_ceiling_to_its_processes() {
    let directory = project();
    completed(run(
        config_with(directory.path(), Permissions::unrestricted()),
        "ceiling.mjs",
        r#"
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
assert.equal(spawnSync('/bin/sh', ['-c', 'printf %s "${BENTO_PERMISSION_CEILING-none}"'], { encoding: 'utf8' }).stdout, 'none');
"#,
    ));
}

#[test]
fn workers_addons_and_the_network_are_denied_with_nodes_errors() {
    let directory = project();
    let root = canonical(directory.path());
    completed(run(
        restricted(&root, project_access(&root)),
        "denials.mjs",
        &format!(
            r#"{DENIED}
import {{ Worker }} from 'node:worker_threads';
import net from 'node:net';
denied(() => new Worker('1', {{ eval: true }}), 'WorkerThreads', '');
denied(() => new globalThis.Worker(new URL('data:text/javascript,1')), 'WorkerThreads', '');
assert.throws(() => process.dlopen({{ exports: {{}} }}, '/tmp/not-an-addon.node'), {{
    code: 'ERR_DLOPEN_DISABLED',
    message: 'Cannot load native addon because loading addons is disabled.',
}});
denied(() => net.createServer().listen(0), 'Net', '');
const failure = await fetch('http://127.0.0.1:8080/').then(() => undefined, (error) => error);
assert.ok(failure instanceof TypeError, String(failure));
assert.equal(failure.message, 'fetch failed');
assert.equal(failure.cause.code, 'ERR_ACCESS_DENIED');
"#
        ),
    ));
}

#[test]
fn system_information_is_available_to_a_restricted_instance() {
    let directory = project();
    let root = canonical(directory.path());
    completed(run(
        restricted(&root, project_access(&root)),
        "system.mjs",
        r#"
import assert from 'node:assert/strict';
import os from 'node:os';
assert.equal(typeof os.homedir(), 'string');
assert.equal(typeof os.hostname(), 'string');
assert.equal(typeof os.userInfo().username, 'string');
assert.ok(os.cpus().length > 0);
assert.equal(typeof os.networkInterfaces(), 'object');
assert.ok(os.totalmem() > 0);
assert.equal(os.loadavg().length, 3);
assert.ok(os.uptime() > 0);
assert.equal(typeof os.release(), 'string');
assert.equal(typeof Deno.hostname(), 'string');
"#,
    ));
}

#[test]
fn an_unrestricted_embedding_that_denies_something_defines_process_permission() {
    let directory = project();
    let root = canonical(directory.path());
    std::fs::write(root.join("secret.txt"), "secret").unwrap();
    std::fs::write(root.join("public.txt"), "public").unwrap();
    let settings = config_with(
        &root,
        Permissions::unrestricted_denying(PermissionSet {
            fs_read: Scope::only([pattern("secret.txt", &root)]),
            ..PermissionSet::default()
        }),
    );
    completed(run(
        settings,
        "denying.mjs",
        &format!(
            r#"{DENIED}
import fs from 'node:fs';
assert.equal(fs.readFileSync('public.txt', 'utf8'), 'public');
denied(() => fs.readFileSync('secret.txt'), 'FileSystemRead', 'secret.txt');
assert.equal(process.permission.has('fs.read'), false);
assert.equal(process.permission.has('fs.read', 'public.txt'), true);
assert.equal(process.permission.has('fs.write'), true);
assert.equal(process.permission.has('child'), true);
"#
        ),
    ));
}

#[test]
fn an_uncaught_denial_is_a_permission_failure_at_its_source_location() {
    let directory = project();
    let root = canonical(directory.path());
    let outcome = run(
        restricted(&root, project_access(&root)),
        "uncaught.mjs",
        "import fs from 'node:fs';\nfs.readFileSync('/etc/hosts');\n",
    );
    let RunOutcome::Failed(error) = outcome else { panic!("{outcome:?}") };
    assert_eq!(error.kind, ErrorKind::Permission, "{}", error.message);
    let location = error.location().expect("the denial is placed in the source");
    assert_eq!(location.path, root.join("uncaught.mjs"));
    assert_eq!(location.line, 2);
}

#[test]
fn dropping_the_language_model_takes_it_from_the_guest() {
    let directory = project();
    let root = canonical(directory.path());
    completed(run(
        restricted(&root, PermissionSet { language_model: true, ..project_access(&root) }),
        "model.mjs",
        r#"
import assert from 'node:assert/strict';
assert.equal(process.permission.has('language-model'), true);
process.permission.drop('language-model');
assert.equal(process.permission.has('language-model'), false);
assert.equal(await LanguageModel.availability(), 'unavailable');
await assert.rejects(LanguageModel.create(), { name: 'NotAllowedError' });
"#,
    ));
}

#[test]
fn a_static_import_of_a_module_the_instance_may_not_read_fails_as_a_denial() {
    let directory = project();
    let root = canonical(directory.path());
    std::fs::write(root.join("secret.mjs"), "export const secret = 1;\n").unwrap();
    let outcome = run(
        restricted(
            &root,
            PermissionSet {
                fs_read: Scope::only([pattern("main.mjs", &root)]),
                ..PermissionSet::default()
            },
        ),
        "main.mjs",
        "import { secret } from './secret.mjs';\nconsole.log(secret);\n",
    );
    let RunOutcome::Failed(error) = outcome else { panic!("{outcome:?}") };
    assert_eq!(error.kind, ErrorKind::Permission, "{}", error.message);
    assert!(
        error.message.contains("Use --allow-fs-read to manage permissions."),
        "{}",
        error.message
    );
    assert!(
        error.message.contains(&root.join("secret.mjs").display().to_string()),
        "{}",
        error.message
    );
}
