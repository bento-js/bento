//! The Rust-owned guest book store: the persistence a reviewer inspects directly.

mod support;

use std::fs;
use std::path::Path;

use chrono::{DateTime, Utc};
use rusqlite::Connection;

use bento_guestbook_host::store::{Access, Entry, GuestBookStore, NewEntry, StoreError};
use support::scratch_dir;

fn signature(name: &str, message: &str) -> NewEntry {
    NewEntry { name: name.to_owned(), message: message.to_owned() }
}

fn rows_in(database: &Path) -> Vec<(i64, String, String, String)> {
    let connection = Connection::open(database).unwrap();
    let mut statement =
        connection.prepare("SELECT id, name, message, createdAt FROM entries ORDER BY id").unwrap();
    let rows = statement
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)))
        .unwrap();
    rows.map(Result::unwrap).collect()
}

#[tokio::test]
async fn opening_a_new_path_creates_an_empty_store() {
    let scratch = scratch_dir();
    let database = scratch.path().join("new.db");
    let store = GuestBookStore::open(&database, Access::ReadWrite).unwrap();
    assert_eq!(store.list().await.unwrap(), Vec::<Entry>::new());
    assert!(database.is_file());
}

#[tokio::test]
async fn appended_entries_list_oldest_first_with_rfc3339_utc_timestamps() {
    let scratch = scratch_dir();
    let store = GuestBookStore::open(&scratch.path().join("book.db"), Access::ReadWrite).unwrap();
    let before = Utc::now();
    store.append(signature("Ada", "First")).await.unwrap();
    store.append(signature("Grace", "Second")).await.unwrap();
    let after = Utc::now();

    let entries = store.list().await.unwrap();
    let summary: Vec<(i64, &str, &str)> =
        entries.iter().map(|entry| (entry.id, &*entry.name, &*entry.message)).collect();
    assert_eq!(summary, [(1, "Ada", "First"), (2, "Grace", "Second")]);
    for entry in &entries {
        assert!(entry.created_at.ends_with('Z'), "{}", entry.created_at);
        let created_at =
            DateTime::parse_from_rfc3339(&entry.created_at).unwrap().with_timezone(&Utc);
        let window = before.timestamp()..=after.timestamp();
        assert!(window.contains(&created_at.timestamp()), "{created_at} outside {window:?}");
    }
}

#[tokio::test]
async fn entries_are_readable_directly_from_sqlite_and_survive_reopening() {
    let scratch = scratch_dir();
    let database = scratch.path().join("book.db");
    {
        let store = GuestBookStore::open(&database, Access::ReadWrite).unwrap();
        store.append(signature("Ada", "Hello")).await.unwrap();
    }
    let rows = rows_in(&database);
    assert_eq!(rows.len(), 1);
    let (id, name, message, created_at) = &rows[0];
    assert_eq!((*id, name.as_str(), message.as_str()), (1, "Ada", "Hello"));
    DateTime::parse_from_rfc3339(created_at).unwrap();

    let reopened = GuestBookStore::open(&database, Access::ReadWrite).unwrap();
    let entries = reopened.list().await.unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].message, "Hello");
}

#[tokio::test]
async fn read_only_append_is_denied_without_touching_the_database() {
    let scratch = scratch_dir();
    let database = scratch.path().join("book.db");
    GuestBookStore::open(&database, Access::ReadWrite)
        .unwrap()
        .append(signature("Ada", "Kept"))
        .await
        .unwrap();
    let bytes_before = fs::read(&database).unwrap();

    let read_only = GuestBookStore::open(&database, Access::ReadOnly).unwrap();
    let refusal = read_only.append(signature("Mallory", "Dropped")).await.unwrap_err();
    assert!(matches!(refusal, StoreError::PermissionDenied), "{refusal:?}");

    let listed = read_only.list().await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].message, "Kept");
    assert_eq!(rows_in(&database).len(), 1);
    assert_eq!(fs::read(&database).unwrap(), bytes_before);
}

#[test]
fn a_read_only_store_needs_an_existing_database() {
    let scratch = scratch_dir();
    let database = scratch.path().join("absent.db");
    let failure = GuestBookStore::open(&database, Access::ReadOnly).unwrap_err();
    assert!(matches!(&failure, StoreError::Missing(path) if path == &database), "{failure:?}");
    assert!(!database.exists());
}
