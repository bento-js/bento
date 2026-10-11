use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::path::{Path, PathBuf};

use clap::{ArgAction, Args};
use thiserror::Error;

use bento_host::{
    CEILING_ENV, Covers, GrantKind, GrantRequest, Grants, InvalidCeiling, InvalidNetTarget,
    NetTarget, Scope, WiderGrant, canonical_path,
};

/// The grant options of one launch. A flag without `=` grants its whole kind; with `=`, only
/// the comma-separated items. Each flag may be given once.
#[derive(Debug, Args)]
#[command(next_help_heading = "Grants")]
pub struct GrantFlags {
    /// Allow reading beneath these paths, or anywhere
    #[arg(
        long, value_name = "PATH", num_args = 0.., action = ArgAction::Set,
        require_equals = true, value_delimiter = ','
    )]
    allow_read: Option<Vec<String>>,
    /// Allow writing beneath these paths, or anywhere
    #[arg(
        long, value_name = "PATH", num_args = 0.., action = ArgAction::Set,
        require_equals = true, value_delimiter = ','
    )]
    allow_write: Option<Vec<String>>,
    /// Allow connecting to and listening on these hosts, or any host
    #[arg(
        long, value_name = "HOST[:PORT]", num_args = 0.., action = ArgAction::Set,
        require_equals = true, value_delimiter = ','
    )]
    allow_net: Option<Vec<String>>,
    /// Allow access to these environment variables, or to all of them
    #[arg(
        long, value_name = "NAME", num_args = 0.., action = ArgAction::Set,
        require_equals = true, value_delimiter = ','
    )]
    allow_env: Option<Vec<String>>,
    /// Allow loading native addons beneath these paths, or anywhere
    #[arg(
        long, value_name = "PATH", num_args = 0.., action = ArgAction::Set,
        require_equals = true, value_delimiter = ','
    )]
    allow_addons: Option<Vec<String>>,
}

#[derive(Debug, Error)]
pub enum GrantError {
    #[error("--allow-{kind} has an empty item")]
    EmptyItem { kind: GrantKind },
    #[error("--allow-{kind}: cannot resolve `{path}`: {source}")]
    Path { kind: GrantKind, path: String, source: io::Error },
    #[error("--allow-net: {0}")]
    NetTarget(#[from] InvalidNetTarget),
    #[error("{flag} asks for more than the inherited {kind} grant: {limit}")]
    Wider { flag: String, kind: GrantKind, limit: String },
    #[error("{CEILING_ENV}: {0}")]
    Ceiling(#[from] InvalidCeiling),
}

/// Removes the policy an invoking launch handed down from `environment` and returns it.
pub fn take_ceiling(
    environment: &mut BTreeMap<String, String>,
) -> Result<Option<Grants>, GrantError> {
    let ceiling = environment.remove(CEILING_ENV);
    Ok(ceiling.map(|value| Grants::from_ceiling_value(&value)).transpose()?)
}

impl GrantFlags {
    /// The grants these flags give a launch started in `invocation_directory` under `ceiling`,
    /// the policy of the launch that started this one, if any.
    pub fn resolve(
        &self,
        invocation_directory: &Path,
        ceiling: Option<&Grants>,
    ) -> Result<Grants, GrantError> {
        let request = self.request(invocation_directory)?;
        match ceiling {
            None => {
                Ok(request.resolve(None).expect("a launch without a ceiling grants any request"))
            }
            Some(ceiling) => request
                .clone()
                .resolve(Some(ceiling))
                .map_err(|wider| exceeded(&wider, &request, ceiling)),
        }
    }

    fn request(&self, directory: &Path) -> Result<GrantRequest, GrantError> {
        let path = |kind| move |item: &str| canonical_item(kind, directory, item);
        Ok(GrantRequest {
            read: scope(GrantKind::Read, &self.allow_read, path(GrantKind::Read))?,
            write: scope(GrantKind::Write, &self.allow_write, path(GrantKind::Write))?,
            net: scope(GrantKind::Net, &self.allow_net, |item| Ok(item.parse::<NetTarget>()?))?,
            env: scope(GrantKind::Env, &self.allow_env, |item| Ok(item.to_owned()))?,
            addons: scope(GrantKind::Addons, &self.allow_addons, path(GrantKind::Addons))?,
        })
    }
}

fn scope<T: Ord>(
    kind: GrantKind,
    values: &Option<Vec<String>>,
    parse: impl Fn(&str) -> Result<T, GrantError>,
) -> Result<Option<Scope<T>>, GrantError> {
    let Some(values) = values else {
        return Ok(None);
    };
    if values.is_empty() {
        return Ok(Some(Scope::All));
    }
    let items = values
        .iter()
        .map(|item| if item.is_empty() { Err(GrantError::EmptyItem { kind }) } else { parse(item) })
        .collect::<Result<BTreeSet<T>, _>>()?;
    Ok(Some(Scope::Only(items)))
}

fn canonical_item(kind: GrantKind, directory: &Path, item: &str) -> Result<PathBuf, GrantError> {
    canonical_path(&directory.join(item)).map_err(|source| GrantError::Path {
        kind,
        path: item.to_owned(),
        source,
    })
}

fn exceeded(wider: &WiderGrant, request: &GrantRequest, ceiling: &Grants) -> GrantError {
    let (requested_all, limit) = match wider.kind {
        GrantKind::Read => (is_all(&request.read), describe(&ceiling.read)),
        GrantKind::Write => (is_all(&request.write), describe(&ceiling.write)),
        GrantKind::Net => (is_all(&request.net), describe(&ceiling.net)),
        GrantKind::Env => (is_all(&request.env), describe(&ceiling.env)),
        GrantKind::Addons => (is_all(&request.addons), describe(&ceiling.addons)),
    };
    let flag = if requested_all {
        format!("--allow-{}", wider.kind)
    } else {
        format!("--allow-{}={}", wider.kind, wider.requested)
    };
    GrantError::Wider { flag, kind: wider.kind, limit }
}

fn is_all<T: Ord>(requested: &Option<Scope<T>>) -> bool {
    matches!(requested, Some(Scope::All))
}

fn describe<T: Ord + Covers>(scope: &Scope<T>) -> String {
    match scope {
        Scope::None => "none".to_owned(),
        Scope::All => "all".to_owned(),
        Scope::Only(items) => items.iter().map(Covers::describe).collect::<Vec<_>>().join(", "),
    }
}
