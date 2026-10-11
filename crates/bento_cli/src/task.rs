use std::env;
use std::process::{ExitCode, ExitStatus};

use crate::error::{self, CliError};
use crate::launch::Launch;
use crate::signals;
use bento_runtime::Signal;
use bento_task::{Cancellation, Plan, TaskError, TaskRunner};

/// The arguments for task `name` among the words after it on the command line: those after
/// a leading `--`. Any other word there is a misplaced option or argument.
pub fn arguments<'a>(name: &str, words: &'a [String]) -> Result<&'a [String], CliError> {
    match words {
        [] => Ok(words),
        [separator, arguments @ ..] if separator == "--" => Ok(arguments),
        _ => Err(CliError::TaskArgumentsWithoutSeparator { task: name.to_owned() }),
    }
}

/// Plans task `name` and runs it until it finishes, fails, or a signal cancels it.
pub fn run(launch: &Launch, name: &str, arguments: &[String]) -> Result<ExitCode, CliError> {
    let plan = Plan::new(launch.project.manifest(), name, arguments)?;
    // A nested `bento` may have been started through the outer launch's temporary link,
    // which disappears when that launch ends; link to the executable itself instead.
    let executable = env::current_exe()
        .and_then(|executable| executable.canonicalize())
        .map_err(CliError::Executable)?;
    let runner =
        TaskRunner::new(&launch.project, &launch.environment, &launch.grants, &executable)?;
    let cancellation = Cancellation::new();
    let cancel = cancellation.clone();
    signals::listen(move |signal| cancel.cancel(task_signal(signal))).map_err(CliError::Signals)?;
    match runner.run(&plan, &cancellation) {
        Ok(()) => Ok(ExitCode::SUCCESS),
        Err(cancelled @ TaskError::Cancelled { signal, status, .. }) => {
            eprintln!("{cancelled}");
            Ok(cancelled_exit_code(signal, status))
        }
        Err(error) => Err(error.into()),
    }
}

/// `bento`'s exit code once `signal` has cancelled a run: the status the stopped command ended
/// with, as a shell reports it, or the signal's when Bento had to end the command itself.
fn cancelled_exit_code(signal: bento_task::Signal, status: Option<ExitStatus>) -> ExitCode {
    if let Some(status) = status {
        return error::shell_status(&status);
    }
    signals::exit_code(match signal {
        bento_task::Signal::Interrupt => Signal::Interrupt,
        bento_task::Signal::Terminate => Signal::Terminate,
    })
}

/// Cancelling a task stops its command's whole process group with the signal `bento` received.
fn task_signal(signal: Signal) -> bento_task::Signal {
    match signal {
        Signal::Interrupt => bento_task::Signal::Interrupt,
        Signal::Terminate => bento_task::Signal::Terminate,
    }
}
