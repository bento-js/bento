use std::io;
use std::path::{Path, PathBuf};

use bento_host::canonical_path;

pub fn package_lookup_paths(project_root: &Path) -> io::Result<Vec<PathBuf>> {
    canonical_path(project_root)?
        .ancestors()
        .map(|ancestor| canonical_path(&ancestor.join("node_modules")))
        .collect()
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::symlink;

    use tempfile::tempdir;

    use super::package_lookup_paths;

    #[test]
    fn includes_each_ancestor_with_canonical_missing_tails() {
        let temporary = tempdir().unwrap();
        let root = temporary.path().join("workspace/application");
        std::fs::create_dir_all(&root).unwrap();
        let root = root.canonicalize().unwrap();
        let expected: Vec<_> =
            root.ancestors().map(|ancestor| ancestor.join("node_modules")).collect();
        assert_eq!(package_lookup_paths(&root).unwrap(), expected);
        assert_eq!(expected.last().unwrap(), &std::path::PathBuf::from("/node_modules"));
    }

    #[test]
    fn resolves_project_and_package_directory_symlinks() {
        let temporary = tempdir().unwrap();
        let root = temporary.path().join("application");
        let packages = temporary.path().join("packages");
        let alias = temporary.path().join("alias");
        std::fs::create_dir(&root).unwrap();
        std::fs::create_dir(&packages).unwrap();
        symlink(&root, &alias).unwrap();
        symlink(&packages, root.join("node_modules")).unwrap();
        let lookups = package_lookup_paths(&alias).unwrap();
        assert_eq!(lookups[0], packages.canonicalize().unwrap());
        assert_eq!(lookups, package_lookup_paths(&root).unwrap());
    }
}
