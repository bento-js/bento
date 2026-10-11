use std::env;

use crate::arguments::InstallArguments;
use crate::error::CliError;
use crate::process_environment;
use bento_package::Lockfile;
use bento_workspace::Project;

/// `bento install`: installs the project around the current directory with its pnpm.
pub fn install(arguments: &InstallArguments) -> Result<(), CliError> {
    let directory = env::current_dir().map_err(CliError::CurrentDirectory)?;
    let project = Project::for_directory(&directory)?;
    let lockfile = if arguments.frozen_lockfile { Lockfile::Frozen } else { Lockfile::Update };
    bento_package::install(&project, lockfile, &process_environment::read()?)?;
    Ok(())
}
