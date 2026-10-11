//! `bento-guestbook-host`: serves the Remix guest book in-process over Rust-owned storage, and
//! inspects that storage without a runtime.

mod arguments;
mod entries;
mod error;

use std::process::ExitCode;

use clap::Parser;

// The Node-API symbols the build script exports are defined in the runtime's dependency graph,
// so the executable links it even before `serve` drives it.
use bento_runtime as _;

use crate::arguments::{Cli, Command};
use crate::error::CommandError;

fn main() -> ExitCode {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(usage) => {
            let _ = usage.print();
            return if usage.use_stderr() { ExitCode::from(2) } else { ExitCode::SUCCESS };
        }
    };
    let outcome = match cli.command {
        Command::Serve(arguments) => Err(CommandError::RuntimeUnavailable(arguments.project)),
        Command::Entries(arguments) => entries::entries(&arguments),
    };
    match outcome {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}
