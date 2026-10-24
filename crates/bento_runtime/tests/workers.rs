mod support;

use std::collections::BTreeSet;

use parking_lot::Mutex;
use serde_json::Value;
use tempfile::tempdir;

use bento_host::Scope;
use bento_runtime::{RunOutcome, start};
use support::{completed, config, run};

static SIGNAL_CHECK: Mutex<()> = Mutex::new(());

fn disposition() -> libc::sigaction {
    let mut action = std::mem::MaybeUninit::uninit();
    // SAFETY: a null new action only reads the current one into `action`.
    assert_eq!(unsafe { libc::sigaction(libc::SIGUSR2, std::ptr::null(), action.as_mut_ptr()) }, 0);
    // SAFETY: sigaction returned 0, so it initialized `action`.
    unsafe { action.assume_init() }
}

fn worker_containment(node: bool, nested: bool) {
    let _serial = SIGNAL_CHECK.lock();
    let root = tempdir().unwrap();
    let key = "BENTO_WORKER_ENV_TEST";
    let host_before = std::env::var_os(key);
    let cwd_before = std::env::current_dir().unwrap();
    let signal_before = disposition();
    std::fs::write(
        root.path().join("worker.mjs"),
        format!(
            r#"
import {{ parentPort }} from 'node:worker_threads';
const initial = Deno.env.get('{key}');
Deno.env.set('{key}', 'worker-local');
let chdirDenied = false;
try {{ Deno.chdir('{root}'); }} catch {{ chdirDenied = true; }}
Deno.addSignalListener('SIGUSR2', () => {{}});
const answer = {{ initial, changed: Deno.env.get('{key}'), chdirDenied, cwd: Deno.cwd() }};
if (parentPort) parentPort.postMessage(answer); else postMessage(answer);
"#,
            root = root.path().display()
        ),
    )
    .unwrap();
    if nested {
        let wrapper = if node {
            r#"
import { Worker, parentPort } from 'node:worker_threads';
const child = new Worker(new URL('./worker.mjs', import.meta.url));
child.once('message', async answer => { await child.terminate(); parentPort.postMessage(answer); });
"#
        } else {
            r#"
const child = new Worker(new URL('./worker.mjs', import.meta.url), { type: 'module' });
child.onmessage = event => { child.terminate(); postMessage(event.data); };
"#
        };
        std::fs::write(root.path().join("outer.mjs"), wrapper).unwrap();
    }
    let mut settings = config(root.path());
    settings.grants.env = Scope::Only(BTreeSet::from([key.into()]));
    settings.env.insert(key.into(), "configured".into());
    let source = if node {
        r#"
import { Worker } from 'node:worker_threads';
const worker = new Worker(new URL('./worker.mjs', import.meta.url));
const answer = await new Promise((resolve, reject) => { worker.once('message', resolve); worker.once('error', reject); });
await import('node:fs').then(fs => fs.writeFileSync('answer.json', JSON.stringify(answer)));
await worker.terminate();
"#
    } else {
        r#"
const worker = new Worker(new URL('./worker.mjs', import.meta.url), { type: 'module' });
const answer = await new Promise((resolve, reject) => { worker.onmessage = event => resolve(event.data); worker.onerror = reject; });
await import('node:fs').then(fs => fs.writeFileSync('answer.json', JSON.stringify(answer)));
worker.terminate();
"#
    };
    let source = if nested { source.replace("./worker.mjs", "./outer.mjs") } else { source.into() };
    completed(run(settings, "entry.mjs", &source));
    assert_eq!(std::env::var_os(key), host_before, "worker changed host environment");
    assert_eq!(std::env::current_dir().unwrap(), cwd_before, "worker changed host cwd");
    let signal_after = disposition();
    assert_eq!(
        signal_after.sa_sigaction, signal_before.sa_sigaction,
        "worker installed an OS signal handler"
    );
    assert_eq!(signal_after.sa_flags, signal_before.sa_flags);
    let answer: Value =
        serde_json::from_str(&std::fs::read_to_string(root.path().join("answer.json")).unwrap())
            .unwrap();
    assert_eq!(answer["initial"], "configured");
    assert_eq!(answer["changed"], "worker-local");
    assert_eq!(answer["chdirDenied"], true);
    assert_eq!(answer["cwd"], root.path().canonicalize().unwrap().to_str().unwrap());
}

#[test]
fn web_workers_keep_environment_signals_and_cwd_inside_the_instance() {
    worker_containment(false, false);
    worker_containment(false, true);
}

#[test]
fn node_workers_keep_environment_signals_and_cwd_inside_the_instance() {
    worker_containment(true, false);
    worker_containment(true, true);
}

#[test]
fn shutdown_disposes_a_spinning_worker_and_releases_its_listener() {
    let root = tempdir().unwrap();
    std::fs::write(
        root.path().join("worker.mjs"),
        r#"
import { writeFileSync } from 'node:fs';
const listener = Deno.listen({ hostname: '127.0.0.1', port: 0 });
writeFileSync('port', String(listener.addr.port));
postMessage('ready');
while (true) {}
"#,
    )
    .unwrap();
    let entry = root.path().join("entry.mjs");
    std::fs::write(
        &entry,
        r#"
import { writeFileSync } from 'node:fs';
const worker = new Worker(new URL('./worker.mjs', import.meta.url), { type: 'module' });
await new Promise(resolve => { worker.onmessage = resolve; });
writeFileSync('ready', 'yes');
setInterval(() => {}, 1000);
"#,
    )
    .unwrap();
    let mut settings = config(root.path());
    settings.grants.net = Scope::All;
    settings.shutdown_deadline = std::time::Duration::from_millis(200);
    let runtime = start(settings, &entry).unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !root.path().join("ready").exists() {
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let port: u16 = std::fs::read_to_string(root.path().join("port")).unwrap().parse().unwrap();
    runtime.handle().shutdown();
    assert!(matches!(runtime.wait(), RunOutcome::Shutdown(_)));
    let listener = std::net::TcpListener::bind(("127.0.0.1", port))
        .expect("worker listener survived disposal");
    drop(listener);
}
