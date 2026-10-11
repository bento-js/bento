//! Guest book entries in a SQLite database the host owns.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use rusqlite::{Connection, OpenFlags, params};
use serde::{Deserialize, Serialize};
use thiserror::Error;

const SCHEMA: &str = "
    CREATE TABLE IF NOT EXISTS entries (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        name TEXT NOT NULL,
        message TEXT NOT NULL,
        createdAt TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
    )
";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Access {
    ReadWrite,
    ReadOnly,
}

/// A saved entry as the guest receives it: `createdAt` is RFC 3339 UTC, such as
/// `2026-10-10T18:30:05Z`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub id: i64,
    pub name: String,
    pub message: String,
    pub created_at: String,
}

/// The fields a guest supplies; the store assigns `id` and `createdAt`.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct NewEntry {
    pub name: String,
    pub message: String,
}

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("no guest book database at {}; run `serve` without --read-only to create one", .0.display())]
    Missing(PathBuf),
    #[error("the guest book is read-only")]
    PermissionDenied,
    #[error("guest book database: {0}")]
    Database(#[from] rusqlite::Error),
}

/// A handle on the database that host-module handlers can clone into their futures. SQLite work
/// runs on Tokio's blocking pool so it never stalls the runtime's event loop.
#[derive(Clone, Debug)]
pub struct GuestBookStore {
    connection: Arc<Mutex<Connection>>,
    access: Access,
}

impl GuestBookStore {
    /// Opens the database at `path`, creating it and the host's table in read-write mode. A
    /// read-only store never writes: the file must already exist.
    pub fn open(path: &Path, access: Access) -> Result<GuestBookStore, StoreError> {
        let connection = match access {
            Access::ReadWrite => {
                let connection = Connection::open(path)?;
                connection.execute_batch(SCHEMA)?;
                connection
            }
            Access::ReadOnly => {
                if !path.is_file() {
                    return Err(StoreError::Missing(path.to_owned()));
                }
                let flags = OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX;
                Connection::open_with_flags(path, flags)?
            }
        };
        Ok(GuestBookStore { connection: Arc::new(Mutex::new(connection)), access })
    }

    /// Every entry, oldest first.
    pub async fn list(&self) -> Result<Vec<Entry>, StoreError> {
        self.blocking(|connection| {
            let mut statement = connection
                .prepare("SELECT id, name, message, createdAt FROM entries ORDER BY id")?;
            let entries = statement.query_map([], |row| {
                Ok(Entry {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    message: row.get(2)?,
                    created_at: row.get(3)?,
                })
            })?;
            Ok(entries.collect::<Result<_, _>>()?)
        })
        .await
    }

    /// Saves `entry`; a read-only store refuses before touching the database.
    pub async fn append(&self, entry: NewEntry) -> Result<(), StoreError> {
        if self.access == Access::ReadOnly {
            return Err(StoreError::PermissionDenied);
        }
        self.blocking(move |connection| {
            connection.execute(
                "INSERT INTO entries (name, message) VALUES (?1, ?2)",
                params![entry.name, entry.message],
            )?;
            Ok(())
        })
        .await
    }

    async fn blocking<T>(
        &self,
        work: impl FnOnce(&Connection) -> Result<T, StoreError> + Send + 'static,
    ) -> Result<T, StoreError>
    where
        T: Send + 'static,
    {
        let connection = Arc::clone(&self.connection);
        tokio::task::spawn_blocking(move || {
            let connection = connection.lock().expect("guest book connection lock poisoned");
            work(&connection)
        })
        .await
        .expect("guest book storage work panicked")
    }
}
