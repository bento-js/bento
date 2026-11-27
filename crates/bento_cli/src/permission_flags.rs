use std::collections::BTreeSet;
use std::io;
use std::path::{Path, PathBuf};

use clap::error::{ContextKind, ContextValue, ErrorKind};
use clap::{ArgAction, Args};
use thiserror::Error;

use bento_host::{
    CEILING_ENV, EnvPattern, Excess, InvalidCeiling, PathPattern, Permission, PermissionSet, Scope,
};

/// The permission options of one launch, spelled and read as Node's. Without `--permission`
/// they ask for nothing, and no `--allow-*` flag may be given. Node accepts each flag more
/// than once, so a repeated boolean flag is not an error.
#[derive(Debug, Args)]
#[command(next_help_heading = "Permissions", args_override_self = true)]
pub(crate) struct PermissionFlags {
    /// Restrict the launch to what its --allow-* flags allow
    #[arg(long)]
    permission: bool,
    /// Allow reading PATH: `*` for every path, an existing directory for everything beneath
    /// it, or text ending in `*` for every path that begins with that text
    #[arg(long, value_name = "PATH", action = ArgAction::Append)]
    allow_fs_read: Vec<String>,
    /// Allow writing PATH, which is read as for --allow-fs-read
    #[arg(long, value_name = "PATH", action = ArgAction::Append)]
    allow_fs_write: Vec<String>,
    /// Keep the inherited environment variables NAME covers: `*`, a name, or a name prefix
    /// followed by `*`, comma-separated
    #[arg(long, value_name = "NAME", action = ArgAction::Append)]
    allow_env: Vec<String>,
    /// Allow connecting to and listening on the network
    #[arg(long)]
    allow_net: bool,
    /// Allow starting child processes. A child runs with your full authority, outside every
    /// permission
    #[arg(long)]
    allow_child_process: bool,
    /// Allow starting worker threads
    #[arg(long)]
    allow_worker: bool,
    /// Allow loading native addons. An addon runs native code in this process, outside every
    /// permission
    #[arg(long)]
    allow_addons: bool,
    /// Allow using WASI
    #[arg(long)]
    allow_wasi: bool,
    /// Allow using FFI
    #[arg(long)]
    allow_ffi: bool,
    /// Allow using the inspector
    #[arg(long)]
    allow_inspector: bool,
    /// Allow using OpenSSL STORE loaders
    #[arg(long)]
    allow_openssl_store: bool,
    /// Allow mounting a virtual file system
    #[arg(long)]
    allow_fs_vfs: bool,
    /// Allow using the local language model
    #[arg(long)]
    allow_language_model: bool,
}

