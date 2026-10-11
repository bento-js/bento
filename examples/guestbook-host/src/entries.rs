use std::io::{self, Write};
use std::process::ExitCode;

use bento_guestbook_host::store::{Access, GuestBookStore};

use crate::arguments::EntriesArguments;
use crate::error::CommandError;

/// Prints every saved entry as one JSON object per line, oldest first, reading the store
/// directly rather than through a runtime.
pub fn entries(arguments: &EntriesArguments) -> Result<ExitCode, CommandError> {
    let store = GuestBookStore::open(&arguments.database, Access::ReadOnly)?;
    let executor =
        tokio::runtime::Builder::new_current_thread().build().map_err(CommandError::Executor)?;
    let entries = executor.block_on(store.list())?;
    let mut stdout = io::stdout().lock();
    for entry in &entries {
        serde_json::to_writer(&mut stdout, entry)
            .map_err(io::Error::from)
            .map_err(CommandError::Output)?;
        writeln!(stdout).map_err(CommandError::Output)?;
    }
    Ok(ExitCode::SUCCESS)
}
