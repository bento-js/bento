use std::collections::{BTreeMap, VecDeque};
use std::path::{Path, PathBuf};

use deno_core::ModuleSpecifier;
use deno_error::JsErrorBox;
use parking_lot::Mutex;

use crate::import_position::import_position;
use crate::{ErrorKind, RuntimeError, SourceLocation};
use bento_compiler::{CompilerSettings, transform};

pub(crate) struct Sources {
    root: PathBuf,
    settings: CompilerSettings,
    maps: Mutex<BTreeMap<String, Vec<u8>>>,
    // JavaScript rethrows module-load failures without preserving their Rust error type.
    errors: Mutex<VecDeque<(String, RecordedFailure)>>,
}

#[derive(Clone)]
pub(crate) struct RecordedFailure {
    class: &'static str,
    pub kind: ErrorKind,
    pub location: Option<SourceLocation>,
}

impl Sources {
    pub fn load(root: &Path) -> Result<Self, RuntimeError> {
        let (settings, diagnostics) = CompilerSettings::load(root)
            .map_err(|error| RuntimeError::transform(error.to_string(), None))?;
        // An unexecutable JSX setting only matters for modules that contain JSX: the compiler
        // rejects those at transform time, naming the setting, so other modules still run.
        for diagnostic in diagnostics {
            eprintln!("{diagnostic}");
        }
        Ok(Self {
            root: root.to_owned(),
            settings,
            maps: Mutex::default(),
            errors: Mutex::default(),
        })
    }

    pub fn transform(&self, path: &Path, text: String) -> Result<String, JsErrorBox> {
        if !path.extension().is_some_and(|extension| {
            matches!(extension.to_str(), Some("ts" | "mts" | "cts" | "tsx" | "jsx"))
        }) {
            return Ok(text);
        }
        let defaults = CompilerSettings::default();
        let settings = if is_dependency(&self.root, path) { &defaults } else { &self.settings };
        let transformed = transform(path, &text, settings).map_err(|error| {
            let location =
                SourceLocation { path: error.path.clone(), line: error.line, column: error.column };
            self.failure(ErrorKind::Transform, error.to_string(), Some(location))
        })?;
        let name =
            ModuleSpecifier::from_file_path(path).expect("module paths are absolute").to_string();
        self.maps.lock().insert(name, transformed.source_map.into_bytes());
        Ok(transformed.code)
    }

    pub fn source_map(&self, name: &str) -> Option<Vec<u8>> {
        self.maps.lock().get(name).cloned()
    }

    pub fn resolution_error(&self, specifier: &str, referrer: &str, detail: &str) -> JsErrorBox {
        let location = ModuleSpecifier::parse(referrer)
            .ok()
            .and_then(|url| url.to_file_path().ok())
            .filter(|path| self.is_project_source(path))
            .map(|path| {
                let position = std::fs::read_to_string(&path)
                    .ok()
                    .and_then(|source| import_position(&path, &source, specifier));
                let (line, column) = position.unwrap_or((1, 1));
                SourceLocation { path, line, column }
            });
        let mut message = format!("Cannot resolve {specifier:?} from {referrer}: {detail}.");
        if !specifier.starts_with("host:")
            && !specifier.starts_with('.')
            && !Path::new(specifier).is_absolute()
            && ModuleSpecifier::parse(specifier).is_err()
        {
            message.push_str(" For missing installed packages, run bento install.");
        }
        self.failure(ErrorKind::Resolution, message, location)
    }

    pub(crate) fn failure(
        &self,
        kind: ErrorKind,
        message: String,
        location: Option<SourceLocation>,
    ) -> JsErrorBox {
        let mut errors = self.errors.lock();
        if errors.len() == 256 {
            errors.pop_front();
        }
        let class = match kind {
            ErrorKind::Startup | ErrorKind::Transform => "SyntaxError",
            _ => "TypeError",
        };
        errors.push_back((message.clone(), RecordedFailure { class, kind, location }));
        JsErrorBox::new(class, message)
    }

    pub fn error_failure(&self, class: &str, message: &str) -> Option<RecordedFailure> {
        self.errors
            .lock()
            .iter()
            .rev()
            .find(|(error, failure)| message == error && class == failure.class)
            .map(|(_, failure)| failure.clone())
    }

    pub fn is_project_source(&self, path: &Path) -> bool {
        self.is_local_source(path)
            && path.strip_prefix(&self.root).is_ok_and(|relative| !in_node_modules(relative))
    }

    pub fn is_local_source(&self, path: &Path) -> bool {
        path.starts_with(&self.root) && !is_environment_file(path)
    }
}

pub(crate) fn is_watch_source(root: &Path, path: &Path) -> bool {
    !is_environment_file(path) && !is_dependency(root, path)
}

fn is_dependency(root: &Path, path: &Path) -> bool {
    in_node_modules(path.strip_prefix(root).unwrap_or(path))
}

fn is_environment_file(path: &Path) -> bool {
    path.file_name()
        .is_some_and(|name| name == ".env" || name.to_string_lossy().starts_with(".env."))
}

fn in_node_modules(path: &Path) -> bool {
    path.components().any(|component| component.as_os_str() == "node_modules")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compiler_settings_follow_dependency_classification_for_generated_paths() {
        let workspace = tempfile::tempdir().unwrap();
        let source_paths = generated_tsx_paths();
        for project_directory in ["app", "node_modules/app"] {
            let root = workspace.path().join(project_directory);
            std::fs::create_dir_all(&root).unwrap();
            std::fs::write(
                root.join("tsconfig.json"),
                r#"{"compilerOptions":{"jsx":"react","jsxFactory":"h"}}"#,
            )
            .unwrap();
            let sources = Sources::load(&root).unwrap();
            for (base, ancestor_dependency) in [
                (root.join("src"), false),
                (workspace.path().join("shared"), false),
                (workspace.path().join("node_modules/shared"), true),
            ] {
                for (relative, nested_dependency) in &source_paths {
                    let path = base.join(relative);
                    let transformed =
                        sources.transform(&path, "export const view = <section />;".into());
                    let dependency = ancestor_dependency || *nested_dependency;
                    assert_eq!(transformed.is_ok(), !dependency, "{path:?}: {transformed:?}");
                }
            }
        }
    }

    fn generated_tsx_paths() -> Vec<(PathBuf, bool)> {
        let mut paths = Vec::new();
        for depth in 0..4 {
            for dependency_position in 0..=depth {
                let mut path: PathBuf =
                    (0..depth)
                        .map(|position| {
                            if position == dependency_position { "node_modules" } else { "source" }
                        })
                        .collect();
                path.push("view.tsx");
                paths.push((path, dependency_position < depth));
            }
        }
        paths
    }

    #[test]
    fn caught_module_failures_keep_a_bounded_location_history() {
        let root = tempfile::tempdir().unwrap();
        let sources = Sources::load(root.path()).unwrap();
        for index in 0..512 {
            let _ = sources.failure(
                ErrorKind::Resolution,
                format!("missing module {index}"),
                Some(SourceLocation { path: root.path().join("entry.ts"), line: 1, column: 1 }),
            );
        }
        assert!(sources.errors.lock().len() <= 256);
        assert!(
            sources.error_failure("TypeError", "missing module 511").unwrap().location.is_some()
        );
    }
}
