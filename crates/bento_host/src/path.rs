use std::io;
use std::path::{Component, Path, PathBuf};

/// Resolves an absolute path to the form grants are checked against: symlinks in its
/// longest existing ancestor are resolved, and the not-yet-existing remainder is
/// normalized lexically. A grant for a file that will be created (a new database, say)
/// therefore names the location its creator will actually reach.
pub fn canonical_path(path: &Path) -> io::Result<PathBuf> {
    if !path.is_absolute() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("grant path `{}` is not absolute", path.display()),
        ));
    }
    let normalized = normalize(path);
    let mut existing = normalized.as_path();
    let mut missing = Vec::new();
    loop {
        match std::fs::canonicalize(existing) {
            Ok(resolved) => {
                return Ok(missing.iter().rev().fold(resolved, |path, part| path.join(part)));
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                let Some(parent) = existing.parent() else { return Err(error) };
                missing.push(existing.file_name().expect("non-root path has a name").to_owned());
                existing = parent;
            }
            Err(error) => return Err(error),
        }
    }
}

/// Removes `.` and resolves `..` without touching the filesystem.
fn normalize(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other),
        }
    }
    normalized
}
