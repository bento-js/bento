use std::collections::BTreeMap;
use std::io;
use std::os::unix::fs::symlink;
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};
use std::time::Duration;

use tempfile::TempDir;
use thiserror::Error;

use crate::assignments::assigned_names;
use crate::cancellation::{Cancellation, Signal};
use crate::plan::{Plan, Step};
use crate::process_group::{Outcome, ProcessGroup};
use bento_host::{CEILING_ENV, Grants};
use bento_workspace::{FILE_KEYS_ENV, LaunchEnvironment, Project};

/// How long a command's process group may take to exit after a forwarded cancellation
/// signal, or after the termination signal sent to processes the command left running,
/// before it is killed. Longer than a nested runtime's five-second shutdown.
pub const DEFAULT_GRACE_PERIOD: Duration = Duration::from_secs(10);

/// Runs planned task commands as a trusted host operation: `/bin/sh -c` in the project root,
/// with the launch environment prepared for tools and nested `bento` launches.
#[derive(Debug)]
pub struct TaskRunner {
    root: PathBuf,
    launch: LaunchEnvironment,
    environment: BTreeMap<String, String>,
    grace_period: Duration,
    /// Holds only a `bento` link to the running executable and leads `PATH`, so that a
    /// project's own `node_modules/.bin/bento` cannot stand in for it. Removed on drop.
    _bento_link: TempDir,
}

#[derive(Debug, Error)]
pub enum TaskError {
    #[error("cannot add {} to PATH because it is not UTF-8 or contains `:`", .0.display())]
    UnrepresentablePath(PathBuf),
    #[error("cannot put the running bento on task commands' PATH: {0}")]
    BentoLink(#[source] io::Error),
    #[error("task `{task}` could not run `{command}`: {source}")]
    Process { task: String, command: String, source: io::Error },
    #[error("task `{task}` failed: `{command}` {}", describe(.status))]
    Failed { task: String, command: String, status: ExitStatus },
    #[error("task `{task}` was cancelled by {signal}")]
    Cancelled { task: String, signal: Signal },
}

impl TaskError {
    /// The failing command's exit code, when it exited rather than being killed.
    pub fn exit_code(&self) -> Option<i32> {
        match self {
            TaskError::Failed { status, .. } => status.code(),
            _ => None,
        }
    }
}

impl TaskRunner {
    /// `executable` is the absolute path of the running `bento` executable, which task
    /// commands reach by the name `bento`.
    pub fn new(
        project: &Project,
        launch: &LaunchEnvironment,
        grants: &Grants,
        executable: &Path,
    ) -> Result<TaskRunner, TaskError> {
        let bento_link = link_bento(executable).map_err(TaskError::BentoLink)?;
        let mut environment = launch.vars().clone();
        let search_path =
            search_path(project.root(), bento_link.path(), executable, environment.get("PATH"))?;
        environment.insert("PATH".to_owned(), search_path);
        environment.insert(CEILING_ENV.to_owned(), grants.to_ceiling_value());
        Ok(TaskRunner {
            root: project.root().to_owned(),
            launch: launch.clone(),
            environment,
            grace_period: DEFAULT_GRACE_PERIOD,
            _bento_link: bento_link,
        })
    }

    pub fn grace_period(self, grace_period: Duration) -> TaskRunner {
        TaskRunner { grace_period, ..self }
    }

    /// Runs the plan's commands in order, stopping at the first failure or cancellation.
    pub fn run(&self, plan: &Plan, cancellation: &Cancellation) -> Result<(), TaskError> {
        plan.steps().iter().try_for_each(|step| self.run_step(step, cancellation))
    }

    fn run_step(&self, step: &Step, cancellation: &Cancellation) -> Result<(), TaskError> {
        let task = step.task.clone();
        if let Some(signal) = cancellation.requested() {
            return Err(TaskError::Cancelled { task, signal });
        }
        let process_error = |source| TaskError::Process {
            task: step.task.clone(),
            command: step.command.clone(),
            source,
        };
        let group = ProcessGroup::spawn(self.shell(&step.command)).map_err(process_error)?;
        match group.wait(cancellation, self.grace_period).map_err(process_error)? {
            Outcome::Exited(status) if status.success() => Ok(()),
            Outcome::Exited(status) => {
                Err(TaskError::Failed { task, command: step.command.clone(), status })
            }
            Outcome::Cancelled(signal) => Err(TaskError::Cancelled { task, signal }),
        }
    }

    fn shell(&self, command_line: &str) -> Command {
        let file_keys = self.launch.file_keys_value(&assigned_names(command_line));
        let mut command = Command::new("/bin/sh");
        command
            .arg("-c")
            .arg(command_line)
            .current_dir(&self.root)
            .env_clear()
            .envs(&self.environment)
            .env(FILE_KEYS_ENV, file_keys);
        command
    }
}

fn link_bento(executable: &Path) -> io::Result<TempDir> {
    let directory = tempfile::Builder::new().prefix("bento-").tempdir()?;
    symlink(executable, directory.path().join("bento"))?;
    Ok(directory)
}

fn search_path(
    root: &Path,
    bento_link: &Path,
    executable: &Path,
    inherited: Option<&String>,
) -> Result<String, TaskError> {
    let local_bin = root.join("node_modules").join(".bin");
    let executable_dir = executable.parent().expect("an executable path has a parent directory");
    let mut entries =
        vec![path_entry(bento_link)?, path_entry(&local_bin)?, path_entry(executable_dir)?];
    entries.extend(inherited.filter(|path| !path.is_empty()).map(String::as_str));
    Ok(entries.join(":"))
}

fn path_entry(path: &Path) -> Result<&str, TaskError> {
    path.to_str()
        .filter(|entry| !entry.contains(':'))
        .ok_or_else(|| TaskError::UnrepresentablePath(path.to_owned()))
}

fn describe(status: &ExitStatus) -> String {
    match (status.code(), status.signal()) {
        (Some(code), _) => format!("exited with code {code}"),
        (None, Some(signal)) => format!("was terminated by signal {signal}"),
        (None, None) => format!("ended with {status}"),
    }
}
