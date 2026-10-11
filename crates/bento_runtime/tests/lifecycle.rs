mod support;

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::time::{Duration, Instant};

use tempfile::{TempDir, tempdir};

use bento_host::Scope;
use bento_runtime::{RunOutcome, RunningRuntime, ShutdownOutcome, Signal, start};
use support::{completed, config, run};

fn port() -> u16 {
    TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

fn launch(root: &Path, source: &str) -> RunningRuntime {
    let mut settings = config(root);
    settings.grants.net = Scope::All;
    settings.shutdown_deadline = Duration::from_millis(200);
    let entry = root.join("entry.mjs");
    std::fs::write(&entry, source).unwrap();
    start(settings, &entry).unwrap()
}

fn ready(root: &Path) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !root.join("ready").exists() {
        assert!(Instant::now() < deadline, "guest did not become ready");
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn server_source(port: u16, handler: &str, signals: &str) -> String {
    format!(
        r#"
import http from 'node:http';
import {{ writeFileSync }} from 'node:fs';
const server = http.createServer((req, res) => {{ {handler} }});
{signals}
server.listen({port}, '127.0.0.1', () => writeFileSync('ready', 'yes'));
"#
    )
}

fn bind_after(port: u16) {
    let listener =
        TcpListener::bind(("127.0.0.1", port)).expect("disposed listener still owns port");
    drop(listener);
}

fn request(port: u16) -> TcpStream {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
    stream.write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n").unwrap();
    stream
}

#[test]
fn process_exit_is_contained_at_top_level_and_in_http_handlers() {
    let root = tempdir().unwrap();
    assert!(matches!(
        run(config(root.path()), "exit.mjs", "process.exit(7);"),
        RunOutcome::Exited(7)
    ));
    let port = port();
    let runtime = launch(root.path(), &server_source(port, "process.exit();", ""));
    ready(root.path());
    let mut connection = request(port);
    let mut response = Vec::new();
    let _ = connection.read_to_end(&mut response);
    assert!(matches!(runtime.wait(), RunOutcome::Exited(0)));
    bind_after(port);
}

#[test]
fn virtual_signals_deliver_handlers_and_default_to_shutdown() {
    for signal in [Signal::Terminate, Signal::Interrupt] {
        let root = tempdir().unwrap();
        let port = port();
        let name = if signal == Signal::Terminate { "SIGTERM" } else { "SIGINT" };
        let runtime = launch(
            root.path(),
            &server_source(
                port,
                "res.end('ok');",
                &format!(
                    "process.on('{name}', () => {{ writeFileSync('signalled', '{name}'); server.close(); server.closeAllConnections(); }});"
                ),
            ),
        );
        ready(root.path());
        runtime.handle().signal(signal);
        completed(runtime.wait());
        assert_eq!(std::fs::read_to_string(root.path().join("signalled")).unwrap(), name);
        bind_after(port);
    }
    let root = tempdir().unwrap();
    let port = port();
    let runtime = launch(root.path(), &server_source(port, "res.end('ok');", ""));
    ready(root.path());
    runtime.handle().signal(Signal::Terminate);
    assert!(matches!(runtime.wait(), RunOutcome::Shutdown(_)));
    bind_after(port);
}

#[test]
fn registering_guest_signals_does_not_change_os_signal_disposition() {
    fn disposition() -> libc::sigaction {
        let mut action = std::mem::MaybeUninit::uninit();
        assert_eq!(
            unsafe { libc::sigaction(libc::SIGUSR2, std::ptr::null(), action.as_mut_ptr()) },
            0
        );
        unsafe { action.assume_init() }
    }
    let before = disposition();
    let root = tempdir().unwrap();
    let runtime = launch(
        root.path(),
        "import { writeFileSync } from 'node:fs'; process.on('SIGUSR2', () => {}); writeFileSync('ready', 'yes'); setInterval(() => {}, 1000);",
    );
    ready(root.path());
    let after = disposition();
    assert_eq!(after.sa_sigaction, before.sa_sigaction);
    assert_eq!(after.sa_flags, before.sa_flags);
    runtime.handle().shutdown();
    assert!(matches!(runtime.wait(), RunOutcome::Shutdown(_)));
}

#[test]
fn fixture_shaped_shutdown_drains_and_releases_the_listener() {
    let root = tempdir().unwrap();
    let port = port();
    let runtime = launch(
        root.path(),
        &server_source(
            port,
            "writeFileSync('connected', 'yes');",
            "process.on('SIGTERM', () => { server.close(() => process.exit(0)); server.closeAllConnections(); });",
        ),
    );
    ready(root.path());
    let mut connection = TcpStream::connect(("127.0.0.1", port)).unwrap();
    connection.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
    connection
        .write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: keep-alive\r\n\r\n")
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !root.path().join("connected").exists() {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }
    let start = Instant::now();
    let report = runtime.handle().shutdown();
    assert_eq!(report.outcome, ShutdownOutcome::Graceful { exit_code: 0 });
    assert!(start.elapsed() < Duration::from_secs(1));
    let mut response = Vec::new();
    match connection.read_to_end(&mut response) {
        Ok(0) => {}
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::ConnectionAborted
            ) => {}
        outcome => panic!("handler did not drain the open connection: {outcome:?}"),
    }
    bind_after(port);
    assert!(matches!(runtime.wait(), RunOutcome::Shutdown(_)));
}

#[test]
fn shutdown_without_handler_closes_pending_connections() {
    let root = tempdir().unwrap();
    let port = port();
    let runtime = launch(root.path(), &server_source(port, "", ""));
    ready(root.path());
    let mut connection = request(port);
    let start = Instant::now();
    assert_eq!(runtime.handle().shutdown().outcome, ShutdownOutcome::Graceful { exit_code: 0 });
    assert!(start.elapsed() < Duration::from_secs(1));
    let mut response = Vec::new();
    match connection.read_to_end(&mut response) {
        Ok(0) => {}
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::ConnectionAborted
            ) => {}
        outcome => panic!("connection was not closed: {outcome:?}"),
    }
    bind_after(port);
    assert!(matches!(runtime.wait(), RunOutcome::Shutdown(_)));
}

