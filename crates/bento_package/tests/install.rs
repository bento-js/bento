use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use tempfile::TempDir;

use bento_package::{InstallError, Lockfile, install};
use bento_workspace::Project;

/// A disposable project under `/tmp/bento-project/` whose dependencies are local directories,
/// so pnpm installs it without registry access.
struct TempProject {
    _dir: TempDir,
    root: PathBuf,
}

impl TempProject {
    fn new(manifest: Value) -> Self {
        fs::create_dir_all("/tmp/bento-project").unwrap();
        let dir =
            tempfile::Builder::new().prefix("package-").tempdir_in("/tmp/bento-project").unwrap();
        let root = dir.path().canonicalize().unwrap();
        let temp = TempProject { _dir: dir, root };
        temp.write("package.json", &manifest.to_string());
        temp.write(
            "local-tool/package.json",
            &json!({ "name": "local-tool", "version": "1.0.0", "bin": { "local-tool": "bin.js" } })
                .to_string(),
        );
        temp.write("local-tool/bin.js", "#!/bin/sh\necho local-tool\n");
        temp.write("other-tool/package.json", r#"{ "name": "other-tool", "version": "1.0.0" }"#);
        temp
    }

    fn path(&self, relative: &str) -> PathBuf {
        self.root.join(relative)
    }

    fn write(&self, relative: &str, contents: &str) {
        let path = self.path(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }

    fn project(&self) -> Project {
        Project::for_directory(&self.root).unwrap()
    }
}

fn pnpm_manifest(dev_dependencies: Value) -> Value {
    json!({
        "name": "install-probe",
        "private": true,
        "devDependencies": dev_dependencies,
        "devEngines": { "packageManager": { "name": "pnpm" } },
    })
}

/// The caller's environment as pnpm sees it: CI's frozen-by-default lockfile behavior is
/// removed so tests observe pnpm's ordinary local behavior, and the store stays disposable.
fn environment(extra: &[(&str, &str)]) -> BTreeMap<String, String> {
    let mut environment: BTreeMap<String, String> =
        std::env::vars().filter(|(name, _)| name != "CI").collect();
    environment
        .insert("pnpm_config_store_dir".to_string(), "/tmp/bento-project/pnpm-store".to_string());
    for (name, value) in extra {
        environment.insert(name.to_string(), value.to_string());
    }
    environment
}

fn assert_nothing_installed(root: &Path) {
    assert!(!root.join("node_modules").exists());
    assert!(!root.join("pnpm-lock.yaml").exists());
}

#[test]
fn install_requires_a_declared_package_manager() {
    let temp =
        TempProject::new(json!({ "devDependencies": { "local-tool": "file:./local-tool" } }));

    let error = install(&temp.project(), Lockfile::Update, &environment(&[])).unwrap_err();

    assert!(matches!(error, InstallError::UnsupportedPackageManager { .. }), "{error:?}");
    let message = error.to_string();
    assert!(message.contains("devEngines.packageManager") && message.contains("pnpm"), "{message}");
    assert_nothing_installed(&temp.root);
}

#[test]
fn install_rejects_another_package_manager() {
    let mut manifest = pnpm_manifest(json!({ "local-tool": "file:./local-tool" }));
    manifest["devEngines"]["packageManager"]["name"] = json!("npm");
    let temp = TempProject::new(manifest);

    let error = install(&temp.project(), Lockfile::Update, &environment(&[])).unwrap_err();

    let message = error.to_string();
    assert!(message.contains("npm") && message.contains("pnpm"), "{message}");
    assert_nothing_installed(&temp.root);
}

#[test]
fn install_runs_pnpm_from_the_project_root_with_dev_dependencies_under_production() {
    let temp = TempProject::new(pnpm_manifest(json!({ "local-tool": "file:./local-tool" })));
    temp.write("app/routes/home.tsx", "");
    let project = Project::for_directory(&temp.path("app/routes")).unwrap();

    install(&project, Lockfile::Update, &environment(&[("NODE_ENV", "production")])).unwrap();

    assert!(temp.path("node_modules/.bin/local-tool").exists());
    assert!(temp.path("pnpm-lock.yaml").exists());
    assert!(!temp.path("app/routes/node_modules").exists());
}

#[test]
fn frozen_install_fails_on_a_mismatched_lockfile_without_rewriting_it() {
    let temp = TempProject::new(pnpm_manifest(json!({ "local-tool": "file:./local-tool" })));
    install(&temp.project(), Lockfile::Update, &environment(&[])).unwrap();
    let locked = fs::read(temp.path("pnpm-lock.yaml")).unwrap();
    let changed = pnpm_manifest(json!({
        "local-tool": "file:./local-tool",
        "other-tool": "file:./other-tool",
    }));
    temp.write("package.json", &changed.to_string());

    let error = install(&temp.project(), Lockfile::Frozen, &environment(&[])).unwrap_err();

    assert!(matches!(error, InstallError::Failed(_)), "{error:?}");
    assert_eq!(error.exit_code(), Some(1));
    assert!(error.to_string().contains("pnpm"), "{error}");
    assert_eq!(fs::read(temp.path("pnpm-lock.yaml")).unwrap(), locked);
    assert!(!temp.path("node_modules/other-tool").exists());
}

#[test]
fn normal_install_keeps_pnpm_update_behavior() {
    let temp = TempProject::new(pnpm_manifest(json!({ "local-tool": "file:./local-tool" })));
    install(&temp.project(), Lockfile::Update, &environment(&[])).unwrap();
    let changed = pnpm_manifest(json!({
        "local-tool": "file:./local-tool",
        "other-tool": "file:./other-tool",
    }));
    temp.write("package.json", &changed.to_string());

    install(&temp.project(), Lockfile::Update, &environment(&[])).unwrap();

    let lockfile = fs::read_to_string(temp.path("pnpm-lock.yaml")).unwrap();
    assert!(lockfile.contains("other-tool"), "{lockfile}");
    assert!(temp.path("node_modules/other-tool").exists());
}

fn project_with_build_script() -> (TempProject, PathBuf) {
    let temp = TempProject::new(pnpm_manifest(json!({ "build-probe": "file:./build-probe" })));
    let marker = temp.path("build-script-ran");
    let probe = json!({
        "name": "build-probe",
        "version": "1.0.0",
        "scripts": { "postinstall": format!("touch {}", marker.display()) },
    });
    temp.write("build-probe/package.json", &probe.to_string());
    (temp, marker)
}

#[test]
fn bento_never_approves_build_scripts() {
    let (temp, marker) = project_with_build_script();

    let error = install(&temp.project(), Lockfile::Update, &environment(&[])).unwrap_err();

    assert!(matches!(error, InstallError::Failed(_)), "{error:?}");
    assert!(!marker.exists());
}

#[test]
fn workspace_build_policy_is_respected() {
    let (temp, marker) = project_with_build_script();
    temp.write("pnpm-workspace.yaml", "allowBuilds:\n  \"build-probe@file:build-probe\": true\n");

    install(&temp.project(), Lockfile::Update, &environment(&[])).unwrap();

    assert!(marker.exists());
}

#[test]
fn missing_pnpm_is_a_missing_prerequisite() {
    let temp = TempProject::new(pnpm_manifest(json!({ "local-tool": "file:./local-tool" })));

    let error =
        install(&temp.project(), Lockfile::Update, &environment(&[("PATH", "/usr/bin:/bin")]))
            .unwrap_err();

    assert!(matches!(error, InstallError::MissingPnpm), "{error:?}");
    assert!(error.to_string().contains("pnpm"), "{error}");
    assert_nothing_installed(&temp.root);
}
