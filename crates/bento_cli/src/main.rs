//! The `bento` command: runs scripts and project tasks, and installs dependencies.

mod arguments;
mod error;
mod grant_flags;
mod install;
mod launch;
mod process_environment;
mod run;
mod script;
mod signals;
mod task;
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "`bento run --watch` drives it once the runtime can shut a script down"
    )
)]
mod watch;

use std::process::ExitCode;

use clap::Parser;

use crate::arguments::{Cli, Command};

fn main() -> ExitCode {
    let cli = match Cli::try_parse() {
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
            eprintln!("error: {error}");
            error.exit_code()
        }
    }
}
