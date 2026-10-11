mod support;

use std::os::unix::fs::symlink;
use std::path::PathBuf;

use serde_json::json;

use bento_host::{Grants, NetTarget, Scope};
use support::{TempProject, bento_in, stderr, stdout};

/// A project whose `ceiling` task records the grants its launch handed to task commands.
fn project() -> TempProject {
    let temp = TempProject::with_tasks(json!({
        "ceiling": "printf '%s' \"$BENTO_GRANT_CEILING\" > ceiling.json",
        "nested": "bento run ceiling",
        "narrowed": "bento run --allow-read=data/inner --allow-net=localhost:3000 ceiling",
        "widened": "bento run --allow-read=. ceiling",
        "respelled": "bento run '--allow-net=[0:0:0:0:0:0:0:1]:18540' ceiling",
    }));
    temp.write("data/inner/entry.txt", "");
    temp.write("db/.keep", "");
    temp.write("node_modules/.keep", "");
    temp
}

fn recorded_grants(temp: &TempProject) -> Grants {
    Grants::from_ceiling_value(&temp.read("ceiling.json")).unwrap()
}

fn paths(paths: &[PathBuf]) -> Scope<PathBuf> {
    Scope::only(paths.iter().cloned())
}

fn names(names: &[&str]) -> Scope<String> {
    Scope::only(names.iter().map(|name| name.to_string()))
}

#[test]
fn the_guest_book_grant_set_resolves_to_canonical_scopes() {
    let temp = project();
    let fixture_grants = [
        "--allow-read=.",
        "--allow-write=./db",
        "--allow-net",
        "--allow-env=PORT,DATABASE_URL,NODE_ENV",
        "--allow-addons=./node_modules",
    ];
    let arguments = [&["run"][..], &fixture_grants[..], &["ceiling"][..]].concat();

    let output = temp.bento(&arguments).output().unwrap();

    assert!(output.status.success(), "{}", stderr(&output));
    let expected = Grants {
        read: paths(&[temp.root().to_owned()]),
        write: paths(&[temp.path("db")]),
        net: Scope::All,
        env: names(&["PORT", "DATABASE_URL", "NODE_ENV"]),
        addons: paths(&[temp.path("node_modules")]),
    };
    assert_eq!(recorded_grants(&temp), expected);
}

#[test]
fn grant_paths_resolve_from_the_invocation_directory_through_symlinks() {
    let temp = project();
    symlink(temp.path("data"), temp.path("linked-data")).unwrap();
    let invocation_directory = temp.linked_root().join("data");
    let through_tmp_link = temp.linked_root().join("db");
    let read = format!("--allow-read=../linked-data,{}", through_tmp_link.display());

    let output = bento_in(&invocation_directory, &["run", &read, "ceiling"]).output().unwrap();

    assert!(output.status.success(), "{}", stderr(&output));
    let grants = recorded_grants(&temp);
    assert_eq!(grants.read, paths(&[temp.path("data"), temp.path("db")]));
    assert_eq!(grants.write, Scope::None);
}

#[test]
fn net_targets_and_env_names_are_listed_items() {
    let temp = project();

    let output = temp
        .bento(&[
            "run",
            "--allow-net=localhost:3000,[::1]:8080,example.com",
            "--allow-env=PORT",
            "ceiling",
        ])
        .output()
        .unwrap();

    assert!(output.status.success(), "{}", stderr(&output));
    let grants = recorded_grants(&temp);
    let targets = [
        NetTarget::new("localhost", Some(3000)),
        NetTarget::new("::1", Some(8080)),
        NetTarget::new("example.com", None),
    ];
    assert_eq!(grants.net, Scope::only(targets));
    assert_eq!(grants.env, names(&["PORT"]));
}

#[test]
fn a_nested_launch_without_grant_flags_inherits_the_outer_grants() {
    let temp = project();

    let output = temp
        .bento(&["run", "--allow-read=data", "--allow-net=localhost", "--allow-env", "nested"])
        .output()
        .unwrap();

    assert!(output.status.success(), "{}", stderr(&output));
    let expected = Grants {
        read: paths(&[temp.path("data")]),
        net: Scope::only([NetTarget::new("localhost", None)]),
        env: Scope::All,
        ..Grants::none()
    };
    assert_eq!(recorded_grants(&temp), expected);
}

