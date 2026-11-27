//! The `bento` command: runs scripts and project tasks, and installs dependencies.

mod arguments;
mod diagnostic;
mod ending;
mod error;
mod events;
mod install;
mod instance;
mod launch;
mod permission_flags;
mod process_environment;
mod run;
mod runtime_report;
mod script;
mod signals;
mod task;
mod watch;

use std::process::ExitCode;

use clap::Parser;

use crate::arguments::{Cli, Command};
use crate::error::CliError;

fn main() -> ExitCode {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(usage) => return usage_error(&usage),
    };
    let outcome = match cli.command {
        Command::Run(arguments) => run::run(&arguments),
        Command::Install(arguments) => install::install(&arguments).map(|()| ExitCode::SUCCESS),
    };
    match outcome {
        Ok(code) => code,
        Err(error) => {
            error.report();
            error.exit_code()
        }
    }
}

/// Reports a command line clap rejected, or the help or version it asked for. A value flag
/// without its value is reported in Node's words.
fn usage_error(usage: &clap::Error) -> ExitCode {
    if let Some(missing) = permission_flags::missing_argument(usage) {
        CliError::from(missing).report();
        return ExitCode::FAILURE;
    }
    let _ = usage.print();
    if usage.use_stderr() { ExitCode::FAILURE } else { ExitCode::SUCCESS }
}
