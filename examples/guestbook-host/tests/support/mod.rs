#![allow(dead_code, reason = "each integration test binary uses a different subset")]

pub mod http;

use std::fs;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::LazyLock;

use tempfile::TempDir;

/// Everything disposable the host's tests create lives here.
pub const SCRATCH: &str = "/tmp/bento-host";

pub fn scratch_dir() -> TempDir {
    fs::create_dir_all(SCRATCH).unwrap();
    tempfile::Builder::new().prefix("host-").tempdir_in(SCRATCH).unwrap()
}

/// `bento-guestbook-host <arguments>`, started in `directory`.
pub fn host_command(directory: &Path, arguments: &[&str]) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_bento-guestbook-host"));
    command.args(arguments).current_dir(directory);
    command
}

pub fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

pub fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// A port that is free on every local address, so a server binding `::` or `0.0.0.0` can take it.
pub fn free_port() -> u16 {
    TcpListener::bind("[::]:0").unwrap().local_addr().unwrap().port()
}

/// A disposable copy of the guest book fixture with its locked dependencies installed.
///
/// The copy lives at a fixed path and is brought up to date on first use in each test process:
/// `rsync --delete` mirrors the fixture's sources (never its `node_modules` or databases) and
/// `pnpm install --frozen-lockfile` reconciles the installed graph, which is a no-op when the
/// previous run already installed it.
pub fn fixture() -> &'static Path {
    static FIXTURE: LazyLock<PathBuf> = LazyLock::new(|| {
        let copy = PathBuf::from(SCRATCH).join("fixture");
        fs::create_dir_all(&copy).unwrap();
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/remix-guest-book/");
        let mirrored = Command::new("rsync")
            .args(["-a", "--delete", "--exclude", "node_modules", "--exclude", "db/*.db"])
            .arg(source)
            .arg(&copy)
            .status()
            .unwrap();
        assert!(mirrored.success(), "rsync of the fixture failed: {mirrored}");
        let installed = Command::new("pnpm")
            .args(["install", "--frozen-lockfile"])
            .current_dir(&copy)
            .status()
            .unwrap();
        assert!(installed.success(), "pnpm install in the fixture copy failed: {installed}");
        copy
    });
    &FIXTURE
}
