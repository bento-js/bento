//! Project discovery, the `package.json` model, launch targets, and environment files.
//!
//! A host composes these in launch order: [`select_target`] (or [`Project::for_directory`]
//! for installation) finds the project, and [`LaunchEnvironment::load`] prepares the
//! environment the script or task starts with.

mod dotenv;
mod environment;
mod manifest;
mod project;
mod provenance;
mod target;

pub use dotenv::{DotenvError, DotenvProblem};
pub use environment::{EnvironmentError, LaunchEnvironment, LaunchKind};
pub use manifest::{Manifest, ManifestError, PackageManager, PackageType};
pub use project::{Project, ProjectError};
pub use provenance::{FILE_KEYS_ENV, InvalidFileKeys};
pub use target::{Selection, Target, TargetError, select_target};