#[derive(Debug, Error)]
pub(crate) enum PermissionError {
    #[error("--permission is required")]
    PermissionRequired,
    /// `flag` as given: the flag alone without a value, or followed by `=` before an empty one.
    #[error("{flag} requires an argument")]
    MissingArgument { flag: String },
    #[error("--allow-env must be '*', a variable name, or a variable name prefix followed by '*'")]
    InvalidVariable,
    #[error("cannot resolve {flag}={value}: {source}")]
    Path { flag: &'static str, value: String, source: io::Error },
    #[error("cannot resolve the script {}: {source}", .path.display())]
    Entry { path: PathBuf, source: io::Error },
    #[error("{} asks for more than the inherited {} permission: {limit}", requested_flag(.excess), .excess.permission)]
    Excess { excess: Excess, limit: String },
    #[error("{CEILING_ENV}: {0}")]
    Ceiling(#[from] InvalidCeiling),
}

/// The flags that take a value, and the permission each asks for.
const VALUE_FLAGS: [Permission; 3] = [Permission::FsRead, Permission::FsWrite, Permission::Env];

impl PermissionFlags {
    /// What a launch started in `invocation_directory` asks for: `None` without
    /// `--permission`, when it holds whatever it inherits, and otherwise exactly what its
    /// `--allow-*` flags allow.
    ///
    /// # Errors
    ///
    /// [`PermissionError::MissingArgument`] for an empty value,
    /// [`PermissionError::PermissionRequired`] for an `--allow-*` flag without `--permission`,
    /// [`PermissionError::InvalidVariable`] for an `--allow-env` item that is not a variable
    /// pattern, and [`PermissionError::Path`] for a path that cannot be resolved.
    pub(crate) fn requested(
        &self,
        invocation_directory: &Path,
    ) -> Result<Option<PermissionSet>, PermissionError> {
        if let Some(permission) =
            VALUE_FLAGS.into_iter().find(|flag| self.values(*flag).iter().any(String::is_empty))
        {
            return Err(PermissionError::MissingArgument {
                flag: format!("{}=", permission.flag()),
            });
        }
        if !self.permission {
            return if self.allows_anything() {
                Err(PermissionError::PermissionRequired)
            } else {
                Ok(None)
            };
        }
        Ok(Some(PermissionSet {
            fs_read: paths(Permission::FsRead, &self.allow_fs_read, invocation_directory)?,
            fs_write: paths(Permission::FsWrite, &self.allow_fs_write, invocation_directory)?,
            env: variables(&self.allow_env)?,
            net: self.allow_net,
            child_process: self.allow_child_process,
            worker: self.allow_worker,
            addons: self.allow_addons,
            wasi: self.allow_wasi,
            ffi: self.allow_ffi,
            inspector: self.allow_inspector,
            openssl_store: self.allow_openssl_store,
            fs_vfs: self.allow_fs_vfs,
            language_model: self.allow_language_model,
        }))
    }

    /// Node's warning for each file system flag given a value with a comma, which names one
    /// path rather than a list of them.
    pub(crate) fn comma_warnings(&self) -> impl Iterator<Item = String> {
        [Permission::FsRead, Permission::FsWrite]
            .into_iter()
            .filter(|permission| self.values(*permission).iter().any(|value| value.contains(',')))
            .map(|permission| {
                format!(
                    "The {} CLI flag has changed. Passing a comma-separated list of paths is no \
                     longer valid. Documentation can be found at \
                     https://nodejs.org/api/permissions.html#file-system-permissions",
                    permission.flag()
                )
            })
    }

    fn values(&self, flag: Permission) -> &[String] {
        match flag {
            Permission::FsRead => &self.allow_fs_read,
            Permission::FsWrite => &self.allow_fs_write,
            _ => &self.allow_env,
        }
    }

    fn allows_anything(&self) -> bool {
        let booleans = [
            self.allow_net,
            self.allow_child_process,
            self.allow_worker,
            self.allow_addons,
            self.allow_wasi,
            self.allow_ffi,
            self.allow_inspector,
            self.allow_openssl_store,
            self.allow_fs_vfs,
            self.allow_language_model,
        ];
        booleans.into_iter().any(|given| given)
            || VALUE_FLAGS.into_iter().any(|flag| !self.values(flag).is_empty())
    }
}

/// The error Node reports when `usage` is clap's report that a value flag was given no value,
/// as at the end of the command line or before another option.
pub(crate) fn missing_argument(usage: &clap::Error) -> Option<PermissionError> {
    let is_empty =
        |kind| matches!(usage.get(kind), Some(ContextValue::String(value)) if value.is_empty());
    if usage.kind() != ErrorKind::InvalidValue || !is_empty(ContextKind::InvalidValue) {
        return None;
    }
    let Some(ContextValue::String(argument)) = usage.get(ContextKind::InvalidArg) else {
        return None;
    };
    let given = argument.split(' ').next()?;
    let flag = VALUE_FLAGS.into_iter().map(Permission::flag).find(|flag| *flag == given)?;
    Some(PermissionError::MissingArgument { flag: flag.to_owned() })
}

/// The paths `values` allow for `permission`, resolved against `base`.
fn paths(
    permission: Permission,
    values: &[String],
    base: &Path,
) -> Result<Scope<PathPattern>, PermissionError> {
    if values.iter().any(|value| value == "*") {
        return Ok(Scope::All);
    }
    let patterns = values
        .iter()
        .map(|value| {
            PathPattern::resolve(value, base).map_err(|source| PermissionError::Path {
                flag: permission.flag(),
                value: value.clone(),
                source,
            })
        })
        .collect::<Result<BTreeSet<_>, _>>()?;
    Ok(Scope::only(patterns))
}

/// The variables the comma-separated items of `values` allow. Empty items are skipped.
fn variables(values: &[String]) -> Result<Scope<EnvPattern>, PermissionError> {
    let items: Vec<&str> =
        values.iter().flat_map(|value| value.split(',')).filter(|item| !item.is_empty()).collect();
    let patterns = items
        .iter()
        .filter(|item| **item != "*")
        .map(|item| EnvPattern::parse(item).map_err(|_| PermissionError::InvalidVariable))
        .collect::<Result<BTreeSet<_>, _>>()?;
    Ok(if items.contains(&"*") { Scope::All } else { Scope::only(patterns) })
}

/// The flag that asked for `excess`, as a command line writes it.
fn requested_flag(excess: &Excess) -> String {
    match &excess.requested {
        Some(value) => format!("{}={value}", excess.permission.flag()),
        None => excess.permission.flag().to_owned(),
    }
}
