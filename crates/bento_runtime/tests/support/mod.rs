use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use bento_host::{Grants, Scope};
use bento_runtime::{RunOutcome, RuntimeConfig, start};

pub(crate) fn config(root: &Path) -> RuntimeConfig {
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

pub(crate) fn run(config: RuntimeConfig, name: &str, source: &str) -> RunOutcome {
    let entry = config.project_root.join(name);
    std::fs::write(&entry, source).unwrap();
    start(config, &entry).unwrap().wait()
}

/// Like [`config`], with the language-model capability granted. Grants nothing else beyond
/// the project root's read and write scopes.
#[allow(dead_code, reason = "shared helper is used by other integration test binaries")]
pub(crate) fn model_access(root: &Path) -> RuntimeConfig {
    let mut settings = config(root);
    settings.grants.language_model = true;
    settings
}

#[allow(dead_code, reason = "shared helper is used by other integration test binaries")]
pub(crate) fn completed(outcome: RunOutcome) {
    assert!(matches!(outcome, RunOutcome::Completed), "{outcome:?}");
}

#[allow(dead_code, reason = "shared helper is used by other integration test binaries")]
pub(crate) fn project() -> tempfile::TempDir {
    let root = Path::new("/tmp/bento-runtime");
    std::fs::create_dir_all(root).unwrap();
    tempfile::tempdir_in(root).unwrap()
}

#[allow(dead_code, reason = "shared helper is used by other integration test binaries")]
pub(crate) fn fixture() -> tempfile::TempDir {
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
