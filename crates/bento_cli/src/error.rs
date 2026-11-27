use std::io::{self, IsTerminal};
use std::os::unix::process::ExitStatusExt;
use std::path::PathBuf;
use std::process::{ExitCode, ExitStatus};

use thiserror::Error;

use crate::diagnostic::{self, Diagnostic};
use crate::grant_flags::GrantError;
use crate::runtime_report;
use bento_package::InstallError;
use bento_runtime::RuntimeError;
use bento_workspace::{EnvironmentError, ManifestError, ProjectError};

#[derive(Debug, Error)]
pub(crate) enum CliError {
    #[error(transparent)]
    Project(#[from] ProjectError),
    #[error(transparent)]
    Grant(#[from] GrantError),
    #[error(transparent)]
    Environment(#[from] EnvironmentError),
    #[error(transparent)]
    Install(#[from] InstallError),
    #[error("environment variable `{0}` is not valid UTF-8")]
    NonUnicodeVariable(String),
    #[error("cannot determine the current directory: {0}")]
    CurrentDirectory(#[source] io::Error),
    #[error("cannot listen for interrupt and termination signals: {0}")]
    Signals(#[source] io::Error),
    #[error("{}: {}", runtime_report::describe(.0.kind), runtime_report::plain_text(&.0.message))]
    Script(RuntimeError),
    #[error(
        "work the script started did not finish within the shutdown deadline; disposal is incomplete"
    )]
    IncompleteDisposal,
    #[error("cannot watch {} for changes: {source}", .path.display())]
    Watch { path: PathBuf, source: notify::Error },
}

impl CliError {
    /// Writes this error to standard error: as a report showing the place in a file it points
    /// at, when it has one Bento can show, and otherwise as one line.
    pub(crate) fn report(&self) {
        let color = io::stderr().is_terminal();
        let shown = self.diagnostic().and_then(|diagnostic| diagnostic::render(&diagnostic, color));
        match shown {
            Some(report) => eprint!("{report}"),
            None => eprintln!("error: {self}"),
        }
    }

    /// The place in a source file this error points at, if it points at one.
    fn diagnostic(&self) -> Option<Diagnostic<'_>> {
        match self {
            CliError::Project(ProjectError::Manifest(error)) => manifest_diagnostic(error),
            CliError::Script(error) => runtime_report::diagnostic(error),
            _ => None,
        }
    }

    /// pnpm's status when it failed, and `1` for everything else.
    pub(crate) fn exit_code(&self) -> ExitCode {
        match self {
            CliError::Install(InstallError::Failed(status)) => shell_status(status),
            _ => ExitCode::FAILURE,
        }
    }
}

/// A child's status as a shell reports it: its exit code, or 128 plus the signal that ended it.
/// `FAILURE` stands in for a status that reports neither.
fn shell_status(status: &ExitStatus) -> ExitCode {
    let code = status.code().or_else(|| status.signal().map(|signal| 128 + signal));
    code.map_or(ExitCode::FAILURE, process_exit_code)
}

/// The status the operating system reports for exit code `code`: its low eight bits, as for
/// any process.
pub(crate) fn process_exit_code(code: i32) -> ExitCode {
    ExitCode::from(code.to_le_bytes()[0])
}

fn manifest_diagnostic(error: &ManifestError) -> Option<Diagnostic<'_>> {
    let ManifestError::Syntax { path, source } = error else {
        return None;
    };
    // serde_json describes the problem and then its position, which the report shows itself.
    let position = format!(" at line {} column {}", source.line(), source.column());
    let described = source.to_string();
    Some(Diagnostic {
        message: format!("{} is not valid JSON", path.display()),
        path,
        line: source.line(),
        column: source.column(),
        label: described.strip_suffix(&position).unwrap_or(&described).to_owned(),
        trace: String::new(),
    })
}
