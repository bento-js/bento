//! `host:guestbook`: the store's `list` and `append` as asynchronous functions over JSON values.

use serde_json::Value;

use bento_runtime::{HostError, HostModule};

use crate::store::{GuestBookStore, NewEntry, StoreError};

pub const SPECIFIER: &str = "host:guestbook";

/// The module one runtime instance imports as `host:guestbook`. `list()` resolves to the saved
/// entries oldest first; `append({ name, message })` saves one. A refusal rejects with the
/// store error's name, so a read-only store surfaces as `PermissionDenied`.
pub fn guest_book_module(store: GuestBookStore) -> HostModule {
    let listing = store.clone();
    HostModule::new(SPECIFIER)
        .function("list", move |_arguments| {
            let store = listing.clone();
            async move {
                let entries = store.list().await.map_err(host_error)?;
                Ok(serde_json::to_value(entries).expect("entries serialize as JSON"))
            }
        })
        .function("append", move |arguments| {
            let store = store.clone();
            async move {
                let entry = new_entry(arguments)?;
                store.append(entry).await.map_err(host_error)?;
                Ok(Value::Null)
            }
        })
}

/// `arguments` is the guest's argument list; `append` takes a single `{ name, message }`.
fn new_entry(arguments: Value) -> Result<NewEntry, HostError> {
    let first = match arguments {
        Value::Array(mut arguments) if !arguments.is_empty() => arguments.swap_remove(0),
        _ => Value::Null,
    };
    serde_json::from_value(first).map_err(|error| HostError {
        name: "TypeError".to_owned(),
        message: format!("append expects {{ name, message }}: {error}"),
    })
}

fn host_error(error: StoreError) -> HostError {
    let name = match error {
        StoreError::PermissionDenied => "PermissionDenied",
        StoreError::Missing(_) => "NotFound",
        StoreError::Database(_) => "DatabaseError",
    };
    HostError { name: name.to_owned(), message: error.to_string() }
}
