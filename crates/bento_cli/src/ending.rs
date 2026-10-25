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
        // A script that ended by itself reports a shutdown only when its disposal did not
        // finish: model work it started outlived the deadline, so nothing can say the run
        // is over and done with.
        RunOutcome::Shutdown(ShutdownReport { outcome: ShutdownOutcome::TimedOut }) => {
            Err(CliError::IncompleteDisposal)
        }
        RunOutcome::Shutdown(_) => {
            unreachable!("a script that ended by itself reports only incomplete disposal")
        }
    }
}

#[cfg(test)]
mod tests {
    use std::process::ExitCode;

    use bento_runtime::{RunOutcome, ShutdownOutcome, ShutdownReport};

    use super::finished;

    /// A script can end on its own while the model work it started is still running. The
    /// runtime reports that as incomplete disposal, and `bento` says so and fails rather
    /// than claiming the script finished cleanly.
    #[test]
    fn a_script_whose_disposal_did_not_finish_is_reported_as_an_error() {
        let outcome = RunOutcome::Shutdown(ShutdownReport { outcome: ShutdownOutcome::TimedOut });

        let error = finished(outcome).expect_err("incomplete disposal is not a clean ending");

        assert!(
            error.to_string().contains("disposal is incomplete"),
            "the ending a user sees must say what was left unfinished: {error}"
        );
        assert_eq!(format!("{:?}", error.exit_code()), format!("{:?}", ExitCode::from(1)));
    }
}
