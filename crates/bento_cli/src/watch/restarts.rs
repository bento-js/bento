use std::mem;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

use bento_runtime::{RunOutcome, RuntimeConfig, ShutdownOutcome, ShutdownReport, Signal};

use super::SourceWatcher;
use crate::ending::{self, Stop};
use crate::error::CliError;
use crate::events::{Event, next_event, send_signals};
use crate::instance::Instance;
use crate::launch::Launch;
use crate::script;
use crate::signals;

/// How often the loop looks for modules the script has newly loaded, to watch them, and for a
/// stopping script that has reached its deadline.
const CHECK_INTERVAL: Duration = Duration::from_millis(200);

/// Runs the script at `entry` and runs it again, in a new instance, after each burst of
/// changes to its source, until a signal stops it.
pub fn run(
    launch: Launch,
    entry: &Path,
    invocation_directory: &Path,
    arguments: &[String],
) -> Result<ExitCode, CliError> {
    let project_root = launch.project.root().to_owned();
    let shown_root = project_root.canonicalize().unwrap_or_else(|_| project_root.clone());
    let config = script::runtime_config(launch, invocation_directory, arguments);
    let (events, received) = mpsc::channel();
    send_signals(&events)?;
    let changes = events.clone();
    let watcher = SourceWatcher::start(&project_root, move |paths| {
        // Sending fails only once `bento` has stopped listening because it is exiting.
        let _ = changes.send(Event::Changed(paths));
    })
    .map_err(|source| CliError::Watch { path: project_root.clone(), source })?;
    let mut restarts = Restarts {
        config,
        entry: entry.to_owned(),
        project_root: shown_root,
        events,
        watcher,
        state: State::Waiting,
    };
    restarts.start();
    restarts.supervise(&received)
}

/// Where the restart loop is between instances.
enum State {
    /// No instance runs: the last one failed to load or ended by itself.
    Waiting,
    Running(Instance),
    /// The instance is shutting down so that its replacement can start.
    Restarting(Instance),
    /// A signal is stopping the instance; `bento` exits once it has ended or at the deadline.
    Stopping(Stop),
}

struct Restarts {
    config: RuntimeConfig,
    entry: PathBuf,
    /// The project root as the watcher names files in it, which messages leave out.
    project_root: PathBuf,
    events: Sender<Event>,
    watcher: SourceWatcher,
    state: State,
}

impl Restarts {
    fn supervise(&mut self, received: &Receiver<Event>) -> Result<ExitCode, CliError> {
        loop {
            let exit = match next_event(received, Some(Instant::now() + CHECK_INTERVAL)) {
                Some(Event::Changed(paths)) => self.changed(&paths),
                Some(Event::Ended(outcome)) => self.ended(outcome),
                Some(Event::Signal(signal)) => self.signalled(signal),
                None => None,
            };
            if let Some(exit) = exit {
                return exit;
            }
            self.check();
        }
    }

    fn start(&mut self) {
        match Instance::start(self.config.clone(), &self.entry, &self.events) {
            Ok(instance) => self.state = State::Running(instance),
            Err(error) => {
                CliError::Script(error).report();
                eprintln!("watch: waiting for changes before restarting");
                self.state = State::Waiting;
            }
        }
    }

    fn changed(&mut self, paths: &[PathBuf]) -> Option<Result<ExitCode, CliError>> {
        self.watch_configuration();
        match mem::replace(&mut self.state, State::Waiting) {
            State::Waiting => {
                self.announce_restart(paths);
                self.start();
            }
            State::Running(instance) => {
                self.announce_restart(paths);
                instance.shut_down();
                self.state = State::Restarting(instance);
            }
            unchanged => self.state = unchanged,
        }
        None
    }

    fn ended(&mut self, outcome: RunOutcome) -> Option<Result<ExitCode, CliError>> {
        match mem::replace(&mut self.state, State::Waiting) {
            State::Stopping(stop) => return Some(stop.exit_code(outcome)),
            State::Running(instance) => {
                self.watch_loaded_modules(&instance);
                self.report_end(outcome);
            }
            State::Restarting(instance) => {
                self.watch_loaded_modules(&instance);
                report_failure(outcome);
                self.start();
            }
            State::Waiting => unreachable!("an instance ended while none was running"),
        }
        None
    }

    fn signalled(&mut self, signal: Signal) -> Option<Result<ExitCode, CliError>> {
        match mem::replace(&mut self.state, State::Waiting) {
            State::Waiting => return Some(Ok(signals::exit_code(signal))),
            State::Running(instance) | State::Restarting(instance) => {
                let stop = Stop::begin(signal, &instance, self.config.shutdown_deadline);
                self.state = State::Stopping(stop);
            }
            State::Stopping(_) => ending::force(signal),
        }
        None
    }

    /// Watches modules the running script has loaded since the last check, and ends `bento`
    /// once a stopping instance reaches its deadline.
    fn check(&mut self) {
        let modules = match &self.state {
            State::Running(instance) => instance.handle().loaded_modules(),
            State::Stopping(stop) => {
                stop.enforce_deadline();
                return;
            }
            State::Waiting | State::Restarting(_) => return,
        };
        self.watch_new_modules(modules);
    }

    fn watch_loaded_modules(&mut self, instance: &Instance) {
        self.watch_new_modules(instance.handle().loaded_modules());
    }

    fn watch_new_modules(&mut self, modules: Vec<PathBuf>) {
        for (module, error) in self.watcher.watch_modules(modules) {
            eprintln!(
                "warning: watch: cannot watch {} for changes yet, so Bento will keep trying: \
                 {error}",
                module.display()
            );
        }
    }

    /// Watches the files `tsconfig.json` extends now, since the change may have been to it.
    fn watch_configuration(&mut self) {
        if let Err(error) = self.watcher.watch_configuration() {
            eprintln!("warning: watch: cannot watch the files tsconfig.json extends: {error}");
        }
    }

    fn announce_restart(&self, paths: &[PathBuf]) {
        let Some(first) = paths.first() else { return };
        let first = first.strip_prefix(&self.project_root).unwrap_or(first).display();
        match paths.len() {
            1 => eprintln!("watch: restarting after a change to {first}"),
            count => eprintln!("watch: restarting after changes to {first} and {} more", count - 1),
        }
    }

    fn report_end(&self, outcome: RunOutcome) {
        let script = self.entry.strip_prefix(&self.project_root).unwrap_or(&self.entry).display();
        match outcome {
            RunOutcome::Completed => eprintln!("watch: {script} finished"),
            RunOutcome::Exited(code) => eprintln!("watch: {script} exited with code {code}"),
            failure => report_failure(failure),
        }
        eprintln!("watch: waiting for changes before restarting");
    }
}

/// Reports `outcome` if the script failed; ending any other way is expected while restarting.
fn report_failure(outcome: RunOutcome) {
    match outcome {
        RunOutcome::Failed(error)
        | RunOutcome::Shutdown(ShutdownReport { outcome: ShutdownOutcome::Failed(error) }) => {
            CliError::Script(error).report();
        }
        _ => {}
    }
}
