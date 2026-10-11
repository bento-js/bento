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
    errors: Mutex<VecDeque<(String, SourceLocation)>>,
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
        let settings = if path.starts_with(&self.root) && !in_node_modules(path) {
            &self.settings
        } else {
            &defaults
        };
        let transformed = transform(path, &text, settings).map_err(|error| {
            let location =
                SourceLocation { path: error.path.clone(), line: error.line, column: error.column };
            self.failure(ErrorKind::Transform, format!("BENTO_TRANSFORM {error}"), Some(location))
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
        self.failure(ErrorKind::Resolution, format!("BENTO_RESOLUTION Cannot resolve {specifier:?} from {referrer}: {detail}. For missing installed packages, run bento install."), location)
    }

    fn failure(
        &self,
        kind: ErrorKind,
        message: String,
        location: Option<SourceLocation>,
    ) -> JsErrorBox {
        if let Some(location) = location {
            let mut errors = self.errors.lock();
            if errors.len() == 256 {
                errors.pop_front();
            }
            errors.push_back((message.clone(), location));
        }
        JsErrorBox::new(
            if kind == ErrorKind::Transform { "SyntaxError" } else { "TypeError" },
            message,
        )
    }

    pub fn error_location(&self, message: &str) -> Option<SourceLocation> {
        self.errors
            .lock()
            .iter()
            .rev()
            .find(|(error, _)| message == error)
            .map(|(_, location)| location.clone())
    }

    pub fn is_project_source(&self, path: &Path) -> bool {
        self.is_local_source(path) && !in_node_modules(path)
    }

    pub fn is_local_source(&self, path: &Path) -> bool {
        path.starts_with(&self.root)
            && !path
                .file_name()
                .is_some_and(|name| name == ".env" || name.to_string_lossy().starts_with(".env."))
    }
}

fn in_node_modules(path: &Path) -> bool {
    path.components().any(|component| component.as_os_str() == "node_modules")
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(sources.error_location("missing module 511").is_some());
    }
}
