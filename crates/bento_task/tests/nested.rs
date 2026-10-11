mod support;

use std::collections::{BTreeMap, BTreeSet};

use serde_json::json;

use bento_host::{CEILING_ENV, GrantRequest, Grants, Scope};
use bento_task::{Cancellation, Plan};
use bento_workspace::{FILE_KEYS_ENV, LaunchEnvironment, LaunchKind};
use support::{SYSTEM_PATH, TempProject, parse_env_dump};

/// Runs `task` from an outer, non-watch launch and returns the environment its command saw.
fn child_environment(
    temp: &TempProject,
    task: &str,
    inherited: &[(&str, &str)],
    grants: &Grants,
) -> BTreeMap<String, String> {
    let plan = Plan::new(temp.project().manifest(), task, &[]).unwrap();
    temp.runner(inherited, grants).run(&plan, &Cancellation::new()).unwrap();
    parse_env_dump(&temp.read("child.env"))
}

fn nested_watch_launch(
    temp: &TempProject,
    inherited: BTreeMap<String, String>,
) -> LaunchEnvironment {
    LaunchEnvironment::load(temp.root(), inherited, LaunchKind::Watch).unwrap()
}

fn value<'a>(environment: &'a LaunchEnvironment, name: &str) -> Option<&'a str> {
    environment.vars().get(name).map(String::as_str)
}

/// The environment a nested watch launch resolves when `command` starts it from a project
/// whose `.env` sets `DATABASE_URL=base` and whose `.env.development` sets `DATABASE_URL=dev`.
fn nested_launch_started_by(command: &str) -> LaunchEnvironment {
    let temp = TempProject::with_tasks(json!({ "dev": command }));
    temp.write(".env", "DATABASE_URL=base\n");
    temp.write(".env.development", "DATABASE_URL=dev\n");
    let child = child_environment(&temp, "dev", &[("PATH", SYSTEM_PATH)], &Grants::none());
    nested_watch_launch(&temp, child)
}

#[test]
fn task_children_receive_the_launch_grants_as_their_ceiling() {
    let temp = TempProject::with_tasks(json!({ "dump": "env -0 > child.env" }));
    let grants = GrantRequest {
        read: Some(Scope::Only(BTreeSet::from([temp.root().to_path_buf()]))),
        env: Some(Scope::Only(BTreeSet::from(["DATABASE_URL".to_string()]))),
        ..GrantRequest::default()
    }
    .resolve(None)
    .unwrap();

    let child = child_environment(&temp, "dump", &[("PATH", SYSTEM_PATH)], &grants);

    assert_eq!(Grants::from_ceiling_value(&child[CEILING_ENV]).unwrap(), grants);
    assert!(child.contains_key(FILE_KEYS_ENV));
}

#[test]
fn nested_watch_command_uses_its_own_mode_file() {
    let temp = TempProject::with_tasks(json!({ "dev": "env -0 > child.env" }));
    temp.write(".env", "DATABASE_URL=base\n");
    temp.write(".env.development", "DATABASE_URL=dev\n");

    let child = child_environment(&temp, "dev", &[("PATH", SYSTEM_PATH)], &Grants::none());
    let nested = nested_watch_launch(&temp, child.clone());

    assert_eq!(child["DATABASE_URL"], "base");
    assert!(!child.contains_key("NODE_ENV"));
    assert_eq!(value(&nested, "DATABASE_URL"), Some("dev"));
    assert_eq!(value(&nested, "NODE_ENV"), Some("development"));
}

#[test]
fn exported_value_still_wins_in_the_nested_launch() {
    let temp = TempProject::with_tasks(json!({ "dev": "env -0 > child.env" }));
    temp.write(".env", "DATABASE_URL=base\n");
    temp.write(".env.development", "DATABASE_URL=dev\n");

    let inherited = [("PATH", SYSTEM_PATH), ("DATABASE_URL", "exported")];
    let child = child_environment(&temp, "dev", &inherited, &Grants::none());
    let nested = nested_watch_launch(&temp, child.clone());

    assert_eq!(child["DATABASE_URL"], "exported");
    assert_eq!(value(&nested, "DATABASE_URL"), Some("exported"));
}

#[test]
fn inline_assignment_in_the_task_wins_in_the_nested_launch() {
    let temp = TempProject::with_tasks(json!({
        "dev": "DATABASE_URL=inline NODE_ENV=test env -0 > child.env",
    }));
    temp.write(".env", "DATABASE_URL=base\nNODE_ENV=production\n");
    temp.write(".env.development", "DATABASE_URL=dev\n");
    temp.write(".env.test", "TEST_ONLY=1\n");

    let child = child_environment(&temp, "dev", &[("PATH", SYSTEM_PATH)], &Grants::none());
    let nested = nested_watch_launch(&temp, child);

    assert_eq!(value(&nested, "DATABASE_URL"), Some("inline"));
    assert_eq!(value(&nested, "NODE_ENV"), Some("test"));
    assert_eq!(value(&nested, "TEST_ONLY"), Some("1"));
}

#[test]
fn inline_assignment_of_the_file_value_still_wins_in_the_nested_launch() {
    for command in [
        "DATABASE_URL=base env -0 > child.env",
        "true && DATABASE_URL='base' env -0 > child.env",
        "export DATABASE_URL=base; env -0 > child.env",
        "env DATABASE_URL=base env -0 > child.env",
    ] {
        let nested = nested_launch_started_by(command);

        assert_eq!(value(&nested, "DATABASE_URL"), Some("base"), "{command}");
    }
}

#[test]
fn text_that_only_resembles_an_assignment_leaves_the_file_value_to_the_nested_mode() {
    for command in [
        "echo DATABASE_URL=base > /dev/null; env -0 > child.env",
        "echo 'x; DATABASE_URL=base' > /dev/null; env -0 > child.env",
        "env -0 > child.env # DATABASE_URL=base",
    ] {
        let nested = nested_launch_started_by(command);

        assert_eq!(value(&nested, "DATABASE_URL"), Some("dev"), "{command}");
    }
}
