use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::error::CliError;
use crate::permission_flags::{PermissionError, PermissionFlags};
use crate::process_environment;
use bento_host::{
    CEILING_ENV, PathPattern, Permission, PermissionSet, Permissions, canonical_path,
};
use bento_workspace::{LaunchEnvironment, LaunchKind, Project};

/// What a launch settles before it runs anything, whether it runs a script or a task.
pub(crate) struct Launch {
    pub project: Project,
    pub permissions: Permissions,
    pub environment: LaunchEnvironment,
}

impl Launch {
    /// Resolves `flags` within the ceiling an invoking launch handed down, then loads the
    /// project's environment files beneath the environment `bento` was started with. `entry`
    /// is the script the launch runs, which a restricted launch may read without a flag; a
    /// task launch has none.
    pub(crate) fn prepare(
        project: Project,
        flags: &PermissionFlags,
        invocation_directory: &Path,
        kind: LaunchKind,
        entry: Option<&Path>,
    ) -> Result<Launch, CliError> {
        let requested = flags.requested(invocation_directory)?;
        for warning in flags.comma_warnings(entry.is_some()) {
            eprintln!("warning: {warning}");
        }
        let mut inherited = process_environment::read()?;
        let ceiling = take_ceiling(&mut inherited)?;
        let permissions = match requested {
            None => ceiling.unwrap_or_else(Permissions::unrestricted),
            Some(allowed) => restricted(allowed, ceiling.as_ref(), entry)?,
        };
        let environment = LaunchEnvironment::load(project.root(), inherited, kind)?;
        Ok(Launch { project, permissions, environment })
    }
}

/// Removes the permissions an invoking launch handed down from `environment` and returns them.
fn take_ceiling(
    environment: &mut BTreeMap<String, String>,
) -> Result<Option<Permissions>, PermissionError> {
    let ceiling = environment.remove(CEILING_ENV);
    Ok(ceiling.map(|value| Permissions::from_ceiling_value(&value)).transpose()?)
}

/// The permissions of a launch restricted to `allowed` under `ceiling`, which must hold all of
/// them. The launch may also read `entry`, the script it runs, unless its ceiling may not.
fn restricted(
    allowed: PermissionSet,
    ceiling: Option<&Permissions>,
    entry: Option<&Path>,
) -> Result<Permissions, PermissionError> {
    if let Some(ceiling) = ceiling
        && let Some(excess) = ceiling.excess(&allowed)
    {
        let limit = ceiling.describe(excess.permission);
        return Err(PermissionError::Excess { excess, limit });
    }
    let permissions = Permissions::restricted(allowed);
    let Some(entry) = entry else {
        return Ok(permissions);
    };
    let entry = canonical_path(entry)
        .map_err(|source| PermissionError::Entry { path: entry.to_owned(), source })?;
    if ceiling.is_none_or(|ceiling| ceiling.holds_path(Permission::FsRead, &entry)) {
        return Ok(permissions.reading(PathPattern::Exact(entry)));
    }
    Ok(permissions)
}

/// The environment a runtime instance of `environment`'s launch starts with under
/// `permissions`, and the names in it that the instance keeps whatever they say: the values
/// Bento prepared, as Node keeps its `--env-file` values. Where an inherited value overrides a
/// prepared one the permissions would remove, the prepared value takes its place again, so
/// that only the inherited value is removed.
pub(crate) fn guest_environment(
    environment: LaunchEnvironment,
    permissions: &Permissions,
) -> (BTreeMap<String, String>, BTreeSet<String>) {
    let prepared = environment.prepared_values();
    let kept: BTreeSet<String> = prepared.keys().cloned().collect();
    let restored: Vec<(String, String)> = prepared
        .iter()
        .filter(|(name, _)| permissions.removes_variable(name))
        .map(|(name, value)| (name.clone(), value.clone()))
        .collect();
    let mut vars = environment.into_vars();
    vars.extend(restored);
    (vars, kept)
}
