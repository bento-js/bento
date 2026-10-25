//! A one-way flag that tasks can wait on.

use std::sync::atomic::{AtomicBool, Ordering};

use tokio::sync::Notify;

/// A flag that is raised once and never lowered.
///
/// Waiting resolves for every waiter, including one that arrives after the
/// latch was already raised.
#[derive(Debug, Default)]
pub(crate) struct Latch {
    raised: AtomicBool,
    signal: Notify,
}

impl Latch {
    /// Raise the latch and wake everyone waiting on it. Raising it again has
    /// no further effect.
    pub(crate) fn raise(&self) {
        if !self.raised.swap(true, Ordering::SeqCst) {
            self.signal.notify_waiters();
        }
    }

    pub(crate) fn is_raised(&self) -> bool {
        self.raised.load(Ordering::SeqCst)
    }

    /// Resolve once the latch is raised.
    pub(crate) async fn raised(&self) {
        loop {
            // Subscribing before the check is what closes the gap between a
            // raise and a waiter that was about to start waiting for it.
            let notified = self.signal.notified();
            if self.is_raised() {
                return;
            }
            notified.await;
        }
    }
}
