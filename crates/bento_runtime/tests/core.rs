mod support;

use std::collections::BTreeSet;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::time::Duration;

use tempfile::tempdir;

use bento_host::{NetTarget, Scope};
use bento_runtime::{ErrorKind, RunOutcome};
use support::{completed, config, run};

#[test]
fn esm_top_level_await_and_node_globals() {
    let root = tempdir().unwrap();
    let mut settings = config(root.path());
    settings.argv = vec!["argument".into()];
    completed(run(
        settings,
        "entry.mjs",
        r#"
import assert from 'node:assert/strict';
import path from 'node:path';
import { writeFile } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { AsyncLocalStorage } from 'node:async_hooks';
assert.equal(process.cwd(), import.meta.dirname);
assert.equal(import.meta.filename, path.join(process.cwd(), 'entry.mjs'));
assert.equal(process.argv.at(-1), 'argument');
assert.equal(Buffer.from('hello').toString('base64'), 'aGVsbG8=');
assert.equal(createRequire(import.meta.url)('node:path').sep, path.sep);
assert.deepEqual(structuredClone({ x: [1] }), { x: [1] });
const context = new AsyncLocalStorage();
await context.run('context', async () => { await Promise.resolve(); assert.equal(context.getStore(), 'context'); });
await new Promise(resolve => queueMicrotask(resolve));
const controller = new AbortController(); controller.abort(); assert.equal(controller.signal.aborted, true);
const form = new FormData(); form.set('name', 'Bento'); assert.equal(form.get('name'), 'Bento');
assert.equal(new Request('http://localhost/').method, 'GET');
assert.equal(await new Response(new ReadableStream({ start(c) { c.enqueue(new TextEncoder().encode('stream')); c.close(); } })).text(), 'stream');
assert.equal(typeof fetch, 'function');
await writeFile(path.join(process.cwd(), 'effect.txt'), 'completed');
"#,
    ));
    assert_eq!(std::fs::read_to_string(root.path().join("effect.txt")).unwrap(), "completed");
}

#[test]
fn filesystem_grants_canonicalize_symlink_targets() {
    let root = tempdir().unwrap();
    let outside = tempdir().unwrap();
    std::fs::write(outside.path().join("secret"), "secret").unwrap();
    std::os::unix::fs::symlink(outside.path(), root.path().join("escape")).unwrap();
    completed(run(
        config(root.path()),
        "fs.mjs",
        r#"
import assert from 'node:assert/strict';
import fs from 'node:fs';
import * as promises from 'node:fs/promises';
fs.writeFileSync('allowed', 'yes'); assert.equal(fs.readFileSync('allowed', 'utf8'), 'yes');
assert.equal(fs.statSync('allowed').isFile(), true); assert.ok(fs.readdirSync('.').includes('allowed'));
await promises.writeFile('async', 'yes'); assert.equal(await promises.readFile('async', 'utf8'), 'yes');
assert.equal((await promises.stat('async')).isFile(), true); assert.ok((await promises.readdir('.')).includes('async'));
assert.throws(() => fs.readFileSync('escape/secret'), /permission|NotCapable|Requires/i);
await assert.rejects(promises.readFile('escape/secret'), /permission|NotCapable|Requires/i);
assert.throws(() => fs.writeFileSync('escape/written', 'no'), /permission|NotCapable|Requires/i);
"#,
    ));
    assert!(!outside.path().join("written").exists());
}

#[test]
fn sqlite_persists_across_instances() {
    let root = tempdir().unwrap();
    completed(run(
        config(root.path()),
        "insert.mjs",
        r#"
import { DatabaseSync } from 'node:sqlite';
const db = new DatabaseSync('book.db');
db.exec('CREATE TABLE entries (name TEXT NOT NULL)');
db.prepare('INSERT INTO entries VALUES (?)').run('Bento'); db.close();
"#,
    ));
    completed(run(
        config(root.path()),
        "select.mjs",
        r#"
import assert from 'node:assert/strict';
import { DatabaseSync } from 'node:sqlite';
const db = new DatabaseSync('book.db');
assert.equal(db.prepare('SELECT name FROM entries WHERE name = ?').get('Bento').name, 'Bento'); db.close();
"#,
    ));
}

