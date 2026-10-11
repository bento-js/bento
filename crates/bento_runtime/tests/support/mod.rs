use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

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

#[allow(dead_code)]
pub fn fixture() -> &'static PathBuf {
    static FIXTURE: LazyLock<PathBuf> = LazyLock::new(|| {
        let root = PathBuf::from(format!("/tmp/bento-runtime/fixture-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/remix-guest-book/");
        let copied = std::process::Command::new("rsync")
            .args(["-a", "--exclude", "node_modules", "--exclude", "db/*.db"])
            .arg(source)
            .arg(&root)
            .status()
            .unwrap();
        assert!(copied.success());
        let installed = std::process::Command::new("pnpm")
            .args(["install", "--frozen-lockfile"])
            .current_dir(&root)
            .status()
            .unwrap();
        assert!(installed.success());
        root
    });
    &FIXTURE
}
