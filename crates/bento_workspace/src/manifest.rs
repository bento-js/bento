use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde_json::Value;
use thiserror::Error;

/// The parts of `package.json` that Bento itself acts on.
#[derive(Clone, Debug, PartialEq)]
pub struct Manifest {
    /// `type`, or `None` when the field is absent.
    pub package_type: Option<PackageType>,
    /// `devEngines.packageManager`.
    pub package_manager: Option<PackageManager>,
    /// Top-level `tasks` exactly as declared. `scripts` never appear here. Shapes are
    /// validated when a launch plans the graph it selected, so an unrelated malformed
    /// task does not block other work.
    pub tasks: BTreeMap<String, Value>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PackageType {
    Module,
    CommonJs,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackageManager {
    pub name: String,
    pub version: Option<String>,
}

#[derive(Debug, Error)]
pub enum ManifestError {
    #[error("cannot read {}: {source}", .path.display())]
    Read { path: PathBuf, source: io::Error },
    #[error("{} is not valid JSON: {source}", .path.display())]
    Syntax { path: PathBuf, source: serde_json::Error },
    #[error("{}: {problem}", .path.display())]
    Invalid { path: PathBuf, problem: ManifestProblem },
}

/// Why a `package.json` is not one Bento can act on. Each variant names the field at fault.
#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum ManifestProblem {
    #[error("package.json must contain a JSON object")]
    NotAnObject,
    #[error("`type` must be \"module\" or \"commonjs\"")]
    InvalidPackageType,
    #[error(
        "`devEngines.packageManager` must be an object with a string `name` and an optional string `version`"
    )]
    InvalidPackageManager,
    #[error("`tasks` must be an object that maps task names to tasks")]
    InvalidTasks,
}

impl Manifest {
    /// Reads the `package.json` at `path`, keeping only the fields Bento acts on.
    ///
    /// # Errors
    ///
    /// [`ManifestError::Read`] when the file cannot be read, [`ManifestError::Syntax`] when
    /// it is not JSON, and [`ManifestError::Invalid`] when one of those fields has a shape
    /// Bento cannot use.
    pub fn read(path: &Path) -> Result<Manifest, ManifestError> {
        let text = fs::read_to_string(path)
            .map_err(|source| ManifestError::Read { path: path.to_owned(), source })?;
        let document: Value = serde_json::from_str(&text)
            .map_err(|source| ManifestError::Syntax { path: path.to_owned(), source })?;
        Manifest::from_document(document)
            .map_err(|problem| ManifestError::Invalid { path: path.to_owned(), problem })
    }

    fn from_document(document: Value) -> Result<Manifest, ManifestProblem> {
        let Value::Object(mut fields) = document else {
            return Err(ManifestProblem::NotAnObject);
        };
        let package_type = package_type(fields.get("type"))?;
        let package_manager = package_manager(fields.remove("devEngines"))?;
        Ok(Manifest { package_type, package_manager, tasks: tasks(fields.remove("tasks"))? })
    }
}

fn package_type(field: Option<&Value>) -> Result<Option<PackageType>, ManifestProblem> {
    match field.map(|kind| kind.as_str()) {
        None => Ok(None),
        Some(Some("module")) => Ok(Some(PackageType::Module)),
        Some(Some("commonjs")) => Ok(Some(PackageType::CommonJs)),
        Some(_) => Err(ManifestProblem::InvalidPackageType),
    }
}

fn package_manager(dev_engines: Option<Value>) -> Result<Option<PackageManager>, ManifestProblem> {
    let Some(Value::Object(mut engines)) = dev_engines else {
        return Ok(None);
    };
    let Some(declaration) = engines.remove("packageManager") else {
        return Ok(None);
    };
    let Value::Object(mut declaration) = declaration else {
        return Err(ManifestProblem::InvalidPackageManager);
    };
    let Some(Value::String(name)) = declaration.remove("name") else {
        return Err(ManifestProblem::InvalidPackageManager);
    };
    let version = match declaration.remove("version") {
        None => None,
        Some(Value::String(version)) => Some(version),
        Some(_) => return Err(ManifestProblem::InvalidPackageManager),
    };
    Ok(Some(PackageManager { name, version }))
}

fn tasks(field: Option<Value>) -> Result<BTreeMap<String, Value>, ManifestProblem> {
    match field {
        None => Ok(BTreeMap::new()),
        Some(Value::Object(tasks)) => Ok(tasks.into_iter().collect()),
        Some(_) => Err(ManifestProblem::InvalidTasks),
    }
}
