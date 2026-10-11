//! How a script's run ends: by itself, or stopped by a signal `bento` received.

use std::process::{self, ExitCode};
use std::time::{Duration, Instant};

use bento_runtime::{RunOutcome, ShutdownOutcome, ShutdownReport, Signal};

use crate::error::CliError;
use crate::instance::Instance;
use crate::signals;

/// Stopping an instance because `bento` received a signal. The script hears the signal first
/// and may end by itself until the deadline; `bento` ends then if the script has not.
pub struct Stop {
    signal: Signal,
    deadline: Instant,
}

impl Stop {
    /// Delivers `signal` to the script's listeners. When it has none, the runtime's default
    /// action shuts the script down at once.
    pub fn begin(signal: Signal, instance: &Instance, deadline: Duration) -> Stop {
        instance.handle().signal(signal);
        Stop { signal, deadline: Instant::now() + deadline }
    }

    pub fn deadline(&self) -> Instant {
        self.deadline
    }

    /// Ends `bento` if the deadline has passed. Asking the runtime to shut the script down now
    /// would grant it another full deadline, so the script ends with `bento` instead.
    pub fn enforce_deadline(&self) {
        if Instant::now() >= self.deadline {
            force(self.signal);
        }
    }

    /// `bento`'s exit code once the instance has ended with `outcome`: the script's own when it
    /// ended by itself, and the signal's when it was shut down, unless the script exited with a
    /// failure code while shutting down.
    pub fn exit_code(&self, outcome: RunOutcome) -> Result<ExitCode, CliError> {
        let RunOutcome::Shutdown(ShutdownReport { outcome }) = outcome else {
            return finished(outcome);
        };
        match outcome {
            ShutdownOutcome::Failed(error) => Err(CliError::Script(error)),
            // The runtime reports a script it closed for want of a listener as exiting with 0,
            // so only a nonzero code is certainly the script's own.
            ShutdownOutcome::Graceful { exit_code } if exit_code != 0 => {
                Ok(process_exit_code(exit_code))
            }
            _ => Ok(signals::exit_code(self.signal)),
        }
    }
}

/// Ends `bento` at once: a second signal does not wait for the script.
pub fn force(signal: Signal) -> ! {
    process::exit(signals::status(signal).into())
}

/// `bento`'s exit code for a script that ended by itself with `outcome`.
pub fn finished(outcome: RunOutcome) -> Result<ExitCode, CliError> {
    match outcome {
        RunOutcome::Completed => Ok(ExitCode::SUCCESS),
        RunOutcome::Exited(code) => Ok(process_exit_code(code)),
        RunOutcome::Failed(error) => Err(CliError::Script(error)),
        RunOutcome::Shutdown(_) => unreachable!("bento shuts a script down only after a signal"),
    }
}

/// The status the operating system reports for `process.exit(code)`: the code's low eight
/// bits, as for any process.
fn process_exit_code(code: i32) -> ExitCode {
    ExitCode::from(code as u8)
}
