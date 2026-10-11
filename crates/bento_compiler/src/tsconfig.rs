use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use oxc_resolver::{CompilerOptions, ResolveError, ResolveOptions, Resolver};
use thiserror::Error;

use crate::Diagnostic;
use crate::settings::{CompilerSettings, JsxMode};
use crate::tsconfig_diagnostics::review;

/// A `tsconfig.json`, or a file in its `extends` chain, could not be read or parsed.
#[derive(Debug, Error, PartialEq, Eq)]
#[error("{}: {message}", path.display())]
pub struct ConfigError {
    /// The file that failed: the extended file when an `extends` target is malformed.
    pub path: PathBuf,
    pub message: String,
}

impl CompilerSettings {
    /// Reads `<project_root>/tsconfig.json` and its `extends` chain (relative files and
    /// package specifiers, JSONC) through `oxc_resolver`. Only an absent `tsconfig.json`
    /// yields the defaults; one that exists but cannot be read as a file is a `ConfigError`.
    /// Diagnostics report options Bento ignores (warnings) and settings under which some
    /// files cannot execute (errors); neither stops loading.
    pub fn load(project_root: &Path) -> Result<(CompilerSettings, Vec<Diagnostic>), ConfigError> {
        let tsconfig_path = project_root.join("tsconfig.json");
        if !tsconfig_present(&tsconfig_path)? {
            return Ok((CompilerSettings::default(), Vec::new()));
        }
        let resolver = Resolver::new(ResolveOptions::default());
        let tsconfig = resolver
            .resolve_tsconfig(&tsconfig_path)
            .map_err(|error| ConfigError::from_resolve(&tsconfig_path, error))?;
        let options = &tsconfig.compiler_options;
        let settings = settings_from(options);
        let diagnostics = review(&tsconfig_path, options, &settings);
        Ok((settings, diagnostics))
    }
}

/// `Ok(false)` only when nothing exists at `path`. A directory, a dangling symbolic link, or
/// an inaccessible entry is a configuration failure, not an absent configuration.
fn tsconfig_present(path: &Path) -> Result<bool, ConfigError> {
    let link_metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(ConfigError::io(path, error)),
    };
    if !link_metadata.is_symlink() {
        return ensure_regular_file(path, &link_metadata);
    }
    match fs::metadata(path) {
        Ok(metadata) => ensure_regular_file(path, &metadata),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Err(ConfigError::dangling(path)),
        Err(error) => Err(ConfigError::io(path, error)),
    }
}

fn ensure_regular_file(path: &Path, metadata: &fs::Metadata) -> Result<bool, ConfigError> {
    if metadata.is_file() {
        return Ok(true);
    }
    let message = if metadata.is_dir() { "is a directory, not a file" } else { "is not a file" };
    Err(ConfigError { path: path.to_path_buf(), message: message.to_owned() })
}

impl ConfigError {
    fn io(path: &Path, error: io::Error) -> Self {
        Self { path: path.to_path_buf(), message: error.to_string() }
    }

    fn dangling(path: &Path) -> Self {
        match fs::read_link(path) {
            Ok(target) => Self {
                path: path.to_path_buf(),
                message: format!(
                    "is a symbolic link to `{}`, which does not exist",
                    target.display()
                ),
            },
            Err(error) => Self::io(path, error),
        }
    }

    fn from_resolve(root: &Path, error: ResolveError) -> Self {
        match error {
            ResolveError::TsconfigLoadFailed { path, source } => match *source {
                ResolveError::Json(json) => {
                    Self { path, message: format!("invalid tsconfig JSON: {}", json.message) }
                }
                other => Self { path, message: other.to_string() },
            },
            ResolveError::TsconfigNotFound(target) => Self {
                path: root.to_path_buf(),
                message: format!("`extends` target `{}` was not found", target.display()),
            },
            other => Self { path: root.to_path_buf(), message: other.to_string() },
        }
    }
}

fn settings_from(options: &CompilerOptions) -> CompilerSettings {
    CompilerSettings {
        jsx: jsx_mode(options),
        verbatim_module_syntax: options.verbatim_module_syntax.unwrap_or(false),
        experimental_decorators: options.experimental_decorators.unwrap_or(false),
        emit_decorator_metadata: options.emit_decorator_metadata.unwrap_or(false),
        strict_null_checks: options.strict_null_checks.or(options.strict).unwrap_or(true),
        use_define_for_class_fields: options
            .use_define_for_class_fields
            .unwrap_or_else(|| target_defines_class_fields(options.target.as_deref())),
    }
}

fn jsx_mode(options: &CompilerOptions) -> JsxMode {
    let import_source = || options.jsx_import_source.clone().unwrap_or_else(|| "react".to_owned());
    match options.jsx.as_deref() {
        Some("react-jsx") => {
            JsxMode::Automatic { import_source: import_source(), development: false }
        }
        Some("react-jsxdev") => {
            JsxMode::Automatic { import_source: import_source(), development: true }
        }
        Some("react") => JsxMode::Classic {
            factory: options
                .jsx_factory
                .clone()
                .unwrap_or_else(|| "React.createElement".to_owned()),
            fragment_factory: options
                .jsx_fragment_factory
                .clone()
                .unwrap_or_else(|| "React.Fragment".to_owned()),
        },
        configured => JsxMode::Unavailable { configured: configured.map(str::to_owned) },
    }
}

/// TypeScript defaults `useDefineForClassFields` to `true` for ES2022 and newer targets.
/// Since TypeScript 6.0 the default target is the latest edition, so an unset target counts
/// as modern.
fn target_defines_class_fields(target: Option<&str>) -> bool {
    let Some(target) = target else { return true };
    let target = target.to_ascii_lowercase();
    let Some(edition) = target.strip_prefix("es") else { return false };
    edition == "next" || edition.parse::<u32>().is_ok_and(|year| year >= 2022)
}
