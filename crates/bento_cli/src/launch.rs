use std::path::Path;

use crate::error::CliError;
use crate::grant_flags::{GrantFlags, take_ceiling};
use crate::process_environment;
use bento_host::Grants;
use bento_workspace::{LaunchEnvironment, LaunchKind, Project};

/// What a launch settles before it runs anything, whether it runs a script or a task.
pub(crate) struct Launch {
    pub project: Project,
    pub grants: Grants,
    pub environment: LaunchEnvironment,
}

impl Launch {
    /// Resolves `flags` against the inherited ceiling, then loads the project's environment
    /// files beneath the environment `bento` was started with.
    pub(crate) fn prepare(
        project: Project,
        flags: &GrantFlags,
        invocation_directory: &Path,
        kind: LaunchKind,
    ) -> Result<Launch, CliError> {
        let mut inherited = process_environment::read()?;
        let ceiling = take_ceiling(&mut inherited)?;
        let grants = flags.resolve(invocation_directory, ceiling.as_ref())?;
        let environment = LaunchEnvironment::load(project.root(), inherited, kind)?;
        Ok(Launch { project, grants, environment })
    }
}
