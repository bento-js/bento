mod support;

use serde_json::json;

use support::{TempProject, stderr, stdout};

/// The guest book's task shapes, with each tool replaced by a line appended to `order.log`.
fn guest_book_tasks() -> TempProject {
    TempProject::new(json!({
        "scripts": {
            "predev": "echo predev >> order.log",
            "dev": "echo dev-script >> order.log",
        },
        "tasks": {
            "dev": { "dependsOn": ["db:migrate"], "command": "echo dev >> order.log" },
            "fmt": "echo fmt >> order.log",
            "lint": "echo lint >> order.log",
            "typecheck": "echo typecheck >> order.log",
            "check": ["echo check-fmt >> order.log", "echo check-lint >> order.log"],
            "check:fix": { "dependsOn": ["fmt", "lint", "typecheck"] },
            "db:migrate": "echo db:migrate >> order.log",
        },
    }))
}

#[test]
fn check_fix_runs_its_dependencies_in_declared_order() {
    let temp = guest_book_tasks();

    let output = temp.bento(&["run", "check:fix"]).output().unwrap();

    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(temp.read("order.log"), "fmt\nlint\ntypecheck\n");
}

#[test]
fn dev_migrates_once_and_never_runs_the_predev_script() {
    let temp = guest_book_tasks();

    let output = temp.bento(&["run", "dev"]).output().unwrap();

    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(temp.read("order.log"), "db:migrate\ndev\n");
}

#[test]
fn command_arrays_run_in_order() {
    let temp = guest_book_tasks();

    let output = temp.bento(&["run", "check"]).output().unwrap();

    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(temp.read("order.log"), "check-fmt\ncheck-lint\n");
}

#[test]
fn arguments_after_the_separator_reach_the_task_command_intact() {
    let temp = TempProject::with_tasks(json!({ "show": "printf '<%s>\\n'" }));

    let output = temp.bento(&["run", "show", "--", "a b", "c\"d", "--flag"]).output().unwrap();

    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(stdout(&output), "<a b>\n<c\"d>\n<--flag>\n");
}

#[test]
fn arguments_for_a_dependency_only_task_fail_without_running_anything() {
    let temp = guest_book_tasks();

    let output = temp.bento(&["run", "check:fix", "--", "a b"]).output().unwrap();

    assert_eq!(output.status.code(), Some(1));
    let message = stderr(&output);
    assert!(message.contains("`check:fix`") && message.contains("no command"), "{message}");
    assert!(!temp.exists("order.log"));
}

#[test]
fn task_arguments_without_the_separator_fail_without_running_anything() {
    let temp = TempProject::with_tasks(json!({ "show": "printf '<%s>\\n' > shown.log" }));

    let output = temp.bento(&["run", "show", "a", "--watch"]).output().unwrap();

    assert_eq!(output.status.code(), Some(1));
    let message = stderr(&output);
    assert!(message.contains("bento run show -- "), "{message}");
    assert!(!temp.exists("shown.log"));
}

#[test]
fn a_failing_dependency_stops_the_run_with_its_exit_code() {
    let temp = TempProject::with_tasks(json!({
        "migrate": "echo migrate >> order.log; exit 3",
        "serve": { "dependsOn": ["migrate"], "command": "echo serve >> order.log" },
    }));

    let output = temp.bento(&["run", "serve"]).output().unwrap();

    assert_eq!(output.status.code(), Some(3));
    let message = stderr(&output);
    assert!(message.contains("`migrate`"), "{message}");
    assert!(message.contains("`echo migrate >> order.log; exit 3`"), "{message}");
    assert_eq!(temp.read("order.log"), "migrate\n");
}

#[test]
fn a_command_killed_by_a_signal_exits_like_a_shell_reports_it() {
    let temp = TempProject::with_tasks(json!({ "crash": "kill -KILL $$" }));

    let output = temp.bento(&["run", "crash"]).output().unwrap();

    assert_eq!(output.status.code(), Some(128 + 9));
    assert!(stderr(&output).contains("`crash`"), "{}", stderr(&output));
}

#[test]
fn task_commands_find_local_tools_by_name() {
    let temp = TempProject::with_tasks(json!({ "which": "command -v local-tool" }));
    temp.write_executable("node_modules/.bin/local-tool", "#!/bin/sh\n");

    let output = temp.bento(&["run", "which"]).output().unwrap();

    assert!(output.status.success(), "{}", stderr(&output));
    let found = temp.path("node_modules/.bin/local-tool").display().to_string();
    assert_eq!(stdout(&output), format!("{found}\n"));
}

#[test]
fn nested_bento_is_this_bento_even_when_the_project_has_a_local_one() {
    let temp = TempProject::with_tasks(json!({
        "outer": "bento run inner",
        "inner": "echo inner >> ran.log",
    }));
    temp.write_executable("node_modules/.bin/bento", "#!/bin/sh\necho local-bento >> ran.log\n");

    let output = temp.bento(&["run", "outer"]).output().unwrap();

    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(temp.read("ran.log"), "inner\n");
}
