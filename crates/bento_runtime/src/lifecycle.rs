use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use deno_core::v8::IsolateHandle;
use parking_lot::{Condvar, Mutex};
use tokio::sync::{mpsc, watch};

use crate::{RunOutcome, RuntimeError, StartError};

// Reserve a bounded window for isolate/resource destruction after interrupting JavaScript.
// Expiry reports incomplete disposal, not proof that native execution is the cause.
const DISPOSAL_MARGIN: Duration = Duration::from_millis(100);

/// A process signal a host forwards into one instance.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Signal {
    /// `SIGINT`, as a terminal sends it on Ctrl-C.
    Interrupt,
    /// `SIGTERM`, the stop request a supervisor sends.
    Terminate,
}

impl Signal {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Interrupt => "SIGINT",
            Self::Terminate => "SIGTERM",
        }
    }
}

/// How an instance ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ShutdownOutcome {
    /// The guest stopped on its own terms rather than being terminated.
    Graceful {
        /// `Some(0)` when the guest ran to the end, `Some(code)` when it exited with one,
        /// `None` when it stopped only because shutdown was requested.
        exit_code: Option<i32>,
    },
    /// Startup failed, or the guest ended on an error it did not handle.
    Failed(RuntimeError),
    /// The shutdown deadline expired, so execution was terminated mid-flight.
    Interrupted,
    /// Terminated execution did not release the isolate and its resources either; disposal
    /// is incomplete, and the thread may still be inside native code.
    TimedOut,
}

/// What one instance's shutdown came to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShutdownReport {
    /// The ending that was recorded, which is the instance's own if it had already finished
    /// before shutdown was requested.
    pub outcome: ShutdownOutcome,
}

/// Signal and shutdown access to one started instance. Clonable, and usable from any thread
/// for as long as the instance exists.
#[derive(Clone)]
pub struct RuntimeHandle {
    pub(crate) control: Arc<Control>,
}

pub(crate) enum Command {
    Shutdown,
    Dispose,
}

#[derive(Default)]
struct State {
    isolate: Option<IsolateHandle>,
    requested: bool,
    default_shutdown: bool,
    shutdown_at: Option<Instant>,
    workers: BTreeMap<u64, IsolateHandle>,
    next_id: u64,
    closing_workers: bool,
    interrupted: bool,
    disposed: bool,
    incomplete: bool,
    report: Option<ShutdownReport>,
    feature_failure: Option<RuntimeError>,
    signals: BTreeMap<u64, (String, watch::Sender<()>)>,
}

impl State {
    fn terminate_isolates(&self) {
        for isolate in self.isolate.iter().chain(self.workers.values()) {
            isolate.terminate_execution();
        }
    }
}

pub(crate) struct Control {
    state: Mutex<State>,
    watch_inputs: Arc<Mutex<BTreeSet<PathBuf>>>,
    changed: Condvar,
    commands: mpsc::UnboundedSender<Command>,
    deadline: Duration,
}

impl RuntimeHandle {
    /// Delivers `signal` to the guest's handlers for it and returns at once. A guest with no
    /// handler for that signal is shut down instead, without waiting for the report.
    pub fn signal(&self, signal: Signal) {
        if !self.control.deliver(signal.name()) {
            self.control.request_shutdown();
        }
    }

    /// Asks the instance to stop and blocks until it has ended, at most for the config's
    /// shutdown deadline plus a short disposal margin. Later calls return the same report.
    ///
    /// # Panics
    ///
    /// Never: the report is only awaited here, and it is never cleared once recorded.
    pub fn shutdown(&self) -> ShutdownReport {
        self.control.request_shutdown();
        let mut state = self.control.state.lock();
        while state.report.is_none() {
            self.control.changed.wait(&mut state);
        }
        state.report.clone().expect("shutdown report available")
    }

    /// Sorted, deduplicated snapshot of loaded sources, failed application targets, and package scopes.
    /// Excludes unused successful-resolution candidates and requests with dependency or synthetic parents.
    /// Included regardless of origin: dependency code borrowing a cached application parent, a URL the
    /// application itself resolved, and missing worker scripts outside `node_modules`, whoever starts them.
    pub fn watch_inputs(&self) -> Vec<PathBuf> {
        self.control.watch_inputs.lock().iter().cloned().collect()
    }
}

impl Control {
    pub(crate) fn new(
        deadline: Duration,
    ) -> Result<(Arc<Self>, mpsc::UnboundedReceiver<Command>), StartError> {
        let (commands, receiver) = mpsc::unbounded_channel();
        let control = Arc::new(Self {
            state: Mutex::new(State::default()),
            watch_inputs: Arc::default(),
            changed: Condvar::new(),
            commands,
            deadline,
        });
        let monitor = control.clone();
        std::thread::Builder::new()
            .name("bento-shutdown".into())
            .spawn(move || monitor.monitor_shutdown())
            .map_err(|error| RuntimeError::startup(format!("shutdown monitor: {error}")))?;
        Ok((control, receiver))
    }

