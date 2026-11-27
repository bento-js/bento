use std::env;
use std::process::ExitCode;

use crate::arguments::RunArguments;
use crate::error::CliError;
use crate::launch::Launch;
use crate::script;
use crate::watch;
use bento_workspace::LaunchKind;

/// `bento run`: prepares the launch of the script, then runs it once or watches it.
pub(crate) fn run(arguments: &RunArguments) -> Result<ExitCode, CliError> {
    let invocation_directory = env::current_dir().map_err(CliError::CurrentDirectory)?;
    let entry = invocation_directory.join(arguments.script());
    let kind = if arguments.watch { LaunchKind::Watch } else { LaunchKind::Run };
    let launch = Launch::prepare(&entry, &arguments.grants, &invocation_directory, kind)?;
    if arguments.watch {
        watch::run(launch, &entry, &invocation_directory, arguments.arguments())
    } else {
        script::run(launch, &entry, &invocation_directory, arguments.arguments())
    }
}
