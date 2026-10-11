//! `bento-guestbook-host serve` as a process: it serves the real guest book, `SIGTERM` ends it
//! gracefully with the port free at once, a second run serves the same entries, `entries` prints
//! them, and `--read-only` refuses a signature without changing the store.

mod support;

use std::fs;
use std::path::Path;
use std::process::{Child, Output, Stdio};
use std::time::{Duration, Instant};

use chrono::DateTime;
use serde_json::Value;

use bento_guestbook_host::launch::Mode;
use bento_guestbook_host::store::{Access, GuestBookStore, NewEntry};
use support::{
    assert_port_free, document, fixture, free_port, guest_project, guests, host_command, http,
    scratch_dir, stderr, stdout, stored_rows, wait_for_host,
};

const DEADLINE: Duration = Duration::from_secs(5);

/// Spawns `serve` on the fixture copy with piped output and waits until it listens.
fn spawn_serve(database: &str, port: u16, extra: &[&str]) -> Child {
    let project = fixture().to_str().unwrap();
    let port_argument = port.to_string();
    let mut arguments =
        vec!["serve", "--project", project, "--database", database, "--port", &port_argument];
    arguments.extend_from_slice(extra);
    spawn_host(fixture(), &arguments, port)
}

fn spawn_host(directory: &Path, arguments: &[&str], port: u16) -> Child {
    let mut command = host_command(directory, arguments);
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = command.spawn().unwrap();
    wait_for_host(&mut child, port);
    child
}

fn send_sigterm(child: &Child) {
    let pid = i32::try_from(child.id()).unwrap();
    // SAFETY: `kill` only sends a signal to the process this test spawned and still owns.
    assert_eq!(unsafe { libc::kill(pid, libc::SIGTERM) }, 0);
}

/// Sends `SIGTERM` and expects the host to report a graceful shutdown with the guest's exit code
/// within the deadline.
fn terminate(child: Child) -> Output {
    let signaled = Instant::now();
    send_sigterm(&child);
    let output = child.wait_with_output().unwrap();
    assert!(signaled.elapsed() < DEADLINE, "{:?}", signaled.elapsed());
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert!(
        stdout(&output).contains("shutdown: graceful (exit code 0)"),
        "{}\n{}",
        stdout(&output),
        stderr(&output)
    );
    output
}

#[test]
fn entries_survive_a_host_restart_and_entries_prints_them() {
    let scratch = scratch_dir();
    let database = scratch.path().join("book.db");
    let database = database.to_str().unwrap();
    let port = free_port();

    let first = spawn_serve(database, port, &["--mode", "development"]);
    let page = http::get(port, "/");
    document::assert_page(&page, &[]);
    document::assert_assets_served(port, &page.body, Mode::Development);
    let visitor = ("Ada Lovelace", "Hello from 1843");
    let signed = http::post_form(port, "/", &[("name", visitor.0), ("message", visitor.1)]);
    assert_eq!(signed.status, 303, "{}", signed.body);
    terminate(first);
    assert_port_free(port);

    let second = spawn_serve(database, port, &["--mode", "production"]);
    let page = http::get(port, "/");
    document::assert_page(&page, &[visitor]);
    document::assert_assets_served(port, &page.body, Mode::Production);
    terminate(second);

    let listed =
        host_command(scratch.path(), &["entries", "--database", database]).output().unwrap();
    assert!(listed.status.success(), "{}", stderr(&listed));
    let lines: Vec<Value> =
        stdout(&listed).lines().map(|line| serde_json::from_str(line).unwrap()).collect();
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert_eq!(lines[0]["name"], visitor.0);
    assert_eq!(lines[0]["message"], visitor.1);
    DateTime::parse_from_rfc3339(lines[0]["createdAt"].as_str().unwrap()).unwrap();
}

#[test]
fn read_only_lists_entries_refuses_signatures_and_leaves_the_store_byte_identical() {
    let scratch = scratch_dir();
    let database = scratch.path().join("book.db");
    let store = GuestBookStore::open(&database, Access::ReadWrite).unwrap();
    let kept = ("Ada Lovelace", "Kept");
    let entry = NewEntry { name: kept.0.to_owned(), message: kept.1.to_owned() };
    tokio::runtime::Runtime::new().unwrap().block_on(store.append(entry)).unwrap();
    drop(store);
    let bytes_before = fs::read(&database).unwrap();
    let port = free_port();

    let serving = spawn_serve(database.to_str().unwrap(), port, &["--read-only"]);
    document::assert_page(&http::get(port, "/"), &[kept]);
    let refused = http::post_form(port, "/", &[("name", "Mallory"), ("message", "Dropped")]);
    assert_ne!(refused.status, 303, "{}", refused.body);
    assert_eq!(refused.header("location"), None);
    assert!(refused.status >= 500, "{} {}", refused.status, refused.body);
    let page = http::get(port, "/");
    document::assert_page(&page, &[kept]);
    assert!(!page.body.contains("Dropped"), "{}", page.body);
    let output = terminate(serving);

    assert_eq!(fs::read(&database).unwrap(), bytes_before);
    assert_eq!(stored_rows(&database).len(), 1);
    assert!(stderr(&output).contains("PermissionDenied"), "{}", stderr(&output));
}

/// A guest that never installs a signal handler is ended by the runtime's default shutdown: the
/// host reports that it closed the instance on the signal and exits as a process ended by
/// `SIGTERM` would, with 143, since there is no guest exit code to pass on.
#[test]
fn a_guest_without_a_signal_handler_is_closed_by_the_host_and_reported_by_signal() {
    let scratch = scratch_dir();
    guest_project(scratch.path(), "server.embedded.ts", guests::STARTING);
    let port = free_port();
    let port_argument = port.to_string();
    let arguments =
        ["serve", "--project", "project", "--database", "book.db", "--port", &port_argument];
    let child = spawn_host(scratch.path(), &arguments, port);

    let signaled = Instant::now();
    send_sigterm(&child);
    let output = child.wait_with_output().unwrap();
    assert!(signaled.elapsed() < DEADLINE, "{:?}", signaled.elapsed());
    assert_eq!(output.status.code(), Some(143), "{}", stderr(&output));
    let printed = stdout(&output);
    assert!(printed.contains("shutdown: graceful (closed by the host on SIGTERM)"), "{printed}");
}