#[test]
fn environment_and_subprocess_authority_are_instance_local() {
    let root = tempdir().unwrap();
    let key = "BENTO_RUNTIME_ISOLATION_TEST";
    let before = std::env::var_os(key);
    let mut settings = config(root.path());
    settings.env.insert(key.into(), "guest".into());
    settings.env.insert("BENTO_HIDDEN".into(), "secret".into());
    settings.grants.env = Scope::Only(BTreeSet::from([key.into()]));
    completed(run(
        settings,
        "env.mjs",
        r#"
import assert from 'node:assert/strict';
import { spawn, execSync } from 'node:child_process';
assert.equal(process.env.BENTO_HIDDEN, undefined);
assert.ok(!Object.keys(process.env).includes('BENTO_HIDDEN'));
assert.equal(process.env.BENTO_RUNTIME_ISOLATION_TEST, 'guest');
process.env.BENTO_RUNTIME_ISOLATION_TEST = 'changed';
assert.equal(process.env.BENTO_RUNTIME_ISOLATION_TEST, 'changed');
assert.throws(() => { process.env.BENTO_HIDDEN = 'no'; }, /permission|NotCapable|Requires/i);
assert.throws(() => spawn('/usr/bin/true'), /NotCapable|subprocess/i);
assert.throws(() => execSync('/usr/bin/true'), /NotCapable|subprocess/i);
"#,
    ));
    assert_eq!(std::env::var_os(key), before);
}

#[test]
fn every_grant_still_exposes_only_the_user_and_group_ids_of_the_system() {
    let root = tempdir().unwrap();
    let mut settings = config(root.path());
    settings.grants.read = Scope::All;
    settings.grants.write = Scope::All;
    settings.grants.net = Scope::All;
    settings.grants.env = Scope::All;
    settings.grants.addons = Scope::All;
    completed(run(
        settings,
        "system.mjs",
        r#"
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
assert.ok(Number.isInteger(process.getuid()) && Number.isInteger(process.getgid()));
fs.accessSync(process.cwd(), fs.constants.R_OK);
for (const query of [os.hostname, os.userInfo, os.homedir, os.networkInterfaces, os.cpus,
    os.totalmem, os.freemem, os.loadavg, os.uptime, os.release]) {
    assert.throws(() => query(), /NotCapable/, query.name);
}
"#,
    ));
}

#[test]
fn event_target_accepts_node_style_signals() {
    let root = tempdir().unwrap();
    completed(run(
        config(root.path()),
        "signals.mjs",
        r#"
import assert from 'node:assert/strict';
const target = new EventTarget(); let calls = 0; let abort;
const signal = { aborted: false, addEventListener(type, fn) { assert.equal(type, 'abort'); abort = fn; } };
target.addEventListener('test', () => calls++, { signal });
target.dispatchEvent(new Event('test')); assert.equal(calls, 1);
abort(); target.dispatchEvent(new Event('test')); assert.equal(calls, 1);
target.addEventListener('test', () => calls++, { signal: { aborted: true } });
target.dispatchEvent(new Event('test')); assert.equal(calls, 1);
const controller = new AbortController();
target.addEventListener('test', () => calls++, { signal: controller.signal });
target.dispatchEvent(new Event('test')); assert.equal(calls, 2);
controller.abort(); target.dispatchEvent(new Event('test')); assert.equal(calls, 2);
"#,
    ));
}

