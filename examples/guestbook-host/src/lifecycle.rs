//! The host-owned lifetime of one served instance: it runs over the store until its entry
//! finishes or Ctrl-C or `SIGTERM` reaches the host process, and reports how it ended.

use std::io;
use std::os::raw::c_int;
use std::path::Path;
use std::sync::mpsc;
use std::thread;

use signal_hook::consts::{SIGINT, SIGTERM};
use signal_hook::iterator::Signals;
use thiserror::Error;

use bento_runtime::{RunOutcome, RuntimeHandle, ShutdownOutcome, StartError, start};

use crate::launch::Launch;
use crate::store::{Access, GuestBookStore, StoreError};

#[derive(Debug, Error)]
pub enum ServeError {
    #[error("cannot handle Ctrl-C and SIGTERM: {0}")]
    Signals(#[source] io::Error),
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Runtime(#[from] StartError),
}

/// How a served instance ended: its outcome, and the OS signal that made the host stop it when
/// one did (`None` when the entry ended on its own).
#[derive(Debug)]
pub struct Served {
    pub outcome: RunOutcome,
    pub signal: Option<c_int>,
}

/// Runs `entry` (absolute, or relative to the project) in one instance configured by `launch`
/// over the store at `launch.database()`, and reports how the instance ended.
///
/// The host owns Ctrl-C and `SIGTERM` from before the runtime exists: the first one asks the
/// runtime to shut down, which delivers `SIGTERM` to the guest's own handler and waits for it
/// within the deadline. A signal that arrives while the guest is still starting ends it the
/// same way.
pub fn serve(launch: &Launch, access: Access, entry: &Path) -> Result<Served, ServeError> {
    let mut signals = Signals::new([SIGINT, SIGTERM]).map_err(ServeError::Signals)?;
    let store = GuestBookStore::open(launch.database(), access)?;
    let runtime = start(launch.runtime_config(store), entry)?;
    let handle = runtime.handle();

    let (outcomes, first_outcome) = mpsc::channel();
    let forwarding = signals.handle();
    let forwarder = {
        let outcomes = outcomes.clone();
        thread::spawn(move || shutdown_on_signal(&mut signals, &handle, &outcomes))
    };
    thread::spawn(move || {
        let _ = outcomes.send(runtime.wait());
    });
    let outcome = first_outcome.recv().expect("the runtime or the forwarder reports an outcome");
    forwarding.close();
    let signal = forwarder.join().expect("signal forwarder panicked");
    Ok(Served { outcome, signal })
}

/// Shuts the runtime down on the first signal and returns it. A shutdown that times out means
/// native code never yielded, so the runtime thread cannot finish and `wait` would block
/// forever; the report is then the outcome.
fn shutdown_on_signal(
    signals: &mut Signals,
    handle: &RuntimeHandle,
    outcomes: &mpsc::Sender<RunOutcome>,
) -> Option<c_int> {
    let signal = signals.forever().next()?;
    let report = handle.shutdown();
    if report.outcome == ShutdownOutcome::TimedOut {
        let _ = outcomes.send(RunOutcome::Shutdown(report));
    }
    Some(signal)
}
