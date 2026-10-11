//! `host:guestbook` as a guest observes it: the round trip over Rust-owned storage, read-only
//! refusal, the hidden database file, instance isolation, and graceful shutdown with the port
//! released for a replacement.

mod support;

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};

use chrono::DateTime;
use rusqlite::Connection;

use bento_guestbook_host::launch::{Launch, Mode};
use bento_guestbook_host::store::{Access, GuestBookStore, NewEntry};
use bento_host::Scope;
use bento_runtime::{ErrorKind, RunOutcome, RuntimeConfig, ShutdownOutcome, start};
use support::{
    assert_port_free, free_port, guest_project, guests, http, scratch_dir, start_guest,
    wait_for_listener,
};

fn completed(outcome: RunOutcome) {
    assert!(matches!(outcome, RunOutcome::Completed), "{outcome:?}");
}

fn seeded_store(database: &Path) -> GuestBookStore {
    let store = GuestBookStore::open(database, Access::ReadWrite).unwrap();
    let executor = tokio::runtime::Runtime::new().unwrap();
    let entry = NewEntry { name: "Ada".to_owned(), message: "Kept".to_owned() };
    executor.block_on(store.append(entry)).unwrap();
    store
}

fn created_at_column(database: &Path) -> Vec<String> {
    let connection = Connection::open(database).unwrap();
    let mut statement = connection.prepare("SELECT createdAt FROM entries ORDER BY id").unwrap();
    let rows = statement.query_map([], |row| row.get(0)).unwrap();
    rows.map(Result::unwrap).collect()
}

#[test]
fn a_guest_appends_and_lists_entries_that_rust_persists() {
    let scratch = scratch_dir();
    let project = guest_project(scratch.path(), "round-trip.mjs", guests::ROUND_TRIP);
    let database = scratch.path().join("book.db");
    let port = free_port();
    let launch = Launch::prepare(&project, &database, port, Mode::Development).unwrap();
    let store = GuestBookStore::open(launch.database(), Access::ReadWrite).unwrap();

    let mut config = launch.runtime_config(store);
    config.argv = vec![port.to_string()];
    completed(start(config, &project.join("round-trip.mjs")).unwrap().wait());

    let timestamps = created_at_column(&database);
    assert_eq!(timestamps.len(), 2);
    for created_at in &timestamps {
        DateTime::parse_from_rfc3339(created_at).unwrap();
    }
}

#[test]
fn a_read_only_store_refuses_append_as_permission_denied_without_changing_the_file() {
    let scratch = scratch_dir();
    let project = guest_project(scratch.path(), "read-only.mjs", guests::READ_ONLY);
    let database = scratch.path().join("book.db");
    drop(seeded_store(&database));
    let bytes_before = fs::read(&database).unwrap();
    let serving = start_guest(&project, "read-only.mjs", &database, Access::ReadOnly, free_port());
    completed(serving.wait());

    assert_eq!(fs::read(&database).unwrap(), bytes_before);
    assert_eq!(created_at_column(&database).len(), 1);
}

#[test]
fn the_guest_cannot_read_the_database_file_whose_rows_list_returns() {
    let scratch = scratch_dir();
    let project = guest_project(scratch.path(), "probe.mjs", guests::PROBE_STORE_FILE);
    let database = scratch.path().join("book.db");
    let store = seeded_store(&database);

    let launch = Launch::prepare(&project, &database, free_port(), Mode::Development).unwrap();
    let mut config = launch.runtime_config(store);
    config.argv = vec![database.to_str().unwrap().to_owned()];
    completed(start(config, &project.join("probe.mjs")).unwrap().wait());
}

#[test]
fn only_the_registering_instance_can_import_host_guestbook() {
    let scratch = scratch_dir();
    let project = guest_project(scratch.path(), "server.mjs", guests::SERVER);
    fs::write(project.join("unregistered.mjs"), guests::UNREGISTERED).unwrap();
    let database = scratch.path().join("book.db");
    let port = free_port();
    let serving = start_guest(&project, "server.mjs", &database, Access::ReadWrite, port);
    wait_for_listener(port);
    assert_eq!(http::get(port, "/").body, "[]");

    let mut unregistered = RuntimeConfig::new(&project);
    unregistered.grants.read = Scope::Only(BTreeSet::from([project.canonicalize().unwrap()]));
    let RunOutcome::Failed(error) =
        start(unregistered, &project.join("unregistered.mjs")).unwrap().wait()
    else {
        panic!("an instance without the registration imported host:guestbook");
    };
    assert_eq!(error.kind, ErrorKind::Resolution, "{error:?}");
    assert!(error.message.contains("host:guestbook"), "{}", error.message);

    let signed = http::post_form(port, "/", &[("name", "Ada"), ("message", "Still here")]);
    assert_eq!(signed.status, 303);
    assert!(http::get(port, "/").body.contains("Still here"));
    assert_eq!(
        serving.handle().shutdown().outcome,
        ShutdownOutcome::Graceful { exit_code: Some(0) }
    );
    assert!(matches!(serving.wait(), RunOutcome::Shutdown(_)));
}

#[test]
fn graceful_shutdown_frees_the_port_and_a_replacement_serves_the_same_entries() {
    let scratch = scratch_dir();
    let project = guest_project(scratch.path(), "server.mjs", guests::SERVER);
    let database = scratch.path().join("book.db");
    let port = free_port();

    let first = start_guest(&project, "server.mjs", &database, Access::ReadWrite, port);
    wait_for_listener(port);
    let signed = http::post_form(port, "/", &[("name", "Ada"), ("message", "Hello from 1843")]);
    assert_eq!(signed.status, 303);
    assert_eq!(signed.header("location"), Some("/"));

    let requested = Instant::now();
    let report = first.handle().shutdown();
    assert_eq!(report.outcome, ShutdownOutcome::Graceful { exit_code: Some(0) });
    assert!(requested.elapsed() < Duration::from_secs(5), "{:?}", requested.elapsed());
    assert!(matches!(first.wait(), RunOutcome::Shutdown(_)));
    assert_port_free(port);

    let second = start_guest(&project, "server.mjs", &database, Access::ReadWrite, port);
    wait_for_listener(port);
    let listed: serde_json::Value = serde_json::from_str(&http::get(port, "/").body).unwrap();
    assert_eq!(listed[0]["name"], "Ada");
    assert_eq!(listed[0]["message"], "Hello from 1843");
    assert_eq!(
        second.handle().shutdown().outcome,
        ShutdownOutcome::Graceful { exit_code: Some(0) }
    );
    assert!(matches!(second.wait(), RunOutcome::Shutdown(_)));
    assert_port_free(port);
}
