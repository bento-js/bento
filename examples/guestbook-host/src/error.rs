use std::io;
use std::path::PathBuf;

use thiserror::Error;

use bento_guestbook_host::store::StoreError;

#[derive(Debug, Error)]
pub enum CommandError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error("cannot start the async executor: {0}")]
    Executor(#[source] io::Error),
    #[error("cannot write the entries: {0}")]
    Output(#[source] io::Error),
    #[error(
        "cannot serve `{}`: serving requires host modules and lifecycle control from the runtime, which this build does not include",
        .0.display()
    )]
    RuntimeUnavailable(PathBuf),
}
