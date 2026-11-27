use std::path::Path;

use crate::error::CliError;
use crate::grant_flags::{GrantFlags, take_ceiling};
use crate::process_environment;
use bento_host::Grants;
use bento_workspace::{LaunchEnvironment, LaunchKind, Project};

/// What a launch settles before it runs its script.
pub(crate) struct Launch {
    pub project: Project,
    pub grants: Grants,
    pub environment: LaunchEnvironment,
}

impl Launch {
    /// Resolves `flags` against the inherited ceiling, then finds the project enclosing
    /// `script` and loads its environment files beneath the environment `bento` was started
    /// with. The grants come first, so malformed or too-wide flags fail before anything looks
    /// at the script or its project.
    pub(crate) fn prepare(
        script: &Path,
        flags: &GrantFlags,
        invocation_directory: &Path,
        kind: LaunchKind,
    ) -> Result<Launch, CliError> {
        let mut inherited = process_environment::read()?;
        let ceiling = take_ceiling(&mut inherited)?;
        let grants = flags.resolve(invocation_directory, ceiling.as_ref())?;
        let project = Project::for_script(script)?;
        let environment = LaunchEnvironment::load(project.root(), inherited, kind)?;
        Ok(Launch { project, grants, environment })
    }
}
