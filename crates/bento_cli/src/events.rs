//! What the loop running a script learns about, in the order it happens.

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender};
use std::time::Instant;

use bento_runtime::{RunOutcome, Signal};

use crate::error::CliError;
use crate::signals;

pub(crate) enum Event {
    /// The instance ended and released everything it held, such as its listeners' ports.
    Ended(RunOutcome),
    /// `bento` received the signal.
    Signal(Signal),
    /// A burst of changes to these watched files ended.
    Changed(Vec<PathBuf>),
}

/// Sends each SIGINT and SIGTERM `bento` receives to `events`.
pub(crate) fn send_signals(events: &Sender<Event>) -> Result<(), CliError> {
    let events = events.clone();
    signals::listen(move |signal| {
        // Sending fails only once `bento` has stopped listening because it is exiting.
        let _ = events.send(Event::Signal(signal));
    })
    .map_err(CliError::Signals)
}

/// The next event, or `None` if `until` passes first.
pub(crate) fn next_event(received: &Receiver<Event>, until: Option<Instant>) -> Option<Event> {
    match until {
        Some(until) => received.recv_timeout(until.saturating_duration_since(Instant::now())).ok(),
        None => received.recv().ok(),
    }
}
