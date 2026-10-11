mod support;

use std::path::Path;
use std::process::Command;

use serde_json::json;

use bento_host::Grants;
use bento_task::{Cancellation, Plan, TaskError, TaskRunner};
use bento_workspace::{LaunchEnvironment, LaunchKind, Project};
use support::{SYSTEM_PATH, TempProject, vars};

#[test]
fn dependencies_run_once_in_declared_order_before_the_task() {
    let temp = TempProject::with_tasks(json!({
        "check:fix": { "dependsOn": ["fmt", "lint", "typecheck"] },
        "fmt": "echo fmt >> log.txt",
        "lint": { "dependsOn": ["fmt"], "command": "echo lint >> log.txt" },
        "typecheck": ["echo typecheck one >> log.txt", "echo typecheck two >> log.txt"],
    }));

    temp.run("check:fix", &[]).unwrap();

    assert_eq!(temp.read("log.txt"), "fmt\nlint\ntypecheck one\ntypecheck two\n");
}

#[test]
fn command_arrays_stop_at_the_first_failure() {
    let temp = TempProject::with_tasks(json!({
        "check": ["echo one >> log.txt", "exit 3", "echo three >> log.txt"],
    }));

    let error = temp.run("check", &[]).unwrap_err();

    assert!(
        matches!(&error, TaskError::Failed { task, command, .. } if task == "check" && command == "exit 3"),
        "{error:?}"
    );
    assert_eq!(error.exit_code(), Some(3));
    let message = error.to_string();
    assert!(message.contains("check") && message.contains("exit 3") && message.contains('3'));
    assert_eq!(temp.read("log.txt"), "one\n");
}

#[test]
fn failed_dependency_prevents_its_dependents() {
    let temp = TempProject::with_tasks(json!({
        "build": { "dependsOn": ["codegen"], "command": "touch built" },
        "codegen": "exit 7",
    }));

    let error = temp.run("build", &[]).unwrap_err();

    assert!(matches!(&error, TaskError::Failed { task, .. } if task == "codegen"), "{error:?}");
    assert_eq!(error.exit_code(), Some(7));
    assert!(!temp.path("built").exists());
}

#[test]
fn forwarded_arguments_reach_only_the_selected_command_intact() {
    let temp = TempProject::with_tasks(json!({
        "prepare": "show-args prepare",
        "show": { "dependsOn": ["prepare"], "command": "show-args show" },
    }));
    temp.write_executable(
        "node_modules/.bin/show-args",
        "#!/bin/sh\nfor argument in \"$@\"; do printf '[%s]\\n' \"$argument\"; done > \"args-$1.txt\"\n",
    );

    temp.run("show", &["hello world", "it's \"quoted\"", "$HOME", "a;b", ""]).unwrap();

    assert_eq!(
        temp.read("args-show.txt"),
        "[show]\n[hello world]\n[it's \"quoted\"]\n[$HOME]\n[a;b]\n[]\n"
    );
    assert_eq!(temp.read("args-prepare.txt"), "[prepare]\n");
}

#[test]
fn commands_run_in_the_project_root_with_local_bins_ahead_of_the_inherited_path() {
    let temp = TempProject::with_tasks(json!({
        "where": "pwd > cwd.txt && printf %s \"$PATH\" > path.txt && local-tool",
    }));
    temp.write_executable("node_modules/.bin/local-tool", "#!/bin/sh\necho local > tool.txt\n");

    temp.run("where", &[]).unwrap();

    assert_eq!(temp.read("cwd.txt").trim_end(), temp.root().display().to_string());
    let expected_tail = format!(
        ":{}:{}:{SYSTEM_PATH}",
        temp.path("node_modules/.bin").display(),
        temp.bento_directory().display()
    );
    let path = temp.read("path.txt");
    assert!(path.ends_with(&expected_tail), "{path}");
    assert_eq!(temp.read("tool.txt"), "local\n");
}

#[test]
fn bento_in_a_command_is_the_running_executable_even_with_a_local_bento_bin() {
    let temp = TempProject::with_tasks(json!({ "nested": "bento run other" }));
    temp.write_executable("node_modules/.bin/bento", "#!/bin/sh\necho local > bento.txt\n");

    temp.run("nested", &[]).unwrap();

    assert_eq!(temp.read("bento.txt"), "running run other\n");
}

#[test]
fn inline_assignments_and_shell_operators_behave_as_in_the_shell() {
    let temp = TempProject::with_tasks(json!({
        "test": "NODE_ENV=test sh -c 'printf %s \"$NODE_ENV\"' > inline.txt",
        "operators": "printf a > ops.txt && printf b >> ops.txt; false || printf c >> ops.txt; printf 'x\\n' | tr x y > piped.txt",
        "launch": "printf %s \"$NODE_ENV:$DATABASE_URL\" > launch.txt",
    }));
    temp.write(".env", "NODE_ENV=production\nDATABASE_URL=file.db\n");

    temp.run("test", &[]).unwrap();
    temp.run("operators", &[]).unwrap();
    temp.run("launch", &[]).unwrap();

    assert_eq!(temp.read("inline.txt"), "test");
    assert_eq!(temp.read("ops.txt"), "abc");
    assert_eq!(temp.read("piped.txt"), "y\n");
    assert_eq!(temp.read("launch.txt"), "production:file.db");
}

const OUTPUT_PROBE: &str = "BENTO_TASK_OUTPUT_PROBE";

/// Runs a task inside a separate test process so its real stdout and stderr can be inspected.
#[test]
fn child_output_passes_through_to_the_caller() {
    if let Some(root) = std::env::var_os(OUTPUT_PROBE) {
        run_speak_task(Path::new(&root));
        return;
    }
    let temp = TempProject::with_tasks(json!({
        "speak": "printf 'task-stdout\\n'; printf 'task-stderr\\n' >&2",
    }));

    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "child_output_passes_through_to_the_caller", "--nocapture"])
        .env(OUTPUT_PROBE, temp.root())
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stdout}\n{stderr}");
    assert!(stdout.contains("task-stdout") && !stdout.contains("task-stderr"), "{stdout}");
    assert!(stderr.contains("task-stderr") && !stderr.contains("task-stdout"), "{stderr}");
}

fn run_speak_task(root: &Path) {
    let project = Project::for_directory(root).unwrap();
    let plan = Plan::new(project.manifest(), "speak", &[]).unwrap();
    let launch =
        LaunchEnvironment::load(root, vars(&[("PATH", SYSTEM_PATH)]), LaunchKind::Run).unwrap();
    let runner = TaskRunner::new(&project, &launch, &Grants::none(), &root.join("bento")).unwrap();
    runner.run(&plan, &Cancellation::new()).unwrap();
}
