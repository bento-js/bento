use std::io;
use std::os::unix::process::ExitStatusExt;
use std::process::{ExitCode, ExitStatus};

use thiserror::Error;

use crate::grant_flags::GrantError;
use bento_package::InstallError;
use bento_task::{PlanError, Signal, TaskError};
use bento_workspace::{EnvironmentError, ProjectError, TargetError};

#[derive(Debug, Error)]
pub enum CliError {
    #[error(transparent)]
    Target(#[from] TargetError),
    #[error(transparent)]
    Project(#[from] ProjectError),
    #[error(transparent)]
    Grant(#[from] GrantError),
    #[error(transparent)]
    Environment(#[from] EnvironmentError),
    #[error(transparent)]
    Plan(#[from] PlanError),
    #[error(transparent)]
    Task(#[from] TaskError),
    #[error(transparent)]
    Install(#[from] InstallError),
    #[error("environment variable `{0}` is not valid UTF-8")]
    NonUnicodeVariable(String),
    #[error("cannot determine the current directory: {0}")]
    CurrentDirectory(#[source] io::Error),
    #[error("cannot locate the running bento executable: {0}")]
    Executable(#[source] io::Error),
    #[error("cannot listen for interrupt and termination signals: {0}")]
    Signals(#[source] io::Error),
    #[error(
        "task `{task}` takes arguments only after `--`, as in `bento run {task} -- <arguments>`; bento options go before the task name"
    )]
    TaskArgumentsWithoutSeparator { task: String },
    #[error(
        "--watch restarts a script when its source changes, so it cannot run task `{0}`; give the task a command that runs `bento run --watch <script>`"
    )]
    WatchedTask(String),
    #[error(
        "cannot run `{0}`: script execution requires the runtime, which this build of bento does not include"
    )]
    RuntimeUnavailable(String),
}

impl CliError {
    /// `130` or `143` for a run stopped by SIGINT or SIGTERM, the failing child's status
    /// when a task command or pnpm failed, and `1` for everything else.
    pub fn exit_code(&self) -> ExitCode {
        match self {
            CliError::Task(TaskError::Cancelled { signal: Signal::Interrupt, .. }) => {
                ExitCode::from(130)
            }
            CliError::Task(TaskError::Cancelled { signal: Signal::Terminate, .. }) => {
                ExitCode::from(143)
            }
            CliError::Task(TaskError::Failed { status, .. }) => shell_status(status),
            CliError::Install(InstallError::Failed(status)) => shell_status(status),
            _ => ExitCode::FAILURE,
        }
    }
}

/// A child's status as a shell reports it: its exit code, or 128 plus the signal that ended it.
fn shell_status(status: &ExitStatus) -> ExitCode {
    let code = status.code().or_else(|| status.signal().map(|signal| 128 + signal));
    code.and_then(|code| u8::try_from(code).ok()).map_or(ExitCode::FAILURE, ExitCode::from)
}
