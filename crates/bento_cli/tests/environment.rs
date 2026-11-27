mod support;

use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;

use serde_json::json;

use support::{TempProject, stderr, stdout};

/// A project whose `show.mjs` prints the variables `A` to `D`.
fn showing_project() -> TempProject {
    let temp = TempProject::new(json!({ "type": "module" }));
    temp.write("show.mjs", "const { A, B, C, D } = process.env;\nconsole.log(A, B, C, D);\n");
    temp
}

#[test]
fn environment_files_reach_the_script_in_precedence_order() {
    let temp = showing_project();
    temp.write(".env", "A=base\nB=base\nC=base\nD=base\n");
    temp.write(".env.test", "B=test\nC=test\nD=test\n");
    temp.write(".env.local", "C=local\nD=local\n");

    let output = temp
        .bento(&["run", "--allow-read=.", "--allow-env=A,B,C,D", "show.mjs"])
        .env("NODE_ENV", "test")
        .env("D", "exported")
        .output()
        .unwrap();

    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(stdout(&output), "base test local exported\n");
}

#[test]
fn a_malformed_environment_file_stops_the_launch_without_showing_the_value() {
    let temp = showing_project();
    temp.write(".env", "PORT=3000\nSECRET=\"hunter2\n");

    let output = temp.bento(&["run", "--allow-read=.", "show.mjs"]).output().unwrap();

    assert_eq!(output.status.code(), Some(1));
    let message = stderr(&output);
    assert!(message.contains(&format!("{}:2", temp.path(".env").display())), "{message}");
    assert!(!message.contains("hunter2"), "{message}");
    assert!(stdout(&output).is_empty(), "{}", stdout(&output));
}

#[test]
fn a_variable_that_is_not_utf8_is_reported_by_name() {
    let temp = showing_project();

    let output = temp
        .bento(&["run", "--allow-read=.", "show.mjs"])
        .env("LATIN1", OsStr::from_bytes(b"caf\xe9"))
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    assert!(stderr(&output).contains("`LATIN1`"), "{}", stderr(&output));
    assert!(stdout(&output).is_empty(), "{}", stdout(&output));
}
