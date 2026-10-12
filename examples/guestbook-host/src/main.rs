//! `bento-guestbook-host`: serves the Remix guest book in-process over Rust-owned storage, and
//! inspects that storage without a runtime.

mod arguments;
mod entries;
mod error;
mod serve;

use std::process::ExitCode;

use clap::Parser;

use crate::arguments::{Cli, Command};

fn main() -> ExitCode {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(usage) => {
            let _ = usage.print();
            return if usage.use_stderr() { ExitCode::from(2) } else { ExitCode::SUCCESS };
        }
    };
    let outcome = match cli.command {
        Command::Serve(arguments) => serve::serve(&arguments),
        Command::Entries(arguments) => entries::entries(&arguments),
    };
    outcome.unwrap_or_else(|error| {
        eprintln!("error: {error}");
        ExitCode::FAILURE
    })
}