    pub(crate) fn dispose(&self) {
        let _ = self.commands.send(Command::Dispose);
    }

    pub(crate) fn install_isolate(&self, isolate: IsolateHandle) {
        let mut state = self.state.lock();
        if state.interrupted {
            isolate.terminate_execution();
        }
        state.isolate = Some(isolate);
    }

    pub(crate) fn register_signal(&self, name: String, sender: watch::Sender<()>) -> u64 {
        let mut state = self.state.lock();
        state.next_id += 1;
        let id = state.next_id;
        state.signals.insert(id, (name, sender));
        id
    }

    pub(crate) fn unregister_signal(&self, id: u64) {
        self.state.lock().signals.remove(&id);
    }

    pub(crate) fn deliver(&self, name: &str) -> bool {
        let state = self.state.lock();
        let mut delivered = false;
        for (registered, sender) in state.signals.values() {
            if registered == name {
                delivered |= sender.send(()).is_ok();
            }
        }
        delivered
    }

    pub(crate) fn register_worker(&self, isolate: IsolateHandle) -> u64 {
        let mut state = self.state.lock();
        if state.interrupted || state.closing_workers {
            isolate.terminate_execution();
        }
        state.next_id += 1;
        let id = state.next_id;
        state.workers.insert(id, isolate);
        id
    }

    pub(crate) fn finish_worker(&self, id: u64) {
        self.state.lock().workers.remove(&id);
        self.changed.notify_all();
    }

    pub(crate) fn close_workers(&self) {
        let mut state = self.state.lock();
        state.closing_workers = true;
        for isolate in state.workers.values() {
            isolate.terminate_execution();
        }
        while !state.workers.is_empty() {
            self.changed.wait(&mut state);
        }
    }

    pub(crate) fn watch_inputs(&self) -> Arc<Mutex<BTreeSet<PathBuf>>> {
        self.watch_inputs.clone()
    }

    pub(crate) fn default_shutdown(&self) {
        self.state.lock().default_shutdown = true;
    }

    pub(crate) fn fail_feature(&self, error: RuntimeError) {
        let mut state = self.state.lock();
        state.feature_failure.get_or_insert(error);
        state.terminate_isolates();
    }

    pub(crate) fn feature_failure(&self) -> Option<RuntimeError> {
        self.state.lock().feature_failure.clone()
    }

    /// Records that owned native work outlived the instance's disposal deadline, so this
    /// shutdown is reported as incomplete rather than graceful.
    pub(crate) fn incomplete_disposal(&self) {
        let mut state = self.state.lock();
        state.incomplete = true;
        state.report.get_or_insert(ShutdownReport { outcome: ShutdownOutcome::TimedOut });
        self.changed.notify_all();
    }

    // A completion report is retained for later shutdown calls, and returned here for a
    // requested shutdown and for a disposal that did not finish.
    pub(crate) fn complete(&self, execution: &RunOutcome) -> Option<ShutdownReport> {
        let mut state = self.state.lock();
        state.disposed = true;
        state.isolate = None;
        state.signals.clear();
        let outcome = match execution {
            _ if state.interrupted => ShutdownOutcome::Interrupted,
            RunOutcome::Failed(error) => ShutdownOutcome::Failed(error.clone()),
            RunOutcome::Exited(exit_code) => {
                ShutdownOutcome::Graceful { exit_code: Some(*exit_code) }
            }
            RunOutcome::Completed if !state.default_shutdown => {
                ShutdownOutcome::Graceful { exit_code: Some(0) }
            }
            _ => ShutdownOutcome::Graceful { exit_code: None },
        };
        let report = state.report.get_or_insert(ShutdownReport { outcome }).clone();
        self.changed.notify_all();
        (state.requested || state.incomplete).then_some(report)
    }

    fn request_shutdown(&self) {
        let mut state = self.state.lock();
        if state.disposed || state.requested {
            return;
        }
        state.requested = true;
        state.shutdown_at = Some(Instant::now() + self.deadline);
        self.changed.notify_all();
        let _ = self.commands.send(Command::Shutdown);
    }

    fn monitor_shutdown(&self) {
        let mut state = self.state.lock();
        while state.shutdown_at.is_none() && !state.disposed {
            self.changed.wait(&mut state);
        }
        if let Some(deadline) = state.shutdown_at {
            drop(state);
            self.enforce_deadline(deadline);
        }
    }

    fn enforce_deadline(&self, deadline: Instant) {
        let mut state = self.state.lock();
        while !state.disposed {
            if self.changed.wait_until(&mut state, deadline).timed_out() {
                break;
            }
        }
        if state.disposed {
            return;
        }
        state.interrupted = true;
        state.terminate_isolates();
        self.dispose();
        let disposal_deadline = Instant::now() + DISPOSAL_MARGIN;
        while !state.disposed {
            if self.changed.wait_until(&mut state, disposal_deadline).timed_out() {
                state.report = Some(ShutdownReport { outcome: ShutdownOutcome::TimedOut });
                self.changed.notify_all();
                return;
            }
        }
    }
}
