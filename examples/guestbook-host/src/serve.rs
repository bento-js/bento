//! `serve`: runs the project's embedded entrypoint over `host:guestbook` until it finishes or
//! the host is told to stop, and prints how it ended.

use std::process::ExitCode;

use bento_guestbook_host::launch::Launch;
use bento_guestbook_host::lifecycle;
use bento_guestbook_host::store::Access;
use bento_runtime::{RunOutcome, ShutdownOutcome};

use crate::arguments::ServeArguments;
use crate::error::CommandError;

const ENTRY: &str = "server.embedded.ts";

pub fn serve(arguments: &ServeArguments) -> Result<ExitCode, CommandError> {
    let launch =
        Launch::prepare(&arguments.project, &arguments.database, arguments.port, arguments.mode)?;
    let access = if arguments.read_only { Access::ReadOnly } else { Access::ReadWrite };
    let outcome = lifecycle::serve(&launch, access, &launch.project().join(ENTRY))?;
    conclude(outcome)
}

fn conclude(outcome: RunOutcome) -> Result<ExitCode, CommandError> {
    match outcome {
        RunOutcome::Completed => Ok(ExitCode::SUCCESS),
        RunOutcome::Exited(code) => Ok(exit_code(code)),
        RunOutcome::Failed(error) => Err(CommandError::Guest(error)),
        RunOutcome::Shutdown(report) => {
            println!("shutdown: {}", describe(&report.outcome));
            Ok(match report.outcome {
                ShutdownOutcome::Graceful { exit_code: code } => exit_code(code),
                _ => ExitCode::FAILURE,
            })
        }
    }
}

fn describe(outcome: &ShutdownOutcome) -> String {
    match outcome {
        ShutdownOutcome::Graceful { exit_code } => format!("graceful (exit code {exit_code})"),
        ShutdownOutcome::Interrupted => "interrupted".to_owned(),
        ShutdownOutcome::TimedOut => "timed out".to_owned(),
        ShutdownOutcome::Failed(error) => format!("failed: {error}"),
    }
}

fn exit_code(code: i32) -> ExitCode {
    u8::try_from(code).map_or(ExitCode::FAILURE, ExitCode::from)
}
