use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

use bento_guestbook_host::launch::Mode;

/// Run the Remix guest book in-process, with entries stored in SQLite that Rust owns.
#[derive(Debug, Parser)]
#[command(name = "bento-guestbook-host", version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Serve the guest book's `server.embedded.ts` over the `host:guestbook` module
    Serve(ServeArguments),
    /// Print the saved entries straight from the Rust-owned store, without a runtime
    Entries(EntriesArguments),
}

#[derive(Debug, Args)]
pub struct ServeArguments {
    /// The guest book project: the fixture, or a copy with installed dependencies
    #[arg(long, value_name = "DIR")]
    pub project: PathBuf,
    /// The SQLite file the host owns, outside the project; created with its table when missing
    #[arg(long, value_name = "PATH")]
    pub database: PathBuf,
    /// The port the application listens on
    #[arg(long, value_name = "N", default_value_t = 1612)]
    pub port: u16,
    /// Refuse appends at the Rust boundary; listing still works and the database must exist
    #[arg(long)]
    pub read_only: bool,
    /// The NODE_ENV the guest sees
    #[arg(long, value_enum, default_value_t = Mode::Production)]
    pub mode: Mode,
}

#[derive(Debug, Args)]
pub struct EntriesArguments {
    /// The SQLite file the host owns
    #[arg(long, value_name = "PATH")]
    pub database: PathBuf,
}
