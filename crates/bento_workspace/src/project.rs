use std::io;
use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::manifest::{Manifest, ManifestError};

const MANIFEST_FILE: &str = "package.json";

/// A directory holding `package.json`. Tasks run here, and environment files,
/// `node_modules`, and configuration are read relative to it.
#[derive(Clone, Debug)]
pub struct Project {
    root: PathBuf,
    manifest: Manifest,
}

#[derive(Debug, Error)]
pub enum ProjectError {
    #[error("no script file at {}", .0.display())]
    MissingScript(PathBuf),
    #[error("no package.json found in {} or any parent directory", .0.display())]
    NotFound(PathBuf),
    #[error("cannot resolve {}: {source}", .path.display())]
    Resolve { path: PathBuf, source: io::Error },
    #[error(transparent)]
    Manifest(#[from] ManifestError),
}

impl Project {
    /// The project enclosing a script: the nearest ancestor of the script's real
    /// location that contains `package.json`.
    pub fn for_script(script: &Path) -> Result<Project, ProjectError> {
        let resolved = script.canonicalize().map_err(|source| match source.kind() {
            io::ErrorKind::NotFound => ProjectError::MissingScript(script.to_owned()),
            _ => ProjectError::Resolve { path: script.to_owned(), source },
        })?;
        if !resolved.is_file() {
            return Err(ProjectError::MissingScript(script.to_owned()));
        }
        Project::nearest(resolved.parent().expect("a file path has a parent directory"))
    }

    /// The project for a task or installation started from `directory`.
    pub fn for_directory(directory: &Path) -> Result<Project, ProjectError> {
        let resolved = directory
            .canonicalize()
            .map_err(|source| ProjectError::Resolve { path: directory.to_owned(), source })?;
        Project::nearest(&resolved)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    fn nearest(start: &Path) -> Result<Project, ProjectError> {
        let root = start
            .ancestors()
            .find(|directory| directory.join(MANIFEST_FILE).is_file())
            .ok_or_else(|| ProjectError::NotFound(start.to_owned()))?;
        let manifest = Manifest::read(&root.join(MANIFEST_FILE))?;
        Ok(Project { root: root.to_owned(), manifest })
    }
}
