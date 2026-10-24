//! How a script's run ends: by itself, or stopped by a signal `bento` received.

use std::process::{self, ExitCode};
use std::time::{Duration, Instant};

use bento_runtime::{RunOutcome, ShutdownOutcome, ShutdownReport, Signal};

use crate::error::{CliError, process_exit_code};
use crate::instance::Instance;
use crate::signals;

/// Stopping an instance because `bento` received a signal. The script hears the signal first
/// and may end by itself until the deadline; `bento` ends then if the script has not.
pub(crate) struct Stop {
    signal: Signal,
    deadline: Instant,
}

impl Stop {
    /// Delivers `signal` to the script's listeners. When it has none, the runtime's default
    /// action shuts the script down at once.
    pub(crate) fn begin(signal: Signal, instance: &Instance, deadline: Duration) -> Stop {
        instance.handle().signal(signal);
        Stop { signal, deadline: Instant::now() + deadline }
    }

    pub(crate) fn deadline(&self) -> Instant {
        self.deadline
    }

    /// Ends `bento` if the deadline has passed. Asking the runtime to shut the script down now
    /// would grant it another full deadline, so the script ends with `bento` instead.
    pub(crate) fn enforce_deadline(&self) {
        if Instant::now() >= self.deadline {
            force(self.signal);
        }
    }

    /// `bento`'s exit code once the instance has ended with `outcome`: the script's own when it
    /// ended by itself or exited while shutting down, and the signal's when Bento ended it.
    pub(crate) fn exit_code(&self, outcome: RunOutcome) -> Result<ExitCode, CliError> {
        let RunOutcome::Shutdown(ShutdownReport { outcome }) = outcome else {
            return finished(outcome);
        };
        match outcome {
            ShutdownOutcome::Failed(error) => Err(CliError::Script(error)),
            ShutdownOutcome::Graceful { exit_code: Some(exit_code) } => {
                Ok(process_exit_code(exit_code))
            }
            ShutdownOutcome::Graceful { exit_code: None }
            | ShutdownOutcome::Interrupted
            | ShutdownOutcome::TimedOut => Ok(signals::exit_code(self.signal)),
        }
    }
}

/// Ends `bento` at once: a second signal does not wait for the script.
pub(crate) fn force(signal: Signal) -> ! {
    process::exit(signals::status(signal).into())
}

/// `bento`'s exit code for a script that ended by itself with `outcome`.
pub(crate) fn finished(outcome: RunOutcome) -> Result<ExitCode, CliError> {
    match outcome {
        RunOutcome::Completed => Ok(ExitCode::SUCCESS),
        RunOutcome::Exited(code) => Ok(process_exit_code(code)),
        RunOutcome::Failed(error) => Err(CliError::Script(error)),
        RunOutcome::Shutdown(_) => unreachable!("bento shuts a script down only after a signal"),
    }
}
