#![allow(dead_code)]

use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use tempfile::TempDir;

use bento_host::Grants;
use bento_task::{Cancellation, Plan, TaskError, TaskRunner};
use bento_workspace::{LaunchEnvironment, LaunchKind, Project};

/// A disposable project under `/tmp/bento-project/`, addressed by its canonical path.
pub struct TempProject {
    _dir: TempDir,
    root: PathBuf,
}

impl TempProject {
    pub fn with_tasks(tasks: Value) -> Self {
        fs::create_dir_all("/tmp/bento-project").unwrap();
        let dir =
            tempfile::Builder::new().prefix("task-").tempdir_in("/tmp/bento-project").unwrap();
        let root = dir.path().canonicalize().unwrap();
        let temp = TempProject { _dir: dir, root };
        temp.write("package.json", &json!({ "tasks": tasks }).to_string());
        temp
    }

    pub fn root(&self) -> &Path {
        &self.root
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

    pub fn project(&self) -> Project {
        Project::for_directory(&self.root).unwrap()
    }

    /// Where these tests pretend the running `bento` executable lives.
    pub fn bento_directory(&self) -> PathBuf {
        self.path("bento-bin")
    }

    /// A stand-in for the running `bento` executable that records how it was invoked in
    /// `bento.txt` and the environment it received in `bento.env`.
    pub fn bento_executable(&self) -> PathBuf {
        self.write_executable(
            "bento-bin/bento",
            "#!/bin/sh\necho running \"$@\" > bento.txt\nenv -0 > bento.env\n",
        )
    }

    pub fn runner(&self, inherited: &[(&str, &str)], grants: &Grants) -> TaskRunner {
        let project = self.project();
        let launch =
            LaunchEnvironment::load(project.root(), vars(inherited), LaunchKind::Run).unwrap();
        TaskRunner::new(&project, &launch, grants, &self.bento_executable()).unwrap()
    }

    pub fn run(&self, task: &str, arguments: &[&str]) -> Result<(), TaskError> {
        let arguments: Vec<String> =
            arguments.iter().map(|argument| argument.to_string()).collect();
        let plan = Plan::new(self.project().manifest(), task, &arguments).unwrap();
        self.runner(&[("PATH", SYSTEM_PATH)], &Grants::none()).run(&plan, &Cancellation::new())
    }
}

pub const SYSTEM_PATH: &str = "/usr/bin:/bin";

pub fn vars(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs.iter().map(|(name, value)| (name.to_string(), value.to_string())).collect()
}

/// Parses the output of `env -0`, which keeps multi-line values intact.
pub fn parse_env_dump(dump: &str) -> BTreeMap<String, String> {
    dump.split('\0')
        .filter(|entry| !entry.is_empty())
        .map(|entry| {
            let (name, value) = entry.split_once('=').unwrap();
            (name.to_string(), value.to_string())
        })
        .collect()
}

pub fn fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/remix-guest-book")
        .canonicalize()
        .unwrap()
}
