mod support;

use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;

use serde_json::json;

use support::{TempProject, stderr, stdout};

#[test]
fn environment_files_reach_task_commands_in_precedence_order() {
    let temp = TempProject::with_tasks(json!({
        "show": "printf '%s %s %s %s\\n' \"$A\" \"$B\" \"$C\" \"$D\"",
    }));
    temp.write(".env", "A=base\nB=base\nC=base\nD=base\n");
    temp.write(".env.test", "B=test\nC=test\nD=test\n");
    temp.write(".env.local", "C=local\nD=local\n");

    let output =
        temp.bento(&["run", "show"]).env("NODE_ENV", "test").env("D", "exported").output().unwrap();

    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(stdout(&output), "base test local exported\n");
}

/// An outer task with no mode whose command starts a nested launch in the `test` mode.
fn nested_mode_project() -> TempProject {
    let temp = TempProject::with_tasks(json!({
        "outer": "printf 'outer %s\\n' \"$DATABASE_URL\"; NODE_ENV=test bento run inner",
        "inner": "printf 'inner %s %s\\n' \"$DATABASE_URL\" \"$NODE_ENV\"",
    }));
    temp.write(".env", "DATABASE_URL=base\n");
    temp.write(".env.test", "DATABASE_URL=test\n");
    temp
}

#[test]
fn a_nested_launch_loads_the_file_for_its_own_mode() {
    let temp = nested_mode_project();

    let output = temp.bento(&["run", "outer"]).output().unwrap();

    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(stdout(&output), "outer base\ninner test test\n");
}

#[test]
fn an_exported_value_wins_at_both_levels() {
    let temp = nested_mode_project();

    let output = temp.bento(&["run", "outer"]).env("DATABASE_URL", "mine").output().unwrap();

    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(stdout(&output), "outer mine\ninner mine test\n");
}

#[test]
fn a_malformed_environment_file_stops_the_launch_without_showing_the_value() {
    let temp = TempProject::with_tasks(json!({ "show": "echo ran > ran.log" }));
    temp.write(".env", "PORT=3000\nSECRET=\"hunter2\n");

    let output = temp.bento(&["run", "show"]).output().unwrap();

    assert_eq!(output.status.code(), Some(1));
    let message = stderr(&output);
    assert!(message.contains(&format!("{}:2", temp.path(".env").display())), "{message}");
    assert!(!message.contains("hunter2"), "{message}");
    assert!(!temp.exists("ran.log"));
}

#[test]
fn a_variable_that_is_not_utf8_is_reported_by_name() {
    let temp = TempProject::with_tasks(json!({ "show": "echo ran > ran.log" }));

    let output =
        temp.bento(&["run", "show"]).env("LATIN1", OsStr::from_bytes(b"caf\xe9")).output().unwrap();

    assert_eq!(output.status.code(), Some(1));
    assert!(stderr(&output).contains("`LATIN1`"), "{}", stderr(&output));
    assert!(!temp.exists("ran.log"));
}
