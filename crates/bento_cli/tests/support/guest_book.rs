//! Disposable copies of the guest-book fixture, prepared the way its README says, and the
//! commands that run Bento in them.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::TempDir;

use super::{bento_in, stderr};

/// The grants the fixture's README runs the guest book with.
pub const README_GRANTS: [&str; 5] = [
    "--allow-read",
    "--allow-write=./db",
    "--allow-net",
    "--allow-env=PORT,DATABASE_URL,NODE_ENV",
    "--allow-addons=./node_modules",
];
const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/remix-guest-book/");
const SCRATCH: &str = "/tmp/bento-cli/guest-book";

/// A copy of the fixture under `/tmp/bento-cli/guest-book/`, removed when dropped.
pub struct GuestBook {
    _directory: TempDir,
    root: PathBuf,
}

impl GuestBook {
    /// Copies the fixture without installed packages or databases, then installs its locked
    /// dependencies with `bento install --frozen-lockfile`.
    pub fn install() -> GuestBook {
        fs::create_dir_all(SCRATCH).unwrap();
        let directory = tempfile::tempdir_in(SCRATCH).unwrap();
        let root = directory.path().canonicalize().unwrap();
        let copied = Command::new("rsync")
            .args(["-a", "--exclude", "node_modules", "--exclude", "db/*.db", FIXTURE])
            .arg(&root)
            .status()
            .unwrap();
        assert!(copied.success());
        let guest_book = GuestBook { _directory: directory, root };
        let installed = guest_book
            .bento(&["install", "--frozen-lockfile"], &tool_path())
            .env("HOME", env::var_os("HOME").unwrap())
            .output()
            .unwrap();
        assert!(installed.status.success(), "{}", stderr(&installed));
        guest_book
    }

    pub fn path(&self, relative: &str) -> PathBuf {
        self.root.join(relative)
    }

    pub fn read(&self, relative: &str) -> String {
        fs::read_to_string(self.path(relative)).unwrap()
    }

    pub fn write(&self, relative: &str, contents: &str) {
        fs::write(self.path(relative), contents).unwrap();
    }

    /// `bento <arguments>` in the copy with `path` as `PATH` and nothing else inherited. That
    /// leaves out `NODE_PATH`, which the fixture's native resolver would follow out of the
    /// project.
    pub fn bento(&self, arguments: &[&str], path: &str) -> Command {
        let mut command = bento_in(&self.root, arguments);
        command.env("PATH", path);
        command
    }

    /// Creates the database at `relative` and applies the fixture's migrations with its own
    /// `remix db migrate`, a trusted package tool that runs under Node.
    pub fn migrate(&self, database: &str) {
        let migrated = Command::new(self.path("node_modules/.bin/remix"))
            .args(["db", "migrate"])
            .current_dir(&self.root)
            .env_clear()
            .env("PATH", tool_path())
            .env("DATABASE_URL", self.path(database))
            .output()
            .unwrap();
        assert!(migrated.status.success(), "{}", stderr(&migrated));
    }

    /// Saves an entry in the migrated database at `relative`, as signing the guest book would.
    pub fn add_entry(&self, database: &str, name: &str, message: &str) {
        let insert =
            format!("INSERT INTO guest_book (name, message) VALUES ('{name}', '{message}');");
        let added =
            Command::new("/usr/bin/sqlite3").arg(self.path(database)).arg(insert).output().unwrap();
        assert!(added.status.success(), "{}", stderr(&added));
    }
}

/// The fixture's own copy of `relative`, as committed.
pub fn fixture_file(relative: &str) -> String {
    fs::read_to_string(Path::new(FIXTURE).join(relative)).unwrap()
}

/// The test's own `PATH`, where `pnpm` and the Node that package tools run under are found.
pub fn tool_path() -> String {
    env::var("PATH").unwrap()
}

/// `bento run` with the README's grants and `target`.
pub fn run_with_readme_grants(target: &str) -> Vec<&str> {
    [&["run"][..], &README_GRANTS[..], &[target]].concat()
}
