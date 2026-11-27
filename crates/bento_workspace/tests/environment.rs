mod support;

use std::collections::BTreeMap;

use bento_workspace::{EnvironmentError, LaunchEnvironment, LaunchKind};
use support::TempProject;

fn vars(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs.iter().map(|(name, value)| (name.to_string(), value.to_string())).collect()
}

fn load(temp: &TempProject, inherited: &[(&str, &str)], kind: LaunchKind) -> LaunchEnvironment {
    LaunchEnvironment::load(temp.root(), vars(inherited), kind).unwrap()
}

fn get<'a>(environment: &'a LaunchEnvironment, name: &str) -> Option<&'a str> {
    environment.vars().get(name).map(String::as_str)
}

#[test]
fn mode_comes_from_the_inherited_node_env_first() {
    let temp = TempProject::new();
    temp.write(".env", "NODE_ENV=production\n");
    temp.write(".env.production", "PRODUCTION=1\n");
    temp.write(".env.test", "TEST=1\n");

    let environment = load(&temp, &[("NODE_ENV", "test")], LaunchKind::Watch);

    assert_eq!(environment.mode(), Some("test"));
    assert_eq!(get(&environment, "NODE_ENV"), Some("test"));
    assert_eq!(get(&environment, "TEST"), Some("1"));
    assert_eq!(get(&environment, "PRODUCTION"), None);
}

#[test]
fn mode_falls_back_to_the_base_file() {
    let temp = TempProject::new();
    temp.write(".env", "NODE_ENV=production\n");
    temp.write(".env.production", "PRODUCTION=1\n");
    temp.write(".env.development", "DEVELOPMENT=1\n");

    let environment = load(&temp, &[], LaunchKind::Watch);

    assert_eq!(environment.mode(), Some("production"));
    assert_eq!(get(&environment, "NODE_ENV"), Some("production"));
    assert_eq!(get(&environment, "PRODUCTION"), Some("1"));
    assert_eq!(get(&environment, "DEVELOPMENT"), None);
}

#[test]
fn watch_launches_default_to_development_and_plain_launches_have_no_mode() {
    let temp = TempProject::new();
    temp.write(".env.development", "DEVELOPMENT=1\n");

    let watch = load(&temp, &[], LaunchKind::Watch);
    let plain = load(&temp, &[], LaunchKind::Run);

    assert_eq!(LaunchKind::Watch.default_mode(), Some("development"));
    assert_eq!(LaunchKind::Run.default_mode(), None);
    assert_eq!(watch.mode(), Some("development"));
    assert_eq!(get(&watch, "NODE_ENV"), Some("development"));
    assert_eq!(get(&watch, "DEVELOPMENT"), Some("1"));
    assert_eq!(plain.mode(), None);
    assert_eq!(get(&plain, "NODE_ENV"), None);
    assert_eq!(get(&plain, "DEVELOPMENT"), None);
}

#[test]
fn watch_default_enters_the_environment_without_any_dotenv_files() {
    let temp = TempProject::new();

    let environment = load(&temp, &[("PATH", "/usr/bin")], LaunchKind::Watch);

    assert_eq!(get(&environment, "NODE_ENV"), Some("development"));
    assert_eq!(get(&environment, "PATH"), Some("/usr/bin"));
}

#[test]
fn watch_default_never_overrides_an_explicit_value() {
    let temp = TempProject::new();
    temp.write(".env.production", "PRODUCTION=1\n");
    let exported = load(&temp, &[("NODE_ENV", "production")], LaunchKind::Watch);

    assert_eq!(get(&exported, "NODE_ENV"), Some("production"));
    assert_eq!(get(&exported, "PRODUCTION"), Some("1"));

    temp.write(".env.local", "NODE_ENV=staging\n");
    let from_file = load(&temp, &[], LaunchKind::Watch);

    assert_eq!(from_file.mode(), Some("development"));
    assert_eq!(get(&from_file, "NODE_ENV"), Some("staging"));
}

#[test]
fn later_files_override_earlier_files() {
    let temp = TempProject::new();
    temp.write(".env", "BASE=base\nMODE=base\nLOCAL=base\n");
    temp.write(".env.development", "MODE=mode\nLOCAL=mode\n");
    temp.write(".env.local", "LOCAL=local\n");

    let environment = load(&temp, &[], LaunchKind::Watch);

    assert_eq!(get(&environment, "BASE"), Some("base"));
    assert_eq!(get(&environment, "MODE"), Some("mode"));
    assert_eq!(get(&environment, "LOCAL"), Some("local"));
}

