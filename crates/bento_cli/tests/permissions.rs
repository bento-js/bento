//! What a launch may do: everything without `--permission`, only what its flags allow with
//! it, and never more than the ceiling a launch inherits from the task that started it.

mod support;

use std::path::{Path, PathBuf};
use std::process::Output;
use std::time::Duration;

use serde_json::json;

use bento_host::{CEILING_ENV, EnvPattern, PathPattern, PermissionSet, Permissions, Scope};
use support::running::Background;
use support::{TempProject, stderr, stdout};

/// A project whose `ceiling` task records the permissions its launch hands to task commands,
/// with tasks that start nested launches.
fn project() -> TempProject {
    let temp = TempProject::with_tasks(json!({
        "ceiling": "printf '%s' \"$BENTO_PERMISSION_CEILING\" > ceiling.json",
        "nested": "bento run ceiling",
        "narrowed": "bento run --permission --allow-fs-read=data/inner ceiling",
        "widened": "bento run --permission --allow-fs-read=. ceiling",
        "net-widened": "bento run --permission --allow-net ceiling",
        "nested-script": "bento run --permission scripts/hello.mjs",
    }));
    temp.write("data/inner/entry.txt", "");
    temp.write("scripts/hello.mjs", "console.log('hello from the nested script');\n");
    temp
}

fn recorded(temp: &TempProject) -> Permissions {
    Permissions::from_ceiling_value(&temp.read("ceiling.json")).unwrap()
}

fn beneath(directory: &Path) -> PathPattern {
    PathPattern::Prefix(PathBuf::from(format!("{}/", directory.display())))
}

fn succeeded(output: &Output) {
    assert!(output.status.success(), "{}{}", stdout(output), stderr(output));
}

/// Asserts that `output` is a failure that stopped before anything ran, and returns its
/// report.
fn failed_before_running(temp: &TempProject, output: &Output) -> String {
    assert_eq!(output.status.code(), Some(1), "{}", stderr(output));
    assert!(!temp.exists("ceiling.json"));
    stderr(output)
}

#[test]
fn a_launch_without_permission_is_unrestricted() {
    let temp = project();

    succeeded(&temp.bento(&["run", "ceiling"]).output().unwrap());

    assert_eq!(recorded(&temp), Permissions::unrestricted());
}

#[test]
fn an_unrestricted_script_reads_writes_listens_and_sees_the_environment() {
    let temp = project();
    let outside = tempfile::tempdir().unwrap();
    let data = outside.path().join("data.txt");
    std::fs::write(&data, "outside").unwrap();
    temp.write(
        "unrestricted.mjs",
        &format!(
            "import fs from 'node:fs';\n\
             import net from 'node:net';\n\
             const text = fs.readFileSync({data:?}, 'utf8');\n\
             fs.writeFileSync('written.txt', text.toUpperCase());\n\
             const server = net.createServer();\n\
             server.listen(0, '127.0.0.1', () => {{\n\
               server.close();\n\
               console.log(text, process.env.SECRET, process.permission === undefined);\n\
             }});\n"
        ),
    );

    let output = temp.bento(&["run", "unrestricted.mjs"]).env("SECRET", "exported").output();

    let output = output.unwrap();
    succeeded(&output);
    assert_eq!(stdout(&output), "outside exported true\n");
    assert_eq!(temp.read("written.txt"), "OUTSIDE");
}

#[test]
fn a_restricted_script_loads_itself_but_is_denied_a_file_it_was_not_allowed() {
    let temp = project();
    temp.write("secret.txt", "secret");
    temp.write(
        "reader.mjs",
        "import fs from 'node:fs';\nconsole.log('started');\nfs.readFileSync('secret.txt');\n",
    );

    let output = temp.bento(&["run", "--permission", "reader.mjs"]).output().unwrap();

    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    assert_eq!(stdout(&output), "started\n");
    let message = stderr(&output);
    assert!(message.contains("permission denied"), "{message}");
    assert!(message.contains("reader.mjs:3"), "{message}");
}

#[test]
fn only_the_entry_is_readable_without_a_flag() {
    let temp = project();
    temp.write("main.mjs", "import { word } from './word.mjs';\nconsole.log(word);\n");
    temp.write("word.mjs", "export const word = 'imported';\n");

    let denied = temp.bento(&["run", "--permission", "main.mjs"]).output().unwrap();
    let allowed =
        temp.bento(&["run", "--permission", "--allow-fs-read=.", "main.mjs"]).output().unwrap();

    assert_eq!(denied.status.code(), Some(1), "{}", stderr(&denied));
    assert!(stderr(&denied).contains("permission denied"), "{}", stderr(&denied));
    assert_eq!(stdout(&denied), "");
    succeeded(&allowed);
    assert_eq!(stdout(&allowed), "imported\n");
}

#[test]
fn a_nested_launch_without_permission_holds_exactly_its_ceiling() {
    let temp = project();
    let flags = ["--permission", "--allow-fs-read=data", "--allow-net", "--allow-env=PORT"];

    let output = temp.bento(&[&["run"], &flags[..], &["nested"]].concat()).output().unwrap();

    succeeded(&output);
    let expected = PermissionSet {
        fs_read: Scope::only([beneath(&temp.path("data"))]),
        net: true,
        env: Scope::only([EnvPattern::parse("PORT").unwrap()]),
        ..PermissionSet::default()
    };
    assert_eq!(recorded(&temp), Permissions::restricted(expected));
}

#[test]
fn a_nested_launch_under_an_unrestricted_ceiling_is_unrestricted() {
    let temp = project();

    succeeded(&temp.bento(&["run", "nested"]).output().unwrap());

    assert_eq!(recorded(&temp), Permissions::unrestricted());
}

