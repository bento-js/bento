#![allow(dead_code)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::{Value, json};
use tempfile::TempDir;

pub const SYSTEM_PATH: &str = "/usr/bin:/bin";
const SCRATCH: &str = "/tmp/bento-cli-tasks";

/// A disposable project under `/tmp/bento-cli-tasks/`.
pub struct TempProject {
    dir: TempDir,
    root: PathBuf,
}

impl TempProject {
    pub fn new(manifest: Value) -> Self {
        fs::create_dir_all(SCRATCH).unwrap();
        let dir = tempfile::Builder::new().prefix("cli-").tempdir_in(SCRATCH).unwrap();
        let root = dir.path().canonicalize().unwrap();
        let temp = TempProject { dir, root };
        temp.write("package.json", &manifest.to_string());
        temp
    }

    pub fn with_tasks(tasks: Value) -> Self {
        TempProject::new(json!({ "tasks": tasks }))
    }

    /// The canonical project root, which is how grants name it.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The root as created, reached through the `/tmp` symlink on macOS.
    pub fn linked_root(&self) -> &Path {
        self.dir.path()
    }

    pub fn path(&self, relative: &str) -> PathBuf {
        self.root.join(relative)
    }

    pub fn write(&self, relative: &str, contents: &str) -> PathBuf {
        let path = self.path(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, contents).unwrap();
        path
    }

    pub fn write_executable(&self, relative: &str, contents: &str) -> PathBuf {
        let path = self.write(relative, contents);
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    pub fn read(&self, relative: &str) -> String {
        fs::read_to_string(self.path(relative)).unwrap()
    }

    pub fn exists(&self, relative: &str) -> bool {
        self.path(relative).exists()
    }

    /// `bento <arguments>` started in the project root.
    pub fn bento(&self, arguments: &[&str]) -> Command {
        bento_in(&self.root, arguments)
    }
}

/// `bento <arguments>` started in `directory` with nothing but a system `PATH`, so a nested
/// `bento` is found only through the directory the running `bento` adds for its tasks.
pub fn bento_in(directory: &Path, arguments: &[&str]) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_bento"));
    command.args(arguments).current_dir(directory).env_clear().env("PATH", SYSTEM_PATH);
    command
}

pub fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

pub fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}
