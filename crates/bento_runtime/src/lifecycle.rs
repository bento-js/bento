use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use deno_core::v8::IsolateHandle;
use parking_lot::{Condvar, Mutex};
use tokio::sync::{mpsc, watch};

use crate::{RunOutcome, RuntimeError, StartError};

// Reserve a bounded window for isolate/resource destruction after interrupting JavaScript.
// Expiry reports incomplete disposal, not proof that native execution is the cause.
const DISPOSAL_MARGIN: Duration = Duration::from_millis(100);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Signal {
    Interrupt,
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ShutdownOutcome {
    Graceful { exit_code: Option<i32> },
    Failed(RuntimeError),
    Interrupted,
    TimedOut,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShutdownReport {
    pub outcome: ShutdownOutcome,
}

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
    report: Option<ShutdownReport>,
    feature_failure: Option<RuntimeError>,
    signals: BTreeMap<u64, (String, watch::Sender<()>)>,
    modules: BTreeSet<PathBuf>,
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
    changed: Condvar,
    commands: mpsc::UnboundedSender<Command>,
    deadline: Duration,
}

impl RuntimeHandle {
    pub fn signal(&self, signal: Signal) {
        if !self.control.deliver(signal.name()) {
            self.control.request_shutdown();
        }
    }

    pub fn shutdown(&self) -> ShutdownReport {
        self.control.request_shutdown();
        let mut state = self.control.state.lock();
        while state.report.is_none() {
            self.control.changed.wait(&mut state);
        }
        state.report.clone().expect("shutdown report available")
    }

    pub fn loaded_modules(&self) -> Vec<PathBuf> {
        self.control.state.lock().modules.iter().cloned().collect()
    }
}

impl Control {
    pub fn new(
        deadline: Duration,
    ) -> Result<(Arc<Self>, mpsc::UnboundedReceiver<Command>), StartError> {
        let (commands, receiver) = mpsc::unbounded_channel();
        let control = Arc::new(Self {
            state: Mutex::new(State::default()),
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

    pub fn dispose(&self) {
        let _ = self.commands.send(Command::Dispose);
    }

    pub fn install_isolate(&self, isolate: IsolateHandle) {
        let mut state = self.state.lock();
        if state.interrupted {
            isolate.terminate_execution();
        }
        state.isolate = Some(isolate);
    }

    pub fn register_signal(&self, name: String, sender: watch::Sender<()>) -> u64 {
        let mut state = self.state.lock();
        state.next_id += 1;
        let id = state.next_id;
        state.signals.insert(id, (name, sender));
        id
    }

    pub fn unregister_signal(&self, id: u64) {
        self.state.lock().signals.remove(&id);
    }

    pub fn deliver(&self, name: &str) -> bool {
        let state = self.state.lock();
        let mut delivered = false;
        for (registered, sender) in state.signals.values() {
            if registered == name {
                delivered |= sender.send(()).is_ok();
            }
        }
        delivered
    }

    pub fn register_worker(&self, isolate: IsolateHandle) -> u64 {
        let mut state = self.state.lock();
        if state.interrupted || state.closing_workers {
            isolate.terminate_execution();
        }
        state.next_id += 1;
        let id = state.next_id;
        state.workers.insert(id, isolate);
        id
    }

    pub fn finish_worker(&self, id: u64) {
        self.state.lock().workers.remove(&id);
        self.changed.notify_all();
    }

    pub fn close_workers(&self) {
        let mut state = self.state.lock();
        state.closing_workers = true;
        for isolate in state.workers.values() {
            isolate.terminate_execution();
        }
        while !state.workers.is_empty() {
            self.changed.wait(&mut state);
        }
    }

    pub fn record_module(&self, path: &Path) {
        self.state.lock().modules.insert(path.to_owned());
    }

    pub fn default_shutdown(&self) {
        self.state.lock().default_shutdown = true;
    }

    pub fn fail_feature(&self, error: RuntimeError) {
        let mut state = self.state.lock();
        state.feature_failure.get_or_insert(error);
        state.terminate_isolates();
    }

    pub fn feature_failure(&self) -> Option<RuntimeError> {
        self.state.lock().feature_failure.clone()
    }

    // A completion report is retained for later shutdown calls, but returned only for requested shutdown.
    pub fn complete(&self, execution: &RunOutcome) -> Option<ShutdownReport> {
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
        state.requested.then_some(report)
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
