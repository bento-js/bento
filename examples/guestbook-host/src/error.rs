use std::io;

use thiserror::Error;

use bento_guestbook_host::launch::LaunchError;
use bento_guestbook_host::lifecycle::ServeError;
use bento_guestbook_host::store::StoreError;
use bento_runtime::RuntimeError;

#[derive(Debug, Error)]
pub(crate) enum CommandError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Launch(#[from] LaunchError),
    #[error(transparent)]
    Serve(#[from] ServeError),
    #[error(transparent)]
    Guest(RuntimeError),
    #[error("cannot start the async executor: {0}")]
    Executor(#[source] io::Error),
    #[error("cannot write the entries: {0}")]
    Output(#[source] io::Error),
}