#[test]
fn a_nested_launch_with_permission_holds_only_what_its_own_flags_allow() {
    let temp = project();
    let flags = ["--permission", "--allow-fs-read=data", "--allow-net"];

    let output = temp.bento(&[&["run"], &flags[..], &["narrowed"]].concat()).output().unwrap();

    succeeded(&output);
    let expected = PermissionSet {
        fs_read: Scope::only([beneath(&temp.path("data/inner"))]),
        ..PermissionSet::default()
    };
    assert_eq!(recorded(&temp), Permissions::restricted(expected));
}

#[test]
fn a_nested_flag_beyond_the_ceiling_fails_before_its_task_runs() {
    let temp = project();

    let output =
        temp.bento(&["run", "--permission", "--allow-fs-read=data", "widened"]).output().unwrap();

    let message = failed_before_running(&temp, &output);
    let excess = format!(
        "error: --allow-fs-read={}/* asks for more than the inherited fs-read permission: {}/data/*\n",
        temp.root().display(),
        temp.root().display(),
    );
    assert!(message.contains(&excess), "{message}");
    assert!(message.contains("task `widened` failed"), "{message}");
}

#[test]
fn a_nested_boolean_flag_beyond_the_ceiling_names_the_flag_alone() {
    let temp = project();

    let output = temp.bento(&["run", "--permission", "net-widened"]).output().unwrap();

    let message = failed_before_running(&temp, &output);
    assert!(
        message
            .contains("error: --allow-net asks for more than the inherited net permission: none\n"),
        "{message}"
    );
}

#[test]
fn an_inherited_ceiling_bounds_a_script_launch_before_the_script_runs() {
    let temp = project();
    let ceiling = Permissions::restricted(PermissionSet {
        fs_write: Scope::only([beneath(&temp.path("data"))]),
        ..PermissionSet::default()
    });

    let output = temp
        .bento(&["run", "--permission", "--allow-fs-write=*", "scripts/hello.mjs"])
        .env(CEILING_ENV, ceiling.to_ceiling_value().unwrap())
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    assert_eq!(stdout(&output), "");
    let excess = format!(
        "error: --allow-fs-write=* asks for more than the inherited fs-write permission: {}/data/*\n",
        temp.root().display()
    );
    assert_eq!(stderr(&output), excess);
}

#[test]
fn an_unreadable_inherited_ceiling_fails_closed() {
    let temp = project();

    let output =
        temp.bento(&["run", "ceiling"]).env(CEILING_ENV, "not permissions").output().unwrap();

    let message = failed_before_running(&temp, &output);
    assert!(message.contains(CEILING_ENV), "{message}");
}

#[test]
fn a_nested_script_is_readable_only_when_the_ceiling_allows_reading_it() {
    let temp = project();
    let restricted = |read: &str| {
        let flag = format!("--allow-fs-read={read}");
        temp.bento(&["run", "--permission", &flag, "nested-script"]).output().unwrap()
    };

    let covered = restricted("scripts");
    let unrestricted = temp.bento(&["run", "nested-script"]).output().unwrap();
    let uncovered = restricted("data");

    for output in [&covered, &unrestricted] {
        succeeded(output);
        assert_eq!(stdout(output), "hello from the nested script\n");
    }
    assert_eq!(uncovered.status.code(), Some(1), "{}", stderr(&uncovered));
    assert_eq!(stdout(&uncovered), "");
    let message = stderr(&uncovered);
    assert!(message.contains("permission denied"), "{message}");
    assert!(message.contains("hello.mjs"), "{message}");
}

/// Prints `GREETING` and `OTHER` as the script sees them.
const SHOW_VARIABLES: &str =
    "console.log(process.env.GREETING, process.env.OTHER, 'OTHER' in process.env);\n";

#[test]
fn a_restricted_launch_keeps_environment_file_values_over_inherited_ones_it_removes() {
    let temp = project();
    temp.write(".env", "GREETING=from-file\n");
    temp.write("show.mjs", SHOW_VARIABLES);
    let run = |flags: &[&str]| {
        let arguments = [&["run"], flags, &["show.mjs"]].concat();
        let mut command = temp.bento(&arguments);
        let output = command.env("GREETING", "exported").env("OTHER", "exported").output();
        let output = output.unwrap();
        succeeded(&output);
        stdout(&output)
    };

    assert_eq!(run(&["--permission"]), "from-file undefined false\n");
    assert_eq!(run(&["--permission", "--allow-env=GREETING,OTHER"]), "exported exported true\n");
    assert_eq!(run(&[]), "exported exported true\n");
}

#[test]
fn a_restricted_launch_removes_an_inherited_node_env() {
    let temp = project();
    temp.write("mode.mjs", "console.log(`mode ${process.env.NODE_ENV}`);\n");

    let output = temp
        .bento(&["run", "--permission", "mode.mjs"])
        .env("NODE_ENV", "production")
        .output()
        .unwrap();

    succeeded(&output);
    assert_eq!(stdout(&output), "mode undefined\n");
}

#[test]
fn a_restricted_watch_launch_keeps_the_development_default() {
    let temp = project();
    temp.write("mode.mjs", "console.log(`mode ${process.env.NODE_ENV}`);\n");
    let mut command = temp.bento(&["run", "--watch", "--permission", "mode.mjs"]);
    command.env_remove("NODE_ENV");

    let mut bento = Background::spawn(command);

    bento.wait_for_stdout("mode development", 1);
    bento.wait_for_stderr("waiting for changes");
    bento.signal(libc::SIGINT);
    assert_eq!(bento.wait(Duration::from_secs(5)).code(), Some(130));
}
