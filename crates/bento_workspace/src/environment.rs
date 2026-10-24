use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::dotenv::{self, DotenvError};
use crate::provenance::{self, InvalidFileKeys};

const NODE_ENV: &str = "NODE_ENV";

/// How a launch runs, which decides the environment mode it falls back to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LaunchKind {
    Run,
    Watch,
}

impl LaunchKind {
    /// The mode used when neither the inherited environment nor `.env` sets `NODE_ENV`.
    pub fn default_mode(self) -> Option<&'static str> {
        match self {
            LaunchKind::Run => None,
            LaunchKind::Watch => Some("development"),
        }
    }
}

/// The environment one launch starts with: inherited variables layered over the
/// project's environment files. Building it never touches the calling process's
/// own environment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LaunchEnvironment {
    vars: BTreeMap<String, String>,
    file_keys: BTreeSet<String>,
    mode: Option<String>,
}

#[derive(Debug, Error)]
pub enum EnvironmentError {
    #[error(transparent)]
    Dotenv(#[from] DotenvError),
    #[error("cannot read {}: {source}", .path.display())]
    Read { path: PathBuf, source: io::Error },
    #[error("NODE_ENV `{mode}` cannot select an environment file because it contains `/`")]
    InvalidMode { mode: String },
    #[error(transparent)]
    FileKeys(#[from] InvalidFileKeys),
}

impl LaunchEnvironment {
    /// Loads `.env`, `.env.<mode>`, and `.env.local` from `project_root` beneath
    /// `inherited`, the environment the launch was started with.
    ///
    /// The mode is the inherited `NODE_ENV`, else `.env`'s, else `kind`'s default, and it
    /// is chosen once. Later files override earlier ones; inherited values override all
    /// files, except values an outer launch forwarded from its own files (recorded in
    /// [`crate::FILE_KEYS_ENV`]), which this launch may load again for its own mode.
    pub fn load(
        project_root: &Path,
        inherited: BTreeMap<String, String>,
        kind: LaunchKind,
    ) -> Result<LaunchEnvironment, EnvironmentError> {
        let inherited = provenance::split(inherited)?;
        let base = read_optional(&project_root.join(".env"))?;
        let mode = Mode::select(&inherited.actual, &base, kind)?;
        let mode_file = match &mode.name {
            Some(name) => read_optional(&project_root.join(format!(".env.{name}")))?,
            None => Vec::new(),
        };
        let local = read_optional(&project_root.join(".env.local"))?;

        let mut layers = Layers::default();
        layers.add_file_values(inherited.forwarded);
        if let Some(default) = mode.name.as_ref().filter(|_| mode.is_default) {
            layers.add_value(NODE_ENV.to_owned(), default.clone());
        }
        layers.add_file_values(base.into_iter().chain(mode_file).chain(local));
        for (name, value) in inherited.actual {
            layers.add_value(name, value);
        }
        Ok(LaunchEnvironment { vars: layers.vars, file_keys: layers.file_keys, mode: mode.name })
    }

    pub fn vars(&self) -> &BTreeMap<String, String> {
        &self.vars
    }

    /// The variables, for a caller that owns the environment and keeps nothing else of it.
    pub fn into_vars(self) -> BTreeMap<String, String> {
        self.vars
    }

    pub fn mode(&self) -> Option<&str> {
        self.mode.as_deref()
    }

    /// Variables whose values came from environment files rather than from the user.
    pub fn file_keys(&self) -> &BTreeSet<String> {
        &self.file_keys
    }

    /// The [`crate::FILE_KEYS_ENV`] value to hand a nested launch alongside [`Self::vars`].
    ///
    /// `reassigned` names the variables the command starting the nested launch assigns
    /// itself. They are left out, so the nested launch treats them as explicit values even
    /// when they repeat a value loaded from a file.
    pub fn file_keys_value(&self, reassigned: &BTreeSet<String>) -> String {
        provenance::encode(self.file_keys.difference(reassigned), &self.vars)
    }
}

struct Mode {
    name: Option<String>,
    is_default: bool,
}

impl Mode {
    fn select(
        actual: &BTreeMap<String, String>,
        base: &[(String, String)],
        kind: LaunchKind,
    ) -> Result<Mode, EnvironmentError> {
        let explicit = actual.get(NODE_ENV).map(String::as_str).or_else(|| {
            base.iter().rev().find(|(name, _)| name == NODE_ENV).map(|(_, value)| value.as_str())
        });
        let mode = match explicit {
            Some(value) => {
                Mode { name: (!value.is_empty()).then(|| value.to_owned()), is_default: false }
            }
            None => Mode { name: kind.default_mode().map(str::to_owned), is_default: true },
        };
        match mode.name {
            Some(name) if name.contains('/') => Err(EnvironmentError::InvalidMode { mode: name }),
            _ => Ok(mode),
        }
    }
}

#[derive(Default)]
struct Layers {
    vars: BTreeMap<String, String>,
    file_keys: BTreeSet<String>,
}

impl Layers {
    fn add_file_values(&mut self, assignments: impl IntoIterator<Item = (String, String)>) {
        for (name, value) in assignments {
            self.file_keys.insert(name.clone());
            self.vars.insert(name, value);
        }
    }

    /// Adds a value that did not come from an environment file.
    fn add_value(&mut self, name: String, value: String) {
        self.file_keys.remove(&name);
        self.vars.insert(name, value);
    }
}

fn read_optional(path: &Path) -> Result<Vec<(String, String)>, EnvironmentError> {
    match fs::read_to_string(path) {
        Ok(source) => Ok(dotenv::parse(path, &source)?),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(source) => Err(EnvironmentError::Read { path: path.to_owned(), source }),
    }
}
