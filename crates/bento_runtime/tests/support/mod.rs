use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use bento_host::{Grants, Scope};
use bento_runtime::{RunOutcome, RuntimeConfig, start};

pub fn config(root: &Path) -> RuntimeConfig {
    let root = std::fs::canonicalize(root).unwrap();
    RuntimeConfig {
        project_root: root.to_owned(),
        cwd: root.to_owned(),
        grants: Grants {
            read: Scope::Only(BTreeSet::from([root.to_owned()])),
            write: Scope::Only(BTreeSet::from([root.to_owned()])),
            ..Grants::default()
        },
        env: BTreeMap::new(),
        argv: Vec::new(),
        host_modules: Vec::new(),
        shutdown_deadline: std::time::Duration::from_secs(5),
    }
}

pub fn run(config: RuntimeConfig, name: &str, source: &str) -> RunOutcome {
    let entry = config.project_root.join(name);
    std::fs::write(&entry, source).unwrap();
    start(config, &entry).unwrap().wait()
}

#[allow(dead_code, reason = "shared helper is used by other integration test binaries")]
pub fn completed(outcome: RunOutcome) {
    assert!(matches!(outcome, RunOutcome::Completed), "{outcome:?}");
}

#[allow(dead_code, reason = "shared helper is used by other integration test binaries")]
pub fn project() -> tempfile::TempDir {
    let root = Path::new("/tmp/bento-runtime");
    std::fs::create_dir_all(root).unwrap();
    tempfile::tempdir_in(root).unwrap()
}

#[allow(dead_code)]
pub fn fixture() -> tempfile::TempDir {
    let directory = project();
    let root = directory.path();
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/remix-guest-book/");
    let copied = std::process::Command::new("rsync")
        .args(["-a", "--exclude", "node_modules", "--exclude", "db/*.db"])
        .arg(source)
        .arg(root)
        .status()
        .unwrap();
    assert!(copied.success());
    let installed = std::process::Command::new("pnpm")
        .args(["install", "--frozen-lockfile"])
        .current_dir(root)
        .status()
        .unwrap();
    assert!(installed.success());
    directory
}

#[allow(dead_code, reason = "shared helper is used by other integration test binaries")]
pub fn isolated_fixture_test(name: &str) -> bool {
    if std::env::var("BENTO_FIXTURE_TEST").as_deref() == Ok(name) {
        assert!(std::env::var_os("NODE_PATH").is_none(), "fixture process inherited NODE_PATH");
        return false;
    }
    let child = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", name, "--nocapture"])
        .env("BENTO_FIXTURE_TEST", name)
        .env_remove("NODE_PATH")
        .output()
        .unwrap();
    assert!(
        child.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&child.stdout),
        String::from_utf8_lossy(&child.stderr)
    );
    let stdout = String::from_utf8_lossy(&child.stdout);
    assert!(
        stdout.lines().any(|line| line.starts_with("test result: ok. 1 passed;")),
        "fixture child did not run exactly one test:\n{stdout}"
    );
    eprint!("{}", String::from_utf8_lossy(&child.stderr));
    true
}