#[test]
fn a_nested_launch_may_narrow_the_outer_grants() {
    let temp = project();

    let output = temp
        .bento(&["run", "--allow-read=data", "--allow-net=localhost", "narrowed"])
        .output()
        .unwrap();

    assert!(output.status.success(), "{}", stderr(&output));
    let expected = Grants {
        read: paths(&[temp.path("data/inner")]),
        net: Scope::only([NetTarget::new("localhost", Some(3000))]),
        ..Grants::none()
    };
    assert_eq!(recorded_grants(&temp), expected);
}

#[test]
fn a_nested_launch_may_spell_an_inherited_endpoint_differently() {
    let temp = project();

    let output = temp.bento(&["run", "--allow-net=[::1]:18540", "respelled"]).output().unwrap();

    assert!(output.status.success(), "{}", stderr(&output));
    let endpoint = "[::1]:18540".parse::<NetTarget>().unwrap();
    assert_eq!(recorded_grants(&temp).net, Scope::only([endpoint]));
}

#[test]
fn a_wider_nested_request_fails_before_its_task_runs() {
    let temp = project();

    let output = temp.bento(&["run", "--allow-read=data", "widened"]).output().unwrap();

    assert_eq!(output.status.code(), Some(1));
    let message = stderr(&output);
    assert!(message.contains(&format!("--allow-read={}", temp.root().display())), "{message}");
    assert!(message.contains(&temp.path("data").display().to_string()), "{message}");
    assert!(message.contains("task `widened` failed"), "{message}");
    assert!(!temp.exists("ceiling.json"));
}

#[test]
fn a_whole_kind_beyond_the_inherited_grant_names_the_bare_flag() {
    let temp = project();
    let ceiling = Grants { read: paths(&[temp.path("data")]), ..Grants::none() };

    let output = temp
        .bento(&["run", "--allow-write", "ceiling"])
        .env("BENTO_GRANT_CEILING", ceiling.to_ceiling_value())
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    let message = stderr(&output);
    assert!(message.contains("--allow-write") && message.contains("none"), "{message}");
    assert!(!temp.exists("ceiling.json"));
}

#[test]
fn malformed_grant_flags_fail_before_anything_runs() {
    let temp = project();
    let malformed = [
        ("--allow-net=localhost:http", "--allow-net"),
        ("--allow-read=", "--allow-read"),
        ("--allow-env=PORT,,NODE_ENV", "--allow-env"),
    ];

    for (flag, named) in malformed {
        let output = temp.bento(&["run", flag, "ceiling"]).output().unwrap();

        assert_eq!(output.status.code(), Some(1), "{flag}");
        assert!(stderr(&output).contains(named), "{flag}: {}", stderr(&output));
        assert!(!temp.exists("ceiling.json"), "{flag}");
    }
}

#[test]
fn malformed_network_targets_fail_before_a_task_or_script_runs() {
    let temp = project();
    temp.write("show.mjs", "console.log('ran');\n");
    let malformed =
        ["--allow-net=localhost/path", "--allow-net=[bogus]:18540", "--allow-net=white space"];

    for flag in malformed {
        let task = temp.bento(&["run", flag, "ceiling"]).output().unwrap();
        let script = temp.bento(&["run", "--allow-read=.", flag, "show.mjs"]).output().unwrap();

        for output in [&task, &script] {
            assert_eq!(output.status.code(), Some(1), "{flag}");
            assert!(stderr(output).contains("--allow-net"), "{flag}: {}", stderr(output));
        }
        assert!(!temp.exists("ceiling.json"), "{flag}");
        assert_eq!(stdout(&script), "", "{flag}");
    }
}

#[test]
fn a_grant_flag_given_twice_is_rejected_rather_than_merged() {
    let temp = project();

    let output =
        temp.bento(&["run", "--allow-read", "--allow-read=data", "ceiling"]).output().unwrap();

    assert_eq!(output.status.code(), Some(1));
    assert!(stderr(&output).contains("--allow-read"), "{}", stderr(&output));
    assert!(!temp.exists("ceiling.json"));
}

#[test]
fn an_unreadable_inherited_ceiling_fails_closed() {
    let temp = project();

    let output = temp
        .bento(&["run", "ceiling"])
        .env("BENTO_GRANT_CEILING", "not a policy")
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    assert!(stderr(&output).contains("BENTO_GRANT_CEILING"), "{}", stderr(&output));
    assert!(!temp.exists("ceiling.json"));
}
