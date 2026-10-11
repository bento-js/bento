use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value};
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
    Invalid { path: PathBuf, problem: &'static str },
}

const PACKAGE_MANAGER_SHAPE: &str = "`devEngines.packageManager` must be an object with a string `name` and an optional string `version`";

impl Manifest {
    pub fn read(path: &Path) -> Result<Manifest, ManifestError> {
        let text = fs::read_to_string(path)
            .map_err(|source| ManifestError::Read { path: path.to_owned(), source })?;
        let document: Value = serde_json::from_str(&text)
            .map_err(|source| ManifestError::Syntax { path: path.to_owned(), source })?;
        Manifest::from_document(&document)
            .map_err(|problem| ManifestError::Invalid { path: path.to_owned(), problem })
    }

    fn from_document(document: &Value) -> Result<Manifest, &'static str> {
        let Value::Object(fields) = document else {
            return Err("package.json must contain a JSON object");
        };
        Ok(Manifest {
            package_type: package_type(fields.get("type"))?,
            package_manager: package_manager(fields)?,
            tasks: tasks(fields.get("tasks"))?,
        })
    }
}

fn package_type(field: Option<&Value>) -> Result<Option<PackageType>, &'static str> {
    match field.map(|kind| kind.as_str()) {
        None => Ok(None),
        Some(Some("module")) => Ok(Some(PackageType::Module)),
        Some(Some("commonjs")) => Ok(Some(PackageType::CommonJs)),
        Some(_) => Err("`type` must be \"module\" or \"commonjs\""),
    }
}

fn package_manager(fields: &Map<String, Value>) -> Result<Option<PackageManager>, &'static str> {
    let declaration = fields.get("devEngines").and_then(|engines| engines.get("packageManager"));
    let Some(declaration) = declaration else {
        return Ok(None);
    };
    let name = declaration.get("name").and_then(Value::as_str).ok_or(PACKAGE_MANAGER_SHAPE)?;
    let version = match declaration.get("version") {
        None => None,
        Some(Value::String(version)) => Some(version.clone()),
        Some(_) => return Err(PACKAGE_MANAGER_SHAPE),
    };
    Ok(Some(PackageManager { name: name.to_owned(), version }))
}

fn tasks(field: Option<&Value>) -> Result<BTreeMap<String, Value>, &'static str> {
    match field {
        None => Ok(BTreeMap::new()),
        Some(Value::Object(tasks)) => Ok(tasks.clone().into_iter().collect()),
        Some(_) => Err("`tasks` must be an object that maps task names to tasks"),
    }
}
