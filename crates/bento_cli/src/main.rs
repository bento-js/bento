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

use std::ffi::OsString;
use std::process::ExitCode;

use clap::Parser;

use crate::arguments::{Cli, Command};
use crate::error::CliError;

fn main() -> ExitCode {
    let arguments: Vec<OsString> = std::env::args_os().collect();
    if let Some(missing) = permission_flags::missing_argument(&arguments) {
        CliError::from(missing).report();
        return ExitCode::FAILURE;
    }
    let cli = match Cli::try_parse_from(arguments) {
        Ok(cli) => cli,
        Err(usage) => {
            let _ = usage.print();
            return if usage.use_stderr() { ExitCode::FAILURE } else { ExitCode::SUCCESS };
        }
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
