use std::env;

use crate::arguments::RunArguments;
use crate::error::CliError;
use crate::launch::Launch;
use crate::task;
use bento_workspace::{LaunchKind, Selection, Target, select_target};

/// `bento run`: selects the target, prepares its launch, and runs it.
pub fn run(arguments: &RunArguments) -> Result<(), CliError> {
    let invocation_directory = env::current_dir().map_err(CliError::CurrentDirectory)?;
    let Selection { project, target } = select_target(arguments.target(), &invocation_directory)?;
    match target {
        Target::Task(name) => {
            if arguments.watch {
                return Err(CliError::WatchedTask(name));
            }
            let task_arguments = task::arguments(&name, arguments.arguments())?;
            let launch = Launch::prepare(
                project,
                &arguments.grants,
                &invocation_directory,
                LaunchKind::Run,
            )?;
            task::run(&launch, &name, task_arguments)
        }
        Target::Script(_) => {
            let kind = if arguments.watch { LaunchKind::Watch } else { LaunchKind::Run };
            Launch::prepare(project, &arguments.grants, &invocation_directory, kind)?;
            Err(CliError::RuntimeUnavailable(arguments.target().to_owned()))
        }
    }
}
