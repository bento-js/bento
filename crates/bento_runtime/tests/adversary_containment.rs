mod support;

use std::process::Command;

use support::{completed, config, project, run};

fn isolated(name: &str) -> bool {
    if std::env::var("BENTO_CONTAINMENT_TEST").as_deref() == Ok(name) {
        return false;
    }
    let child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", name, "--nocapture"])
        .env("BENTO_CONTAINMENT_TEST", name)
        .env_remove("NODE_PATH")
        .output()
        .unwrap();
    assert!(
        child.status.success(),
        "host exited {:?}: {}\n{}",
        child.status.code(),
        String::from_utf8_lossy(&child.stdout),
        String::from_utf8_lossy(&child.stderr)
    );
    assert!(String::from_utf8_lossy(&child.stdout).contains("1 passed"));
    true
}

#[test]
fn unsupported_worker_features_fail_the_instance_not_the_host() {
    let name = "unsupported_worker_features_fail_the_instance_not_the_host";
    if isolated(name) {
        return;
    }
    let root = project();
    std::fs::write(root.path().join("worker.mjs"), "postMessage('alive');").unwrap();
    let outcome = run(
        config(root.path()),
        "entry.mjs",
        "new Worker(new URL('./worker.mjs', import.meta.url), { type: 'module', deno: { permissions: { env: [] } } });",
    );
    assert!(matches!(outcome, bento_runtime::RunOutcome::Failed(_)), "{outcome:?}");
    completed(run(config(root.path()), "replacement.mjs", "if (1 + 1 !== 2) throw Error('bad');"));
}

#[test]
fn unsupported_features_in_a_worker_fail_only_its_instance() {
    let name = "unsupported_features_in_a_worker_fail_only_its_instance";
    if isolated(name) {
        return;
    }
    let root = project();
    std::fs::write(root.path().join("child.mjs"), "postMessage('alive');").unwrap();
    std::fs::write(root.path().join("worker.mjs"),
        "new Worker(new URL('./child.mjs', import.meta.url), { type: 'module', deno: { permissions: { env: [] } } });").unwrap();
    let outcome = run(
        config(root.path()),
        "entry.mjs",
        "new Worker(new URL('./worker.mjs', import.meta.url), { type: 'module' }); setInterval(() => {}, 100);",
    );
    assert!(matches!(outcome, bento_runtime::RunOutcome::Failed(_)), "{outcome:?}");
    completed(run(config(root.path()), "replacement.mjs", "await Promise.resolve();"));
}

#[test]
fn process_abort_and_self_kill_cannot_terminate_the_embedding_process() {
    let name = "process_abort_and_self_kill_cannot_terminate_the_embedding_process";
    if isolated(name) {
        return;
    }
    let root = project();
    completed(run(
        config(root.path()),
        "entry.mjs",
        r#"
for (const action of [() => process.abort(), () => process.kill(process.pid, 'SIGKILL'), () => Deno.kill(process.pid, 'SIGKILL')]) {
  let denied = false;
  try { action(); } catch (error) { denied = error.name === 'NotSupported' || error.name === 'NotCapable'; }
  if (!denied) throw Error('host process mutation was allowed');
}
"#,
    ));
}

#[test]
fn permission_mutations_are_rejected_and_queries_keep_host_grants() {
    let root = project();
    completed(run(
        config(root.path()),
        "entry.mjs",
        r#"
for (const name of ['read', 'env']) {
  const descriptor = name === 'read' ? { name, path: '.' } : { name, variable: 'SECRET' };
  const before = Deno.permissions.querySync(descriptor).state;
  for (const method of ['requestSync', 'revokeSync', 'request', 'revoke']) {
    let denied = false;
    try { await Deno.permissions[method](descriptor); }
    catch (error) { denied = error.name === 'NotSupported' && error.message.includes('fixed by the host'); }
    if (!denied) throw Error(`${method} did not explain fixed host grants`);
    if ((await Deno.permissions.query(descriptor)).state !== before) throw Error('host grant changed');
  }
}
"#,
    ));
}

