//! The runtime that owns this crate's native work.
//!
//! Sessions must outlive the runtime that asked for them: a worker isolate's
//! runtime can shut down while the model is still counting tokens, opening a
//! session, generating, or releasing resources. Every such task runs here
//! instead, so cancellation and release always complete and disposal never
//! reports work that is still running.
//!
//! It is started on first use and only when there is native work to own, so a
//! build without a compiled backend never creates a thread.

use std::sync::LazyLock;

use tokio::runtime::{Builder, Runtime};

use crate::{Error, ErrorKind};

static EXECUTOR: LazyLock<std::io::Result<Runtime>> = LazyLock::new(|| {
    Builder::new_multi_thread().worker_threads(1).thread_name("bento-prompt").enable_all().build()
});

/// The runtime that owns native work, or the reason it is unavailable.
///
/// Starting it can fail when the operating system refuses a thread; that is an
/// operation failure to report, not a reason to panic inside a caller.
pub(crate) fn runtime() -> Result<&'static Runtime, Error> {
    EXECUTOR.as_ref().map_err(|error| {
        Error::new(
            ErrorKind::Operation,
            format!("the prompt runtime could not be started: {error}"),
        )
    })
}
