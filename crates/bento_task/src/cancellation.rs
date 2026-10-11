use std::fmt;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

/// The signal that cancelled a task, forwarded to its running command's process group.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Signal {
    Interrupt,
    Terminate,
}

impl Signal {
    pub(crate) fn number(self) -> libc::c_int {
        match self {
            Signal::Interrupt => libc::SIGINT,
            Signal::Terminate => libc::SIGTERM,
        }
    }
}

impl fmt::Display for Signal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Signal::Interrupt => "SIGINT",
            Signal::Terminate => "SIGTERM",
        })
    }
}

/// A request to stop a task run, shared between the runner and whoever handles the
/// host's signals. Clones share one request; the first signal requested wins.
#[derive(Clone, Debug, Default)]
pub struct Cancellation {
    request: Arc<(Mutex<Option<Signal>>, Condvar)>,
}

impl Cancellation {
    pub fn new() -> Self {
        Cancellation::default()
    }

    pub fn cancel(&self, signal: Signal) {
        self.lock().get_or_insert(signal);
        self.request.1.notify_all();
    }

    pub(crate) fn requested(&self) -> Option<Signal> {
        *self.lock()
    }

    /// Waits up to `timeout` for a request and returns it, if one was made.
    pub(crate) fn wait(&self, timeout: Duration) -> Option<Signal> {
        let (requested, _) = self
            .request
            .1
            .wait_timeout_while(self.lock(), timeout, |requested| requested.is_none())
            .unwrap_or_else(PoisonError::into_inner);
        *requested
    }

    fn lock(&self) -> MutexGuard<'_, Option<Signal>> {
        self.request.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}
