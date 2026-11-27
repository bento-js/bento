use std::path::Path;

use bento_host::{PathPattern, PermissionSet, Permissions, Scope};
use bento_runtime::{RunOutcome, RuntimeConfig, start};

/// What [`config`] allows: reading and writing the project root and everything beneath it.
pub(crate) fn project_access(root: &Path) -> PermissionSet {
    let root = std::fs::canonicalize(root).unwrap();
    let project = PathPattern::resolve(&root, &root).unwrap();
    PermissionSet {
        fs_read: Scope::only([project.clone()]),
        fs_write: Scope::only([project]),
        ..PermissionSet::default()
    }
}

/// An instance for `root` holding `permissions`, with the root as its working directory.
pub(crate) fn config_with(root: &Path, permissions: Permissions) -> RuntimeConfig {
    let root = std::fs::canonicalize(root).unwrap();
    RuntimeConfig::new(root, permissions)
}

/// A restricted instance allowed only to read and write its project root.
#[allow(dead_code, reason = "shared helper is used by other integration test binaries")]
pub(crate) fn config(root: &Path) -> RuntimeConfig {
    config_with(root, Permissions::restricted(project_access(root)))
}

/// Like [`config`], also allowed to read every file.
#[allow(dead_code, reason = "shared helper is used by other integration test binaries")]
pub(crate) fn read_access(root: &Path) -> RuntimeConfig {
    config_with(
        root,
        Permissions::restricted(PermissionSet { fs_read: Scope::All, ..project_access(root) }),
    )
}

/// Like [`config`], also allowed the network.
#[allow(dead_code, reason = "shared helper is used by other integration test binaries")]
pub(crate) fn network_access(root: &Path) -> RuntimeConfig {
    config_with(root, Permissions::restricted(PermissionSet { net: true, ..project_access(root) }))
}

/// Like [`config`], reading all of `workspace`, a directory that contains the project root,
/// instead of only the root.
#[allow(dead_code, reason = "shared helper is used by other integration test binaries")]
pub(crate) fn workspace_access(root: &Path, workspace: &Path) -> RuntimeConfig {
    let workspace = std::fs::canonicalize(workspace).unwrap();
    let readable = PathPattern::resolve(&workspace, &workspace).unwrap();
    config_with(
        root,
        Permissions::restricted(PermissionSet {
            fs_read: Scope::only([readable]),
            ..project_access(root)
        }),
    )
}

pub(crate) fn run(config: RuntimeConfig, name: &str, source: &str) -> RunOutcome {
    let entry = config.project_root.join(name);
    std::fs::write(&entry, source).unwrap();
    start(config, &entry).unwrap().wait()
}

/// Like [`config`], also allowed the language model.
#[allow(dead_code, reason = "shared helper is used by other integration test binaries")]
pub(crate) fn model_access(root: &Path) -> RuntimeConfig {
    config_with(
        root,
        Permissions::restricted(PermissionSet { language_model: true, ..project_access(root) }),
    )
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
