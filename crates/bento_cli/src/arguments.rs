use std::path::Path;

use clap::{Args, Parser, Subcommand};

use crate::grant_flags::GrantFlags;

/// Run TypeScript and JavaScript applications.
#[derive(Debug, Parser)]
#[command(name = "bento", version)]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Command {
    /// Run a JavaScript or TypeScript source file
    Run(RunArguments),
    /// Install dependencies and devDependencies with the project's pnpm
    Install(InstallArguments),
}

#[derive(Debug, Args)]
pub(crate) struct RunArguments {
    /// Restart the script whenever its source changes
    #[arg(long)]
    pub watch: bool,
    /// The source file to run, then the arguments it receives
    #[arg(value_name = "SCRIPT", required = true, num_args = 1.., trailing_var_arg = true)]
    script_and_arguments: Vec<String>,
    #[command(flatten)]
    pub grants: GrantFlags,
}

impl RunArguments {
    /// The script's path, as given, relative to the directory `bento` runs in.
    pub(crate) fn script(&self) -> &Path {
        Path::new(&self.script_and_arguments[0])
    }

    /// Every word after the script, exactly as given, including a leading `--`.
    pub(crate) fn arguments(&self) -> &[String] {
        &self.script_and_arguments[1..]
    }
}

#[derive(Debug, Args)]
pub(crate) struct InstallArguments {
    /// Install exactly the locked graph, failing if package.json and the lockfile disagree
    #[arg(long)]
    pub frozen_lockfile: bool,
}
