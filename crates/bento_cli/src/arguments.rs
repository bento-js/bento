use clap::{Args, Parser, Subcommand};

use crate::grant_flags::GrantFlags;

/// Run TypeScript applications and project tasks.
#[derive(Debug, Parser)]
#[command(name = "bento", version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Run a script, or a task from the `tasks` field of package.json
    Run(RunArguments),
    /// Install dependencies and devDependencies with the project's pnpm
    Install(InstallArguments),
}

#[derive(Debug, Args)]
pub struct RunArguments {
    /// Restart the script whenever its source changes
    #[arg(long)]
    pub watch: bool,
    /// The script path or task name, then its arguments; task arguments go after `--`
    #[arg(value_name = "TARGET", required = true, num_args = 1.., trailing_var_arg = true)]
    target_and_arguments: Vec<String>,
    #[command(flatten)]
    pub grants: GrantFlags,
}

impl RunArguments {
    pub fn target(&self) -> &str {
        &self.target_and_arguments[0]
    }

    /// Every word after the target, exactly as given, including a leading `--`.
    pub fn arguments(&self) -> &[String] {
        &self.target_and_arguments[1..]
    }
}

#[derive(Debug, Args)]
pub struct InstallArguments {
    /// Install exactly the locked graph, failing if package.json and the lockfile disagree
    #[arg(long)]
    pub frozen_lockfile: bool,
}