#[test]
fn process_variables_beat_file_values() {
    let temp = TempProject::new();
    temp.write(".env", "DATABASE_URL=file\nPORT=3000\n");
    temp.write(".env.local", "DATABASE_URL=local\n");

    let environment = load(&temp, &[("DATABASE_URL", "exported")], LaunchKind::Run);

    assert_eq!(get(&environment, "DATABASE_URL"), Some("exported"));
    assert_eq!(get(&environment, "PORT"), Some("3000"));
    assert!(!environment.file_keys().contains("DATABASE_URL"));
    assert!(environment.file_keys().contains("PORT"));
}

#[test]
fn missing_files_are_harmless() {
    let temp = TempProject::new();

    let environment = load(&temp, &[("HOME", "/home/me")], LaunchKind::Run);

    assert_eq!(environment.vars(), &vars(&[("HOME", "/home/me")]));
    assert!(environment.file_keys().is_empty());
}

#[test]
fn mode_is_selected_once() {
    let temp = TempProject::new();
    temp.write(".env", "NODE_ENV=production\n");
    temp.write(".env.production", "NODE_ENV=test\nPRODUCTION=1\n");
    temp.write(".env.test", "TEST=1\n");

    let environment = load(&temp, &[], LaunchKind::Run);

    assert_eq!(environment.mode(), Some("production"));
    assert_eq!(get(&environment, "NODE_ENV"), Some("test"));
    assert_eq!(get(&environment, "PRODUCTION"), Some("1"));
    assert_eq!(get(&environment, "TEST"), None);
}

#[test]
fn empty_node_env_that_was_set_selects_no_mode() {
    let temp = TempProject::new();
    temp.write(".env.development", "DEVELOPMENT=1\n");

    let environment = load(&temp, &[("NODE_ENV", "")], LaunchKind::Watch);

    assert_eq!(environment.mode(), None);
    assert_eq!(get(&environment, "NODE_ENV"), Some(""));
    assert_eq!(get(&environment, "DEVELOPMENT"), None);
}

#[test]
fn mode_cannot_name_a_file_outside_the_project_root() {
    let temp = TempProject::new();

    let error = LaunchEnvironment::load(
        temp.root(),
        vars(&[("NODE_ENV", "../../secrets")]),
        LaunchKind::Run,
    )
    .unwrap_err();

    assert!(matches!(error, EnvironmentError::InvalidMode { .. }), "{error:?}");
}

/// `9a7ce19baa54c278` is the fingerprint of `base` in the format an older `bento` wrote for
/// the launches it nested, so the entry is well formed rather than ignored as malformed.
#[test]
fn an_inherited_bento_env_file_keys_entry_never_demotes_an_inherited_value() {
    let temp = TempProject::new();
    temp.write(".env", "DATABASE_URL=base\n");
    temp.write(".env.development", "DATABASE_URL=dev\n");
    let file_keys = "DATABASE_URL:9a7ce19baa54c278";

    let environment = load(
        &temp,
        &[("DATABASE_URL", "base"), ("BENTO_ENV_FILE_KEYS", file_keys)],
        LaunchKind::Watch,
    );

    assert_eq!(get(&environment, "DATABASE_URL"), Some("base"));
    assert!(!environment.file_keys().contains("DATABASE_URL"));
    assert_eq!(get(&environment, "BENTO_ENV_FILE_KEYS"), Some(file_keys));
}

#[test]
fn a_malformed_bento_env_file_keys_is_an_ordinary_inherited_variable() {
    let temp = TempProject::new();

    let environment = load(&temp, &[("BENTO_ENV_FILE_KEYS", "not:a:list")], LaunchKind::Run);

    assert_eq!(get(&environment, "BENTO_ENV_FILE_KEYS"), Some("not:a:list"));
}

#[test]
fn loading_never_mutates_the_process_environment() {
    let temp = TempProject::new();
    let name = format!("BENTO_WORKSPACE_PROBE_{}", std::process::id());
    temp.write(".env", &format!("{name}=loaded\nNODE_ENV=production\n"));
    let node_env_before = std::env::var_os("NODE_ENV");

    let environment = load(&temp, &[], LaunchKind::Watch);

    assert_eq!(get(&environment, &name), Some("loaded"));
    assert_eq!(std::env::var_os(&name), None);
    assert_eq!(std::env::var_os("NODE_ENV"), node_env_before);
}