#[test]
fn rejected_worker_options_throw_before_starting_a_worker() {
    let root = project();
    std::fs::write(
        root.path().join("worker.mjs"),
        "import { writeFileSync } from 'node:fs'; writeFileSync('ran', 'side effect');",
    )
    .unwrap();
    completed(run(
        config(root.path()),
        "entry.mjs",
        r#"
for (const permissions of ['inherit', { env: [] }]) {
  let rejected = false;
  try { new Worker(new URL('./worker.mjs', import.meta.url), { type: 'module', deno: { permissions } }); }
  catch (error) { rejected = error.name === 'NotSupported'; }
  if (!rejected) throw Error('unsupported worker options did not throw');
}
await new Promise(resolve => setTimeout(resolve, 50));
"#,
    ));
    assert!(!root.path().join("ran").exists(), "unsupported worker executed");
}

#[test]
fn late_unsupported_feature_is_an_uncaught_error_with_its_source_location() {
    let root = project();
    std::fs::write(root.path().join("worker.mjs"), "postMessage('alive');").unwrap();
    let outcome = run(
        config(root.path()),
        "late.mjs",
        "await new Promise(resolve => setTimeout(resolve, 10));\nnew Worker(new URL('./worker.mjs', import.meta.url), { type: 'module', deno: { permissions: 'inherit' } });",
    );
    let bento_runtime::RunOutcome::Failed(error) = outcome else { panic!("{outcome:?}") };
    assert_eq!(error.kind, bento_runtime::ErrorKind::Uncaught);
    let location = error.location().expect("unsupported feature has a guest source location");
    assert_eq!(location.path, root.path().canonicalize().unwrap().join("late.mjs"));
    assert_eq!(location.line, 2);
}

#[test]
fn reuse_port_is_rejected_before_binding_a_listener() {
    let root = project();
    // Keeping the port bound proves rejection precedes binding: a bind attempt would be AddrInUse.
    let occupied = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = occupied.local_addr().unwrap().port();
    let mut settings = config(root.path());
    settings.grants.net = bento_host::Scope::All;
    completed(run(
        settings,
        "entry.mjs",
        &format!(
            "let name = 'none'; try {{ Deno.listen({{hostname:'127.0.0.1',port:{port},reusePort:true}}); }} catch(error) {{ name = error.name; }} if (name !== 'NotSupported') throw Error(`reusePort failed with ${{name}}`); const listener = Deno.listen({{hostname:'127.0.0.1',port:0}}); listener.close();"
        ),
    ));
    drop(occupied);
}

#[test]
fn all_exposed_upstream_feature_gates_leave_the_embedding_process_alive() {
    let name = "all_exposed_upstream_feature_gates_leave_the_embedding_process_alive";
    if isolated(name) {
        return;
    }
    let root = project();
    let certificates = Command::new("openssl")
        .args([
            "req",
            "-x509",
            "-newkey",
            "ec",
            "-pkeyopt",
            "ec_paramgen_curve:prime256v1",
            "-nodes",
            "-days",
            "1",
            "-subj",
            "/CN=localhost",
            "-keyout",
        ])
        .arg(root.path().join("key.pem"))
        .arg("-out")
        .arg(root.path().join("cert.pem"))
        .output()
        .unwrap();
    assert!(certificates.status.success(), "{}", String::from_utf8_lossy(&certificates.stderr));
    let mut settings = config(root.path());
    settings.grants.net = bento_host::Scope::All;
    completed(run(
        settings,
        "features.mjs",
        r#"
import { readFileSync } from 'node:fs';
for (const name of ['openKv', 'cron', 'listenDatagram']) {
  if (Deno[name] !== undefined) throw Error(`${name} unexpectedly exposed`);
}
for (const operation of [
  () => Deno.listen({ transport: 'vsock', cid: 1, port: 1234 }),
  () => Deno.connect({ transport: 'vsock', cid: 1, port: 1234 }),
  () => Deno.listenTls({ hostname: '127.0.0.1', port: 0, reusePort: true,
    key: readFileSync('key.pem', 'utf8'), cert: readFileSync('cert.pem', 'utf8') }),
]) {
  let rejected = false;
  try { await operation(); } catch (error) { rejected = error.name === 'NotSupported'; }
  if (!rejected) throw Error('unsupported feature was not contained');
}
"#,
    ));
    completed(run(config(root.path()), "replacement.mjs", "await Promise.resolve();"));
}
