use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

const DEPENDENCIES: &str = "node_modules";
const UNWATCHED_DIRECTORIES: [&str; 7] =
    [DEPENDENCIES, ".git", "dist", "build", "target", ".cache", "coverage"];
const DATABASE_EXTENSIONS: [&str; 3] = ["db", "sqlite", "sqlite3"];
const SQLITE_SIDE_FILE_SUFFIXES: [&str; 3] = ["-wal", "-shm", "-journal"];
const FINDER_METADATA: &str = ".DS_Store";
const ENVIRONMENT_FILE: &str = ".env";

/// The paths whose changes restart a watched script: the project's own files, except what
/// running or building it writes, and files added explicitly wherever they are.
pub struct WatchScope {
    project_root: PathBuf,
    files: BTreeSet<PathBuf>,
}

impl WatchScope {
    pub fn new(project_root: PathBuf) -> WatchScope {
        WatchScope { project_root, files: BTreeSet::new() }
    }

    pub fn project_root(&self) -> &Path {
        &self.project_root
    }

    pub fn add_file(&mut self, file: PathBuf) {
        self.files.insert(file);
    }

    pub fn includes(&self, path: &Path) -> bool {
        self.files.contains(path)
            || path.strip_prefix(&self.project_root).is_ok_and(is_project_source)
    }
}

/// Ancestors of the project do not make its own files dependencies, even if named `node_modules`.
pub fn is_dependency(project_root: &Path, module: &Path) -> bool {
    module
        .strip_prefix(project_root)
        .unwrap_or(module)
        .components()
        .any(|component| component.as_os_str() == DEPENDENCIES)
}

/// Whether `relative`, a path inside the project, is source rather than something running or
/// building the project writes. The project directory itself is not: its own notifications
/// are about entries that are reported separately.
fn is_project_source(relative: &Path) -> bool {
    let Some(name) = relative.file_name().and_then(OsStr::to_str) else {
        return false;
    };
    let in_unwatched_directory = relative.components().any(|component| {
        UNWATCHED_DIRECTORIES.iter().any(|directory| component.as_os_str() == *directory)
    });
    !(in_unwatched_directory
        || is_database(name)
        || is_environment_file(name)
        || name == FINDER_METADATA)
}

fn is_database(name: &str) -> bool {
    let extension = Path::new(name).extension().and_then(OsStr::to_str);
    extension.is_some_and(|extension| DATABASE_EXTENSIONS.contains(&extension))
        || SQLITE_SIDE_FILE_SUFFIXES.iter().any(|suffix| name.ends_with(suffix))
}

/// Environment files are launch inputs, so restarting the script would not apply a change.
fn is_environment_file(name: &str) -> bool {
    name.strip_prefix(ENVIRONMENT_FILE).is_some_and(|rest| rest.is_empty() || rest.starts_with('.'))
}
