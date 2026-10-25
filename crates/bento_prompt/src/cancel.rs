//! Cancellation shared between a caller and the work it started.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::Poll;

use crate::latch::Latch;

/// A cancellation signal shared by every clone.
///
/// Cancelling stops the operation it was passed to. The cancellation given to
/// [`Session::create`](crate::Session::create) additionally controls that
/// session's lifetime.
#[derive(Clone, Debug, Default)]
pub struct Cancellation {
    signal: Arc<Latch>,
    /// Signals this one was derived from. Cancelling any of them cancels this
    /// one; cancelling this one leaves them alone.
    inherited: Arc<Vec<Arc<Latch>>>,
}

impl Cancellation {
    /// A signal that has not been cancelled.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// A signal for one piece of the work this one covers.
    ///
    /// Cancelling this signal cancels the child with it, so a caller still
    /// stops everything it started. Cancelling or abandoning the child stops
    /// only that piece, leaving whatever else shares this signal running.
    pub(crate) fn child(&self) -> Self {
        let mut inherited = Vec::with_capacity(self.inherited.len() + 1);
        inherited.extend(self.inherited.iter().map(Arc::clone));
        inherited.push(Arc::clone(&self.signal));
        Self { signal: Arc::new(Latch::default()), inherited: Arc::new(inherited) }
    }

    /// Cancel the work observing this signal. Cancelling twice has no further
    /// effect.
    pub fn cancel(&self) {
        self.signal.raise();
    }

    /// Whether this signal has been cancelled.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.signal.is_raised() || self.inherited.iter().any(|signal| signal.is_raised())
    }

    /// Resolve once this signal is cancelled.
    pub async fn cancelled(&self) {
        let mut waits: Vec<Pin<Box<dyn Future<Output = ()> + Send + '_>>> =
            std::iter::once(&self.signal)
                .chain(self.inherited.iter())
                .map(|signal| Box::pin(signal.raised()) as Pin<Box<_>>)
                .collect();
        std::future::poll_fn(move |context| {
            for wait in &mut waits {
                if wait.as_mut().poll(context).is_ready() {
                    return Poll::Ready(());
                }
            }
            Poll::Pending
        })
        .await;
    }
}
