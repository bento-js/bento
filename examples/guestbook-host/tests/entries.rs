//! The `bento-guestbook-host` binary: argument parsing and `entries`, which reads the store
//! without starting a runtime.

mod support;

use std::path::Path;

use serde_json::Value;

use bento_guestbook_host::store::{Access, GuestBookStore, NewEntry};
use support::{host_command, scratch_dir, stderr, stdout};

fn store_with_entries(database: &Path, entries: &[(&str, &str)]) {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let store = GuestBookStore::open(database, Access::ReadWrite).unwrap();
    for (name, message) in entries {
        let entry = NewEntry { name: (*name).to_owned(), message: (*message).to_owned() };
        runtime.block_on(store.append(entry)).unwrap();
    }
}

#[test]
fn entries_prints_every_saved_record_oldest_first_as_json_lines() {
    let scratch = scratch_dir();
    let database = scratch.path().join("book.db");
    store_with_entries(&database, &[("Ada", "First"), ("Grace", "Second, with\na line break")]);

    let output =
        host_command(scratch.path(), &["entries", "--database", "book.db"]).output().unwrap();
    assert!(output.status.success(), "{}", stderr(&output));

    let printed: Vec<Value> =
        stdout(&output).lines().map(|line| serde_json::from_str(line).unwrap()).collect();
    assert_eq!(printed.len(), 2);
    assert_eq!(printed[0]["id"], 1);
    assert_eq!(printed[0]["name"], "Ada");
    assert_eq!(printed[0]["message"], "First");
    assert_eq!(printed[1]["id"], 2);
    assert_eq!(printed[1]["message"], "Second, with\na line break");
    let created_at = printed[1]["createdAt"].as_str().unwrap();
    chrono::DateTime::parse_from_rfc3339(created_at).unwrap();
}

#[test]
fn entries_on_an_empty_store_prints_nothing() {
    let scratch = scratch_dir();
    let database = scratch.path().join("empty.db");
    store_with_entries(&database, &[]);

    let output =
        host_command(scratch.path(), &["entries", "--database", "empty.db"]).output().unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(stdout(&output), "");
}

#[test]
fn entries_on_a_missing_database_fails_without_creating_it() {
    let scratch = scratch_dir();
    let output =
        host_command(scratch.path(), &["entries", "--database", "absent.db"]).output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    let message = stderr(&output);
    assert!(message.contains("absent.db"), "{message}");
    assert!(!scratch.path().join("absent.db").exists());
}

#[test]
fn entries_requires_the_database_flag() {
    let scratch = scratch_dir();
    let output = host_command(scratch.path(), &["entries"]).output().unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("--database"), "{}", stderr(&output));
}

#[test]
fn serve_requires_a_project_and_a_database() {
    let scratch = scratch_dir();
    let output = host_command(scratch.path(), &["serve", "--port", "0"]).output().unwrap();
    assert_eq!(output.status.code(), Some(2));
    let message = stderr(&output);
    assert!(message.contains("--project") && message.contains("--database"), "{message}");
}

#[test]
fn serve_names_the_missing_entrypoint_inside_a_relative_project() {
    let scratch = scratch_dir();
    std::fs::create_dir(scratch.path().join("project")).unwrap();
    let arguments = ["serve", "--project", "project", "--database", "book.db", "--port", "0"];
    let output = host_command(scratch.path(), &arguments).output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    let message = stderr(&output);
    let entry = scratch.path().canonicalize().unwrap().join("project/server.embedded.ts");
    assert!(message.contains(entry.to_str().unwrap()), "{message}");
    assert!(!message.contains("project/project"), "{message}");
}

#[test]
fn serve_reports_a_missing_project_before_touching_the_database() {
    let scratch = scratch_dir();
    let arguments = ["serve", "--project", "absent", "--database", "book.db", "--port", "0"];
    let output = host_command(scratch.path(), &arguments).output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr(&output).contains("absent"), "{}", stderr(&output));
    assert!(!scratch.path().join("book.db").exists(), "a failed serve created the database");
}

#[test]
fn serve_refuses_a_database_inside_the_project_without_creating_it() {
    let scratch = scratch_dir();
    std::fs::create_dir(scratch.path().join("project")).unwrap();
    let arguments =
        ["serve", "--project", "project", "--database", "project/book.db", "--port", "0"];
    let output = host_command(scratch.path(), &arguments).output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    let message = stderr(&output);
    assert!(message.contains("inside the project"), "{message}");
    assert!(
        !scratch.path().join("project/book.db").exists(),
        "a refused serve created the database"
    );
}

#[test]
fn serve_refuses_a_database_that_resolves_inside_the_project_through_a_symlink() {
    let scratch = scratch_dir();
    std::fs::create_dir(scratch.path().join("project")).unwrap();
    std::os::unix::fs::symlink("project", scratch.path().join("elsewhere")).unwrap();
    let arguments =
        ["serve", "--project", "project", "--database", "elsewhere/book.db", "--port", "0"];
    let output = host_command(scratch.path(), &arguments).output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    let message = stderr(&output);
    assert!(message.contains("inside the project"), "{message}");
    assert!(
        !scratch.path().join("project/book.db").exists(),
        "a refused serve created the database"
    );
}

#[test]
fn serve_rejects_an_unknown_mode() {
    let scratch = scratch_dir();
    let arguments = ["serve", "--project", ".", "--database", "book.db", "--mode", "staging"];
    let output = host_command(scratch.path(), &arguments).output().unwrap();
    assert_eq!(output.status.code(), Some(2));
    let message = stderr(&output);
    assert!(message.contains("development") && message.contains("production"), "{message}");
}

#[test]
fn serve_rejects_a_port_outside_the_tcp_range() {
    let scratch = scratch_dir();
    let arguments = ["serve", "--project", ".", "--database", "book.db", "--port", "70000"];
    let output = host_command(scratch.path(), &arguments).output().unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("--port"), "{}", stderr(&output));
}

#[test]
fn an_unknown_command_is_a_usage_error() {
    let scratch = scratch_dir();
    let output = host_command(scratch.path(), &["migrate"]).output().unwrap();
    assert_eq!(output.status.code(), Some(2));
}
