//! `serve`: runs the project's embedded entrypoint over `host:guestbook` until it finishes or
//! the host is told to stop, and prints how it ended.

use std::os::raw::c_int;
use std::process::ExitCode;

use signal_hook::low_level::signal_name;

use bento_guestbook_host::launch::Launch;
use bento_guestbook_host::lifecycle::{self, Served};
use bento_guestbook_host::store::Access;
use bento_runtime::{RunOutcome, ShutdownOutcome};

use crate::arguments::ServeArguments;
use crate::error::CommandError;

const ENTRY: &str = "server.embedded.ts";

pub(crate) fn serve(arguments: &ServeArguments) -> Result<ExitCode, CommandError> {
    let launch =
        Launch::prepare(&arguments.project, &arguments.database, arguments.port, arguments.mode)?;
    let access = if arguments.read_only { Access::ReadOnly } else { Access::ReadWrite };
    let served = lifecycle::serve(&launch, access, &launch.project().join(ENTRY))?;
    conclude(served)
}

fn conclude(served: Served) -> Result<ExitCode, CommandError> {
    match served.outcome {
        RunOutcome::Completed => Ok(ExitCode::SUCCESS),
        RunOutcome::Exited(code) => Ok(exit_code(code)),
        RunOutcome::Failed(error) => Err(CommandError::Guest(error)),
        RunOutcome::Shutdown(report) => {
            println!("shutdown: {}", describe(&report.outcome, served.signal));
            Ok(shutdown_status(&report.outcome, served.signal))
        }
    }
}

/// A guest that exited itself during the shutdown decides the status. One the runtime closed
/// on the signal's behalf reports as a process ended by that signal would (130 after Ctrl-C,
/// 143 after `SIGTERM`), as `bento` does, rather than claiming a guest exit that never happened.
fn shutdown_status(outcome: &ShutdownOutcome, signal: Option<c_int>) -> ExitCode {
    match outcome {
        ShutdownOutcome::Graceful { exit_code: Some(code) } => exit_code(*code),
        ShutdownOutcome::Graceful { exit_code: None } => {
            signal.map_or(ExitCode::SUCCESS, |signal| exit_code(128 + signal))
        }
        _ => ExitCode::FAILURE,
    }
}

fn describe(outcome: &ShutdownOutcome, signal: Option<c_int>) -> String {
    match outcome {
        ShutdownOutcome::Graceful { exit_code: Some(code) } => {
            format!("graceful (exit code {code})")
        }
        ShutdownOutcome::Graceful { exit_code: None } => match signal.and_then(signal_name) {
            Some(signal) => format!("graceful (closed by the host on {signal})"),
            None => "graceful (closed by the host)".to_owned(),
        },
        ShutdownOutcome::Interrupted => "interrupted".to_owned(),
        ShutdownOutcome::TimedOut => "timed out".to_owned(),
        ShutdownOutcome::Failed(error) => format!("failed: {error}"),
    }
}

fn exit_code(code: i32) -> ExitCode {
    u8::try_from(code).map_or(ExitCode::FAILURE, ExitCode::from)
}