fn prove_interruption(root: &TempDir, runtime: RunningRuntime, port: Option<u16>) {
    let start = Instant::now();
    assert_eq!(runtime.handle().shutdown().outcome, ShutdownOutcome::Interrupted);
    assert!(start.elapsed() < Duration::from_secs(1));
    assert!(matches!(runtime.wait(), RunOutcome::Shutdown(_)));
    if let Some(port) = port {
        bind_after(port);
    }
    let next_port = port.unwrap_or_else(crate::port);
    std::fs::remove_file(root.path().join("ready")).unwrap();
    let replacement = launch(
        root.path(),
        &server_source(next_port, "res.end('replacement'); server.close();", ""),
    );
    ready(root.path());
    let mut response = String::new();
    request(next_port).read_to_string(&mut response).unwrap();
    assert!(response.contains("replacement"));
    completed(replacement.wait());
}

#[test]
fn spinning_top_level_javascript_is_interrupted_and_disposed() {
    let root = tempdir().unwrap();
    let runtime = launch(
        root.path(),
        "import { writeFileSync } from 'node:fs'; writeFileSync('ready', 'yes'); while (true) {}",
    );
    ready(root.path());
    prove_interruption(&root, runtime, None);
}

#[test]
fn spinning_request_handler_is_interrupted_and_disposed() {
    let root = tempdir().unwrap();
    let port = port();
    let runtime = launch(
        root.path(),
        &server_source(port, "writeFileSync('spinning', 'yes'); while (true) {}", ""),
    );
    ready(root.path());
    let connection = request(port);
    let deadline = Instant::now() + Duration::from_secs(5);
    while !root.path().join("spinning").exists() {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }
    prove_interruption(&root, runtime, Some(port));
    drop(connection);
}

#[test]
fn repeated_shutdown_and_drop_release_the_same_port() {
    let port = port();
    for iteration in 0..20 {
        let root = tempdir().unwrap();
        let runtime = launch(root.path(), &server_source(port, "res.end('ok');", ""));
        ready(root.path());
        if iteration % 2 == 0 {
            runtime.handle().shutdown();
            assert!(matches!(runtime.wait(), RunOutcome::Shutdown(_)));
        } else {
            drop(runtime);
        }
        bind_after(port);
    }
}

#[test]
fn loaded_modules_includes_dynamic_local_imports_not_packages_or_builtins() {
    let root = tempdir().unwrap();
    std::fs::write(root.path().join("dynamic.mjs"), "export const answer = 42;").unwrap();
    let package = root.path().join("node_modules/demo");
    std::fs::create_dir_all(&package).unwrap();
    std::fs::write(
        package.join("package.json"),
        r#"{"name":"demo","type":"module","exports":"./index.mjs"}"#,
    )
    .unwrap();
    std::fs::write(package.join("index.mjs"), "export const answer = 1;").unwrap();
    let port = port();
    let runtime = launch(
        root.path(),
        &server_source(
            port,
            "awaitImport(res);",
            r#"
import 'demo';
async function awaitImport(res) { const module = await import('./dynamic.mjs'); res.end(String(module.answer)); }
"#,
        ),
    );
    ready(root.path());
    assert_eq!(
        runtime.handle().loaded_modules(),
        vec![root.path().canonicalize().unwrap().join("entry.mjs")]
    );
    let mut response = String::new();
    request(port).read_to_string(&mut response).unwrap();
    assert!(response.contains("42"));
    let loaded = runtime.handle().loaded_modules();
    assert_eq!(loaded.len(), 2, "{loaded:?}");
    assert!(loaded.contains(&root.path().canonicalize().unwrap().join("dynamic.mjs")));
    runtime.handle().shutdown();
    assert!(matches!(runtime.wait(), RunOutcome::Shutdown(_)));
}
