mod support;

use std::collections::{BTreeMap, BTreeSet};

use bento_workspace::{EnvironmentError, FILE_KEYS_ENV, LaunchEnvironment, LaunchKind};
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

/// The environment a nested `bento` launch inherits from a task command, launched by
/// this launch, that assigns the variables in `reassigned`.
fn forwarded_through(
    environment: &LaunchEnvironment,
    reassigned: &[&str],
) -> BTreeMap<String, String> {
    let reassigned: BTreeSet<String> = reassigned.iter().map(|name| name.to_string()).collect();
    let mut child = environment.vars().clone();
    child.insert(FILE_KEYS_ENV.to_string(), environment.file_keys_value(&reassigned));
    child
}

/// The environment a nested `bento` launch inherits from a task this launch runs.
fn forwarded(environment: &LaunchEnvironment) -> BTreeMap<String, String> {
    forwarded_through(environment, &[])
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

#[test]
fn nested_watch_launch_rederives_file_values_for_its_own_mode() {
    let temp = TempProject::new();
    temp.write(".env", "DATABASE_URL=base\n");
    temp.write(".env.development", "DATABASE_URL=dev\n");

    let outer = load(&temp, &[("PATH", "/usr/bin")], LaunchKind::Run);
    let child = forwarded(&outer);
    let nested = LaunchEnvironment::load(temp.root(), child.clone(), LaunchKind::Watch).unwrap();

    assert_eq!(get(&outer, "DATABASE_URL"), Some("base"));
    assert_eq!(get(&outer, "NODE_ENV"), None);
    assert_eq!(child.get("DATABASE_URL").map(String::as_str), Some("base"));
    assert!(child.contains_key(FILE_KEYS_ENV));
    assert_eq!(get(&nested, "DATABASE_URL"), Some("dev"));
    assert_eq!(get(&nested, "NODE_ENV"), Some("development"));
    assert_eq!(get(&nested, "PATH"), Some("/usr/bin"));
    assert_eq!(get(&nested, FILE_KEYS_ENV), None);
}

#[test]
fn exported_values_win_at_every_level() {
    let temp = TempProject::new();
    temp.write(".env", "DATABASE_URL=base\n");
    temp.write(".env.development", "DATABASE_URL=dev\n");

    let outer = load(&temp, &[("DATABASE_URL", "exported")], LaunchKind::Run);
    let nested =
        LaunchEnvironment::load(temp.root(), forwarded(&outer), LaunchKind::Watch).unwrap();
    let twice = LaunchEnvironment::load(temp.root(), forwarded(&nested), LaunchKind::Run).unwrap();

    assert_eq!(get(&outer, "DATABASE_URL"), Some("exported"));
    assert_eq!(get(&nested, "DATABASE_URL"), Some("exported"));
    assert_eq!(get(&twice, "DATABASE_URL"), Some("exported"));
}

#[test]
fn reassigned_forwarded_value_counts_as_explicit() {
    let temp = TempProject::new();
    temp.write(".env", "DATABASE_URL=base\n");
    temp.write(".env.development", "DATABASE_URL=dev\n");

    let outer = load(&temp, &[], LaunchKind::Run);
    let mut child = forwarded(&outer);
    child.insert("DATABASE_URL".to_string(), "inline".to_string());
    let nested = LaunchEnvironment::load(temp.root(), child, LaunchKind::Watch).unwrap();

    assert_eq!(get(&nested, "DATABASE_URL"), Some("inline"));
}

#[test]
fn file_value_the_command_reassigns_counts_as_explicit_even_when_unchanged() {
    let temp = TempProject::new();
    temp.write(".env", "DATABASE_URL=base\nPORT=3000\n");
    temp.write(".env.development", "DATABASE_URL=dev\nPORT=4000\n");

    let outer = load(&temp, &[], LaunchKind::Run);
    let child = forwarded_through(&outer, &["DATABASE_URL"]);
    let nested = LaunchEnvironment::load(temp.root(), child, LaunchKind::Watch).unwrap();

    assert_eq!(get(&nested, "DATABASE_URL"), Some("base"));
    assert_eq!(get(&nested, "PORT"), Some("4000"));
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
