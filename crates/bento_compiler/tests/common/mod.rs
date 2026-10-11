#![allow(dead_code)]

use std::fs;
use std::path::Path;

use bento_compiler::{CompilerSettings, JsxMode, Transformed, transform};
use tempfile::TempDir;

pub fn automatic(import_source: &str) -> CompilerSettings {
    CompilerSettings {
        jsx: JsxMode::Automatic { import_source: import_source.to_owned(), development: false },
        ..CompilerSettings::default()
    }
}

pub fn classic(factory: &str, fragment_factory: &str) -> CompilerSettings {
    CompilerSettings {
        jsx: JsxMode::Classic {
            factory: factory.to_owned(),
            fragment_factory: fragment_factory.to_owned(),
        },
        ..CompilerSettings::default()
    }
}

pub fn transformed(path: &str, source: &str, settings: &CompilerSettings) -> Transformed {
    transform(Path::new(path), source, settings).unwrap_or_else(|error| panic!("{error}"))
}

pub fn code(path: &str, source: &str, settings: &CompilerSettings) -> String {
    transformed(path, source, settings).code
}

/// Writes `files` (relative paths, contents) into a fresh temporary project directory.
pub fn project(files: &[(&str, &str)]) -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    for (relative, contents) in files {
        let path = dir.path().join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }
    dir
}
