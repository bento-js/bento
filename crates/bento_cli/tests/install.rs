mod support;

use std::fs;
use std::process::Command;

use serde_json::{Value, json};

use support::{SYSTEM_PATH, TempProject, stderr};

/// A pnpm project whose development dependencies are local directories, so pnpm installs
/// them without registry access.
fn pnpm_project(dev_dependencies: Value) -> TempProject {
    let temp = TempProject::new(manifest(dev_dependencies));
    temp.write(
        "local-tool/package.json",
        &json!({ "name": "local-tool", "version": "1.0.0", "bin": { "local-tool": "bin.js" } })
            .to_string(),
    );
    temp.write("local-tool/bin.js", "#!/bin/sh\necho local-tool\n");
    temp.write("other-tool/package.json", r#"{ "name": "other-tool", "version": "1.0.0" }"#);
    temp
}

fn manifest(dev_dependencies: Value) -> Value {
    json!({
        "name": "install-probe",
        "private": true,
        "devDependencies": dev_dependencies,
        "devEngines": { "packageManager": { "name": "pnpm" } },
    })
}

/// `bento install` with the caller's own `PATH`, where pnpm lives, outside CI so the test
/// observes pnpm's ordinary local lockfile behavior. `CI=false` is the one setting that
/// overrides pnpm's detection of a CI service such as GitHub Actions (`GITHUB_ACTIONS`).
fn bento_install(temp: &TempProject, arguments: &[&str]) -> Command {
    let mut command = temp.bento(&[&["install"][..], arguments].concat());
    let caller: Vec<(String, String)> =
        std::env::vars().filter(|(name, _)| !name.starts_with("BENTO_")).collect();
    command
        .envs(caller)
        .env("CI", "false")
        .env("pnpm_config_store_dir", "/tmp/bento-cli-tasks/pnpm-store")
        .env("NODE_ENV", "production");
    command
}

#[test]
fn install_adds_a_local_dev_dependency() {
    let temp = pnpm_project(json!({ "local-tool": "file:./local-tool" }));

    let output = bento_install(&temp, &[]).output().unwrap();

    assert!(output.status.success(), "{}", stderr(&output));
    assert!(temp.exists("node_modules/.bin/local-tool"));
    assert!(temp.exists("pnpm-lock.yaml"));
}

#[test]
fn frozen_install_with_a_mismatched_lockfile_fails_and_keeps_the_lockfile() {
    let temp = pnpm_project(json!({ "local-tool": "file:./local-tool" }));
    let output = bento_install(&temp, &[]).output().unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let locked = fs::read(temp.path("pnpm-lock.yaml")).unwrap();
    let changed =
        manifest(json!({ "local-tool": "file:./local-tool", "other-tool": "file:./other-tool" }));
    temp.write("package.json", &changed.to_string());

    let output = bento_install(&temp, &["--frozen-lockfile"]).output().unwrap();

    assert!(!output.status.success());
    assert!(stderr(&output).contains("pnpm install failed"), "{}", stderr(&output));
    assert_eq!(fs::read(temp.path("pnpm-lock.yaml")).unwrap(), locked);
    assert!(!temp.exists("node_modules/other-tool"));
}

#[test]
fn install_requires_pnpm_as_the_declared_package_manager() {
    let temp = TempProject::new(json!({ "devDependencies": {} }));

    let output = bento_install(&temp, &[]).output().unwrap();

    assert_eq!(output.status.code(), Some(1));
    assert!(stderr(&output).contains("devEngines.packageManager"), "{}", stderr(&output));
    assert!(!temp.exists("node_modules"));
}

/// The review's reproduction: the shell that starts `bento install` resolves `PATH=./tools`
/// against its own working directory, so Bento must find the same pnpm there, and then run
/// it in the project root.
#[test]
fn install_finds_pnpm_on_a_relative_path_entry_from_the_current_directory() {
    let temp = pnpm_project(json!({ "local-tool": "file:./local-tool" }));
    let record = temp.path("pnpm-invocation");
    temp.write_executable(
        "app/tools/pnpm",
        &format!("#!/bin/sh\nprintf '%s %s\\n' \"$(/bin/pwd)\" \"$*\" > '{}'\n", record.display()),
    );
    let mut command = temp.bento(&["install"]);
    command.current_dir(temp.path("app")).env("PATH", format!("./tools:{SYSTEM_PATH}"));

    let output = command.output().unwrap();

    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(
        fs::read_to_string(record).unwrap(),
        format!("{} install --production=false\n", temp.root().display())
    );
    assert!(!temp.exists("node_modules"));
}
