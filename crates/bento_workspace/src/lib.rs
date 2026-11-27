//! Project discovery, the `package.json` model, and environment files.
//!
//! A host composes these in launch order: [`Project::for_script`] (or
//! [`Project::for_directory`] for installation) finds the project, and
//! [`LaunchEnvironment::load`] prepares the environment the script starts with.

mod dotenv;
mod environment;
mod manifest;
mod project;

pub use dotenv::{DotenvError, DotenvProblem};
pub use environment::{EnvironmentError, LaunchEnvironment, LaunchKind};
pub use manifest::{Manifest, ManifestError, ManifestProblem, PackageManager, PackageType};
pub use project::{Project, ProjectError};
