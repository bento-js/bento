//! Dependency installation through the project's declared pnpm toolchain.
//!
//! Bento does not resolve or write lockfiles itself: it checks that the project declares
//! pnpm and runs the `pnpm` the invoking shell would find on `PATH`, in the project root, so
//! pnpm's own tool-manager lock selects the pinned version and `pnpm-workspace.yaml` policy
//! applies unchanged.

use std::collections::BTreeMap;
use std::env;
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};

use thiserror::Error;

use bento_workspace::Project;

const PNPM: &str = "pnpm";

/// Whether installation may update `pnpm-lock.yaml`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lockfile {
    /// pnpm's normal behavior, including its own frozen default under `CI`.
    Update,
    /// Reproduce the locked graph exactly; a manifest/lock mismatch fails.
    Frozen,
}

#[derive(Debug, Error)]
pub enum InstallError {
    #[error("{}", describe_unsupported(.manifest, .declared.as_deref()))]
    UnsupportedPackageManager { manifest: PathBuf, declared: Option<String> },
    #[error(
        "pnpm is required to install dependencies but was not found on PATH; install pnpm 12 (https://pnpm.io/installation) and run `bento install` again"
    )]
    MissingPnpm,
    #[error("could not start pnpm: {0}")]
    Spawn(#[source] io::Error),
    #[error("pnpm install failed: {}", describe_status(.0))]
    Failed(ExitStatus),
}

impl InstallError {
    /// pnpm's exit code, when installation ran and failed.
    pub fn exit_code(&self) -> Option<i32> {
        match self {
            InstallError::Failed(status) => status.code(),
            _ => None,
        }
    }
}

/// Installs `dependencies` and `devDependencies` with pnpm from the project root.
///
/// `environment` is the complete environment pnpm runs with, including the `PATH`
/// searched for it. That search resolves relative and empty `PATH` entries against
/// `invocation_directory`, the absolute directory Bento was started in, as the shell that
/// started it would; pnpm itself still runs in the project root. Bento adds
/// `--production=false` so an inherited `NODE_ENV=production` cannot drop development
/// dependencies, and nothing that approves dependency build scripts.
pub fn install(
    project: &Project,
    lockfile: Lockfile,
    environment: &BTreeMap<String, String>,
    invocation_directory: &Path,
) -> Result<(), InstallError> {
    require_pnpm_declaration(project)?;
    let pnpm =
        find_executable(PNPM, environment.get("PATH").map(String::as_str), invocation_directory)
            .ok_or(InstallError::MissingPnpm)?;
    let mut command = Command::new(pnpm);
    command
        .arg("install")
        .arg("--production=false")
        .current_dir(project.root())
        .env_clear()
        .envs(environment);
    if lockfile == Lockfile::Frozen {
        command.arg("--frozen-lockfile");
    }
    let status = command.status().map_err(InstallError::Spawn)?;
    if status.success() { Ok(()) } else { Err(InstallError::Failed(status)) }
}

fn require_pnpm_declaration(project: &Project) -> Result<(), InstallError> {
    let declared = project.manifest().package_manager.as_ref().map(|manager| manager.name.as_str());
    if declared == Some(PNPM) {
        return Ok(());
    }
    Err(InstallError::UnsupportedPackageManager {
        manifest: project.root().join("package.json"),
        declared: declared.map(str::to_owned),
    })
}

/// The first executable named `program` on `search_path`, as a shell finds it: an absolute
/// entry stands alone, a relative entry is resolved against `working_directory`, and an
/// empty entry (a leading, trailing, or doubled colon) is `working_directory` itself.
fn find_executable(
    program: &str,
    search_path: Option<&str>,
    working_directory: &Path,
) -> Option<PathBuf> {
    env::split_paths(search_path?)
        .map(|entry| working_directory.join(entry).join(program))
        .find(|candidate| is_executable(candidate))
}

fn is_executable(path: &Path) -> bool {
    path.metadata()
        .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
}

fn describe_unsupported(manifest: &Path, declared: Option<&str>) -> String {
    let manifest = manifest.display();
    match declared {
        Some(name) => format!(
            "{manifest} declares `{name}` in devEngines.packageManager, but `bento install` supports only pnpm; set its name to \"pnpm\""
        ),
        None => format!(
            "{manifest} has no devEngines.packageManager; `bento install` needs \"devEngines\": {{ \"packageManager\": {{ \"name\": \"pnpm\", \"version\": \"12\" }} }}"
        ),
    }
}

fn describe_status(status: &ExitStatus) -> String {
    match (status.code(), status.signal()) {
        (Some(code), _) => format!("exited with code {code}"),
        (None, Some(signal)) => format!("terminated by signal {signal}"),
        (None, None) => format!("ended with {status}"),
    }
}
