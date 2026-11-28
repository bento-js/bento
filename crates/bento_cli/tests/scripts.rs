mod support;

use std::process::Output;

use serde_json::json;

use support::{TempProject, bento_in, path_without_other_runtimes, stderr, stdout, unstyled};

fn project_with(script: &str, source: &str) -> TempProject {
    let temp = TempProject::new(json!({ "type": "module" }));
    temp.write(script, source);
    temp
}

/// `bento run <words>` in the project.
fn run_in(temp: &TempProject, words: &[&str]) -> Output {
    temp.bento(&[&["run"], words].concat()).output().unwrap()
}

#[test]
fn a_module_script_runs_and_prints() {
    let temp = project_with("hello.mjs", "console.log('hello from bento');\n");
    temp.write("hello.js", "import { argv } from 'node:process';\nconsole.log(argv.length > 1);\n");

    let module = run_in(&temp, &["hello.mjs"]);
    let package_module = run_in(&temp, &["hello.js"]);

    assert!(module.status.success(), "{}", stderr(&module));
    assert_eq!(stdout(&module), "hello from bento\n");
    assert!(package_module.status.success(), "{}", stderr(&package_module));
    assert_eq!(stdout(&package_module), "true\n");
}

#[test]
fn a_script_runs_without_node_bun_deno_or_pnpm_on_path() {
    let temp = project_with(
        "wait.mjs",
        "const word = await new Promise(resolve => setTimeout(() => resolve('awaited'), 10));\n\
         console.log(word);\n",
    );

    let output = temp
        .bento(&["run", "wait.mjs"])
        .env("PATH", path_without_other_runtimes())
        .output()
        .unwrap();

    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(stdout(&output), "awaited\n");
}

#[test]
fn process_exit_sets_bento_s_exit_status() {
    let temp = project_with("exit.mjs", "console.log('leaving');\nprocess.exit(3);\n");

    let output = run_in(&temp, &["exit.mjs"]);

    assert_eq!(output.status.code(), Some(3), "{}", stderr(&output));
    assert_eq!(stdout(&output), "leaving\n");
}

#[test]
fn an_uncaught_error_exits_1_and_reports_where_it_was_thrown() {
    let temp = project_with("boom.mjs", "const ready = true;\nthrow new Error('kaboom');\n");

    let output = run_in(&temp, &["boom.mjs"]);

    assert_eq!(output.status.code(), Some(1));
    let message = unstyled(&stderr(&output));
    assert!(message.contains("uncaught exception"), "{message}");
    assert!(message.contains("kaboom"), "{message}");
    assert!(message.contains("boom.mjs:2:7"), "{message}");
}

#[test]
fn a_permission_violation_exits_1_and_says_it_was_denied() {
    let temp = project_with(
        "note.mjs",
        "import fs from 'node:fs';\nfs.writeFileSync('note.txt', 'written');\nconsole.log('wrote');\n",
    );

    let output = run_in(&temp, &["--permission", "note.mjs"]);

    assert_eq!(output.status.code(), Some(1));
    let message = stderr(&output);
    assert!(message.contains("permission denied"), "{message}");
    assert!(message.contains("note.txt"), "{message}");
    assert!(message.contains("note.mjs:2:4") && message.contains("denied here"), "{message}");
    assert!(stdout(&output).is_empty(), "{}", stdout(&output));
    assert!(!temp.exists("note.txt"));
}

#[test]
fn words_after_the_script_are_its_arguments_unchanged() {
    let temp = project_with("args.mjs", "console.log(JSON.stringify(process.argv.slice(2)));\n");

    let output = run_in(&temp, &["args.mjs", "one", "--allow-net", "two words", "--"]);

    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(stdout(&output), "[\"one\",\"--allow-net\",\"two words\",\"--\"]\n");
}

#[test]
fn the_script_s_working_directory_is_where_bento_was_run() {
    let temp = project_with("cwd.mjs", "console.log(process.cwd());\n");
    temp.write("nested/.keep", "");

    let output = bento_in(&temp.path("nested"), &["run", "../cwd.mjs"]).output().unwrap();

    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(stdout(&output), format!("{}\n", temp.path("nested").display()));
}

#[test]
fn a_script_run_from_outside_its_project_resolves_the_project_s_packages() {
    let temp = project_with(
        "scripts/greet.mjs",
        "import { greeting } from 'greeter';\nconsole.log(greeting);\n",
    );
    temp.write(
        "node_modules/greeter/package.json",
        &json!({ "name": "greeter", "type": "module", "exports": "./index.js" }).to_string(),
    );
    temp.write("node_modules/greeter/index.js", "export const greeting = 'hello, project';\n");
    let elsewhere = tempfile::tempdir_in(temp.root().parent().unwrap()).unwrap();
    let script = temp.path("scripts/greet.mjs").display().to_string();

    let output = bento_in(elsewhere.path(), &["run", &script]).output().unwrap();

    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(stdout(&output), "hello, project\n");
}
