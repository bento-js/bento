#![allow(dead_code, reason = "each integration test binary uses a different subset")]

use std::fs;
use std::path::{Path, PathBuf};

use tempfile::TempDir;

/// A disposable project directory under `/tmp/bento-project/`, addressed by its canonical path.
pub(crate) struct TempProject {
    _dir: TempDir,
    root: PathBuf,
}

impl TempProject {
    pub(crate) fn new() -> Self {
        fs::create_dir_all("/tmp/bento-project").unwrap();
        let dir =
            tempfile::Builder::new().prefix("workspace-").tempdir_in("/tmp/bento-project").unwrap();
        let root = dir.path().canonicalize().unwrap();
        TempProject { _dir: dir, root }
    }

    pub(crate) fn root(&self) -> &Path {
        &self.root
    }

    pub(crate) fn path(&self, relative: &str) -> PathBuf {
        self.root.join(relative)
    }

    pub(crate) fn write(&self, relative: &str, contents: &str) -> PathBuf {
        let path = self.path(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, contents).unwrap();
        path
    }
}

pub(crate) fn fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/remix-guest-book")
        .canonicalize()
        .unwrap()
}
