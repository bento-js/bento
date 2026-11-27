mod support;

use std::process::Output;

use serde_json::json;

use bento_host::Grants;
use support::{SYSTEM_PATH, TempProject, stderr, stdout};

/// A project that declares `dev` and `build` both in the top-level `tasks` field earlier
/// releases ran and as package scripts. Each of them records that it ran in `ran.log`.
fn project_declaring_commands() -> TempProject {
    let command = json!("echo ran > ran.log");
    TempProject::new(json!({
        "tasks": { "dev": command, "build": command },
        "scripts": { "dev": command, "build": command },
    }))
}

/// Asserts that `output` is the report that no script file is at `relative` in `temp`.
fn assert_no_script_file(temp: &TempProject, relative: &str, output: &Output) {
    assert_eq!(output.status.code(), Some(1), "{relative}: {}", stderr(output));
    let expected = format!("error: no script file at {}\n", temp.path(relative).display());
    assert_eq!(stderr(output), expected);
    assert!(stdout(output).is_empty(), "{}", stdout(output));
}

#[test]
fn a_script_path_never_names_a_task_a_package_script_or_a_shell_command() {
    let temp = project_declaring_commands();

    for script in ["dev", "touch shell-ran"] {
        let output = temp.bento(&["run", "--allow-read=.", script]).output().unwrap();

        assert_no_script_file(&temp, script, &output);
    }
    assert!(!temp.exists("shell-ran"));
    assert!(!temp.exists("ran.log"));
}

#[test]
fn a_file_named_like_a_task_runs_as_the_script() {
    let temp = project_declaring_commands();
    temp.write("build", "console.log('the build file ran');\n");

    let output = temp.bento(&["run", "--allow-read=.", "build"]).output().unwrap();

    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(stdout(&output), "the build file ran\n");
    assert!(!temp.exists("ran.log"));
}

#[test]
fn a_missing_file_or_a_directory_is_reported_before_environment_files_load() {
    let temp = TempProject::new(json!({}));
    temp.write(".env", "PORT\n");
    temp.write("app/.keep", "");

    for script in ["missing.mjs", "app"] {
        let output = temp.bento(&["run", "--allow-read=.", script]).output().unwrap();

        assert_no_script_file(&temp, script, &output);
    }
}

#[test]
fn watch_takes_a_script_path_too() {
    let temp = project_declaring_commands();

    let output = temp.bento(&["run", "--watch", "--allow-read=.", "dev"]).output().unwrap();

    assert_no_script_file(&temp, "dev", &output);
    assert!(!temp.exists("ran.log"));
}

#[test]
fn usage_errors_exit_with_status_1_and_help_with_0() {
    let temp = TempProject::new(json!({}));

    let missing_script = temp.bento(&["run", "--allow-read"]).output().unwrap();
    let help = temp.bento(&["run", "--help"]).output().unwrap();

    assert_eq!(missing_script.status.code(), Some(1));
    assert!(stderr(&missing_script).contains("<SCRIPT>"), "{}", stderr(&missing_script));
    assert_eq!(help.status.code(), Some(0));
    assert!(stdout(&help).contains("--allow-addons"));
}

#[test]
fn help_describes_running_source_files_and_never_tasks() {
    let temp = TempProject::new(json!({}));

    let bento_help = stdout(&temp.bento(&["--help"]).output().unwrap());
    let run_help = stdout(&temp.bento(&["run", "--help"]).output().unwrap());

    assert!(bento_help.contains("Run TypeScript and JavaScript applications"), "{bento_help}");
    assert!(bento_help.contains("Run a JavaScript or TypeScript source file"), "{bento_help}");
    assert!(run_help.contains("<SCRIPT>"), "{run_help}");
    for help in [&bento_help, &run_help] {
        assert!(!help.to_lowercase().contains("task"), "{help}");
    }
}

/// A project with a script and `node`, `deno`, and `bun` executables on `PATH` that record
/// whether anything tried to run them.
fn script_project() -> (TempProject, String) {
    let temp = TempProject::new(json!({}));
    temp.write("server.mjs", "console.log('served', JSON.stringify(process.argv.slice(2)));\n");
    for runtime in ["node", "deno", "bun"] {
        temp.write_executable(&format!("other-runtimes/{runtime}"), "#!/bin/sh\ntouch ran.log\n");
    }
    let search_path = format!("{}:{SYSTEM_PATH}", temp.path("other-runtimes").display());
    (temp, search_path)
}

#[test]
fn a_script_runs_in_bento_without_trying_another_runtime() {
    let (temp, search_path) = script_project();

    for script in ["./server.mjs", "server.mjs"] {
        let output = temp
            .bento(&["run", "--allow-read=.", script, "--port", "3000"])
            .env("PATH", &search_path)
            .output()
            .unwrap();

        assert!(output.status.success(), "{script}: {}", stderr(&output));
        assert_eq!(stdout(&output), "served [\"--port\",\"3000\"]\n", "{script}");
    }
    assert!(!temp.exists("ran.log"));
}

#[test]
fn script_grants_are_validated_before_the_script_runs() {
    let (temp, _) = script_project();
    let ceiling = Grants::none().to_ceiling_value().unwrap();

    let output = temp
        .bento(&["run", "--allow-net", "./server.mjs"])
        .env("BENTO_GRANT_CEILING", ceiling)
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    assert!(stderr(&output).contains("--allow-net"), "{}", stderr(&output));
    assert!(stdout(&output).is_empty(), "{}", stdout(&output));
}

/// Neither the script nor `package.json` is looked at before the grant flags and the inherited
/// ceiling are known to be valid, so their errors are reported even when both are broken too.
#[test]
fn grant_flags_and_the_ceiling_are_checked_before_the_script_and_package_json() {
    let temp = TempProject::new(json!({}));
    temp.write("package.json", "{ not json");
    let none = Grants::none().to_ceiling_value().unwrap();
    let cases = [
        (&["run", "missing.mjs"][..], Some("not a policy"), "BENTO_GRANT_CEILING"),
        (&["run", "--allow-net", "missing.mjs"][..], Some(none.as_str()), "--allow-net"),
        (&["run", "--allow-net=localhost/path", "missing.mjs"][..], None, "--allow-net"),
    ];

    for (arguments, ceiling, named) in cases {
        let mut command = temp.bento(arguments);
        if let Some(ceiling) = ceiling {
            command.env("BENTO_GRANT_CEILING", ceiling);
        }

        let output = command.output().unwrap();

        assert_eq!(output.status.code(), Some(1), "{arguments:?}");
        let message = stderr(&output);
        assert!(message.contains(named), "{arguments:?}: {message}");
        assert!(!message.contains("no script file"), "{arguments:?}: {message}");
        assert!(!message.contains("package.json"), "{arguments:?}: {message}");
    }
}

#[test]
fn script_environment_files_are_loaded_before_the_script_runs() {
    let (temp, _) = script_project();
    temp.write(".env", "PORT\n");

    let output = temp.bento(&["run", "./server.mjs"]).output().unwrap();

    assert_eq!(output.status.code(), Some(1));
    assert!(stderr(&output).contains(".env:1"), "{}", stderr(&output));
    assert!(stdout(&output).is_empty(), "{}", stdout(&output));
}
