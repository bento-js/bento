//! Cancellation shared between a caller and the work it started.

use std::sync::Arc;

use crate::latch::Latch;

/// A cancellation signal shared by every clone.
///
/// Cancelling stops the operation it was passed to. The cancellation given to
/// [`Session::create`](crate::Session::create) additionally controls that
/// session's lifetime.
#[derive(Clone, Debug, Default)]
pub struct Cancellation {
    signal: Arc<Latch>,
}

impl Cancellation {
    /// A signal that has not been cancelled.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Cancel the work observing this signal. Cancelling twice has no further
    /// effect.
    pub fn cancel(&self) {
        self.signal.raise();
    }

    /// Whether this signal has been cancelled.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.signal.is_raised()
    }

    /// Resolve once this signal is cancelled.
    pub async fn cancelled(&self) {
        self.signal.raised().await;
    }
}
