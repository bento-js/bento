mod support;

use serde_json::json;

use bento_host::Grants;
use support::{SYSTEM_PATH, TempProject, stderr};

const RUNTIME_REQUIRED: &str = "script execution requires the runtime";

#[test]
fn an_unknown_target_is_an_error_and_never_a_shell_command() {
    let temp = TempProject::with_tasks(json!({ "dev": "echo dev > ran.log" }));

    for target in ["missing-name", "touch shell-ran"] {
        let output = temp.bento(&["run", target]).output().unwrap();

        assert_eq!(output.status.code(), Some(1), "{target}");
        let message = stderr(&output);
        assert!(message.contains(&format!("`{target}`")), "{message}");
        assert!(message.contains("neither a task"), "{message}");
    }
    assert!(!temp.exists("shell-ran"));
    assert!(!temp.exists("ran.log"));
}

#[test]
fn usage_errors_exit_with_status_1_and_help_with_0() {
    let temp = TempProject::with_tasks(json!({}));

    let missing_target = temp.bento(&["run", "--allow-read"]).output().unwrap();
    let help = temp.bento(&["run", "--help"]).output().unwrap();

    assert_eq!(missing_target.status.code(), Some(1));
    assert!(stderr(&missing_target).contains("TARGET"), "{}", stderr(&missing_target));
    assert_eq!(help.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&help.stdout).contains("--allow-addons"));
}

#[test]
fn a_task_name_wins_over_a_file_of_the_same_name() {
    let temp = TempProject::with_tasks(json!({ "build": "echo task > chosen.log" }));
    temp.write("build", "");

    let output = temp.bento(&["run", "build"]).output().unwrap();

    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(temp.read("chosen.log"), "task\n");
}

/// A project with a script and `node`, `deno`, and `bun` executables on `PATH` that record
/// whether anything tried to run them.
fn script_project() -> (TempProject, String) {
    let temp = TempProject::with_tasks(json!({}));
    temp.write("server.ts", "console.log('served');\n");
    for runtime in ["node", "deno", "bun"] {
        temp.write_executable(&format!("other-runtimes/{runtime}"), "#!/bin/sh\ntouch ran.log\n");
    }
    let search_path = format!("{}:{SYSTEM_PATH}", temp.path("other-runtimes").display());
    (temp, search_path)
}

#[test]
fn a_script_reports_that_execution_requires_the_runtime_without_trying_another() {
    let (temp, search_path) = script_project();

    for target in ["./server.ts", "server.ts"] {
        let output = temp
            .bento(&["run", "--allow-read=.", target, "--port", "3000"])
            .env("PATH", &search_path)
            .output()
            .unwrap();

        assert_eq!(output.status.code(), Some(1), "{target}");
        let message = stderr(&output);
        assert!(message.contains(RUNTIME_REQUIRED), "{message}");
        assert!(message.contains(&format!("`{target}`")), "{message}");
    }
    assert!(!temp.exists("ran.log"));
}

#[test]
fn a_watched_script_also_reports_that_execution_requires_the_runtime() {
    let (temp, search_path) = script_project();

    let output =
        temp.bento(&["run", "--watch", "server.ts"]).env("PATH", &search_path).output().unwrap();

    assert_eq!(output.status.code(), Some(1));
    assert!(stderr(&output).contains(RUNTIME_REQUIRED), "{}", stderr(&output));
    assert!(!temp.exists("ran.log"));
}

#[test]
fn script_grants_are_validated_first() {
    let (temp, _) = script_project();
    let ceiling = Grants::none().to_ceiling_value();

    let output = temp
        .bento(&["run", "--allow-net", "./server.ts"])
        .env("BENTO_GRANT_CEILING", ceiling)
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    let message = stderr(&output);
    assert!(message.contains("--allow-net") && !message.contains(RUNTIME_REQUIRED), "{message}");
}

#[test]
fn script_environment_files_are_loaded_first() {
    let (temp, _) = script_project();
    temp.write(".env", "PORT\n");

    let output = temp.bento(&["run", "./server.ts"]).output().unwrap();

    assert_eq!(output.status.code(), Some(1));
    let message = stderr(&output);
    assert!(message.contains(".env:1") && !message.contains(RUNTIME_REQUIRED), "{message}");
}

#[test]
fn watching_a_task_is_rejected_before_anything_runs() {
    let temp = TempProject::with_tasks(json!({ "dev": "echo dev > ran.log" }));

    let output = temp.bento(&["run", "--watch", "dev"]).output().unwrap();

    assert_eq!(output.status.code(), Some(1));
    let message = stderr(&output);
    assert!(message.contains("--watch") && message.contains("`dev`"), "{message}");
    assert!(!temp.exists("ran.log"));
}
