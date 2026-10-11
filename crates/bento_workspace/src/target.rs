use std::path::{MAIN_SEPARATOR, Path, PathBuf};

use thiserror::Error;

use crate::project::{Project, ProjectError};

/// What `bento run <target>` launches.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    /// A source file, as an absolute path.
    Script(PathBuf),
    /// A top-level task of the selected project.
    Task(String),
}

/// A launch target together with the project it belongs to.
#[derive(Clone, Debug)]
pub struct Selection {
    pub project: Project,
    pub target: Target,
}

#[derive(Debug, Error)]
pub enum TargetError {
    #[error("{}", describe_unknown(.argument, .manifest.as_deref()))]
    Unknown { argument: String, manifest: Option<PathBuf> },
    #[error(transparent)]
    Project(#[from] ProjectError),
}

/// Decides whether `argument`, given in `cwd`, names a script or a task.
///
/// An argument containing a path separator, or with an extension naming an existing
/// file, is a script. Otherwise an exact task name of the project around `cwd` wins
/// over a same-named file. Anything else is an error, never a shell command.
pub fn select_target(argument: &str, cwd: &Path) -> Result<Selection, TargetError> {
    let candidate = cwd.join(argument);
    if is_explicit_path(argument, &candidate) {
        return script(candidate);
    }
    match project_around(cwd)? {
        Some(project) if project.manifest().tasks.contains_key(argument) => {
            Ok(Selection { project, target: Target::Task(argument.to_owned()) })
        }
        _ if candidate.is_file() => script(candidate),
        project => Err(TargetError::Unknown {
            argument: argument.to_owned(),
            manifest: project.map(|project| project.root().join("package.json")),
        }),
    }
}

fn is_explicit_path(argument: &str, candidate: &Path) -> bool {
    argument.contains(MAIN_SEPARATOR)
        || (Path::new(argument).extension().is_some() && candidate.is_file())
}

fn script(path: PathBuf) -> Result<Selection, TargetError> {
    let project = Project::for_script(&path)?;
    Ok(Selection { project, target: Target::Script(path) })
}

fn project_around(cwd: &Path) -> Result<Option<Project>, ProjectError> {
    match Project::for_directory(cwd) {
        Ok(project) => Ok(Some(project)),
        Err(ProjectError::NotFound(_)) => Ok(None),
        Err(error) => Err(error),
    }
}

fn describe_unknown(argument: &str, manifest: Option<&Path>) -> String {
    match manifest {
        Some(manifest) => {
            format!("`{argument}` is neither a task in {} nor a file", manifest.display())
        }
        None => format!("`{argument}` is not a file, and no package.json declares it as a task"),
    }
}