#[test]
fn http_streams_and_omitted_host_is_dual_stack() {
    let root = tempdir().unwrap();
    let probe = TcpListener::bind("[::1]:0").unwrap();
    let port = probe.local_addr().unwrap().port();
    drop(probe);
    let mut settings = config(root.path());
    settings.grants.net = Scope::Only(BTreeSet::from([NetTarget::new("::", Some(port))]));
    let entry = root.path().join("server.mjs");
    std::fs::write(&entry, format!(r#"
import http from 'node:http';
import {{ writeFileSync }} from 'node:fs';
let requests = 0;
const server = http.createServer((req, res) => {{
  res.write('stream'); setTimeout(() => {{ res.end('ed'); if (++requests === 2) {{ server.close(); server.closeAllConnections(); }} }}, 10);
}});
server.listen({port}, () => writeFileSync('ready', 'yes'));
"#)).unwrap();
    let runtime = bento_runtime::start(settings, &entry).unwrap();
    for _ in 0..500 {
        if root.path().join("ready").exists() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(root.path().join("ready").exists(), "server did not listen");
    for host in ["127.0.0.1", "::1"] {
        let address: SocketAddr = format!(
            "{}:{port}",
            if host.contains(':') { format!("[{host}]") } else { host.into() }
        )
        .parse()
        .unwrap();
        let mut stream = TcpStream::connect_timeout(&address, Duration::from_secs(2)).unwrap();
        stream.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        stream
            .write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
            .unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        assert!(response.starts_with("HTTP/1.1 200"), "{response}");
        assert!(response.contains("6\r\nstream\r\n2\r\ned\r\n0\r\n\r\n"), "{response}");
    }
    completed(runtime.wait());
}

#[test]
fn errors_are_classified_and_located() {
    let root = tempdir().unwrap();
    for (name, source, kind) in [
        ("syntax.mjs", "const = ;", ErrorKind::Startup),
        ("missing.mjs", "import 'bento-missing-package';", ErrorKind::Resolution),
        (
            "denied.mjs",
            "import fs from 'node:fs'; fs.readFileSync('/etc/passwd');",
            ErrorKind::Permission,
        ),
        (
            "subprocess.mjs",
            "import { execSync } from 'node:child_process'; execSync('/usr/bin/true');",
            ErrorKind::Permission,
        ),
        ("throw.mjs", "throw new Error('guest exception');", ErrorKind::Uncaught),
        ("reject.mjs", "Promise.reject(new Error('guest rejection'));", ErrorKind::Uncaught),
        ("remote.mjs", "import 'https://example.invalid/module.js';", ErrorKind::Resolution),
    ] {
        let RunOutcome::Failed(error) = run(config(root.path()), name, source) else {
            panic!("{name} succeeded");
        };
        assert_eq!(error.kind, kind, "{error:?}");
        assert!(error.message.contains(name), "{error:?}");
        if name == "missing.mjs" {
            assert!(
                error.message.contains("bento-missing-package")
                    && error.message.contains("bento install"),
                "{error:?}"
            );
        }
    }
}

#[test]
fn concurrent_instances_do_not_share_permissions() {
    let first = tempdir().unwrap();
    let second = tempdir().unwrap();
    std::fs::write(first.path().join("private"), "first").unwrap();
    let source = format!(
        r#"
import assert from 'node:assert/strict'; import fs from 'node:fs';
await new Promise(resolve => setTimeout(resolve, 30));
assert.throws(() => fs.readFileSync({:?}), /permission|NotCapable|Requires/i);
"#,
        first.path().join("private").to_str().unwrap()
    );
    let first_entry = first.path().join("entry.mjs");
    let second_entry = second.path().join("entry.mjs");
    std::fs::write(&first_entry, "import fs from 'node:fs/promises'; await new Promise(r => setTimeout(r, 60)); await fs.readFile('private');").unwrap();
    std::fs::write(&second_entry, source).unwrap();
    let first_runtime = bento_runtime::start(config(first.path()), &first_entry).unwrap();
    let second_runtime = bento_runtime::start(config(second.path()), &second_entry).unwrap();
    completed(first_runtime.wait());
    completed(second_runtime.wait());
}

#[test]
fn process_exit_does_not_terminate_the_embedding_process() {
    let root = tempdir().unwrap();
    let exited = run(
        config(root.path()),
        "exit.mjs",
        "process.exit(7); throw new Error('execution continued after exit');",
    );
    assert!(matches!(exited, RunOutcome::Exited(7)), "{exited:?}");
    completed(run(
        config(root.path()),
        "after-exit.mjs",
        "import fs from 'node:fs'; fs.writeFileSync('after-exit', 'alive');",
    ));
    assert_eq!(std::fs::read_to_string(root.path().join("after-exit")).unwrap(), "alive");
}
