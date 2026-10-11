use std::path::Path;
use std::process::ExitCode;

use bento_runtime::{RunOutcome, RuntimeConfig};

use crate::error::CliError;
use crate::launch::Launch;

/// Runs the script at `entry` in a runtime instance until it finishes, and returns the exit
/// code it ended with.
pub fn run(
    launch: Launch,
    entry: &Path,
    invocation_directory: &Path,
    arguments: &[String],
) -> Result<ExitCode, CliError> {
    let config = RuntimeConfig {
        project_root: launch.project.root().to_owned(),
        cwd: invocation_directory.to_owned(),
        grants: launch.grants,
        env: launch.environment.vars().clone(),
        argv: arguments.to_vec(),
    };
    let runtime = bento_runtime::start(config, entry).map_err(CliError::Script)?;
    match runtime.wait() {
        RunOutcome::Completed => Ok(ExitCode::SUCCESS),
        RunOutcome::Exited(code) => Ok(exit_code(code)),
        RunOutcome::Failed(error) => Err(CliError::Script(error)),
    }
}

/// The status the operating system reports for `process.exit(code)`: the code's low eight
/// bits, as for any process.
fn exit_code(code: i32) -> ExitCode {
    ExitCode::from(code as u8)
}
