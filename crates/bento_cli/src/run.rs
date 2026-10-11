use std::env;
use std::process::ExitCode;

use crate::arguments::RunArguments;
use crate::error::CliError;
use crate::launch::Launch;
use crate::script;
use crate::task;
use crate::watch;
use bento_workspace::{LaunchKind, Selection, Target, select_target};

/// `bento run`: selects the target, prepares its launch, and runs it.
pub fn run(arguments: &RunArguments) -> Result<ExitCode, CliError> {
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
            task::run(&launch, &name, task_arguments)?;
            Ok(ExitCode::SUCCESS)
        }
        Target::Script(entry) => {
            let kind = if arguments.watch { LaunchKind::Watch } else { LaunchKind::Run };
            let launch = Launch::prepare(project, &arguments.grants, &invocation_directory, kind)?;
            let script_arguments = arguments.arguments();
            if arguments.watch {
                watch::run(launch, &entry, &invocation_directory, script_arguments)
            } else {
                script::run(launch, &entry, &invocation_directory, script_arguments)
            }
        }
    }
}
