//! Grant flags and the inherited ceiling as `bento run` applies them. How each flag resolves
//! into a grant is tested beside `GrantFlags` itself.

mod support;

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::process::{Command, Output};

use serde_json::json;

use bento_host::{Grants, Scope};
use support::{TempProject, stderr, stdout};

/// A project whose `show.mjs` prints `ran` and the `GREETING` it sees.
fn project() -> TempProject {
    let temp = TempProject::new(json!({ "type": "module" }));
    temp.write("show.mjs", "console.log('ran', process.env.GREETING);\n");
    temp.write("data/.keep", "");
    temp
}

fn paths(paths: &[PathBuf]) -> Scope<PathBuf> {
    Scope::only(paths.iter().cloned())
}

/// `bento <arguments>` in `temp` under the inherited `ceiling`.
fn under_ceiling(temp: &TempProject, ceiling: &Grants, arguments: &[&str]) -> Command {
    let mut command = temp.bento(arguments);
    command.env("BENTO_GRANT_CEILING", ceiling.to_ceiling_value().unwrap());
    command
}

/// Asserts that the launch behind `output` stopped before its script ran, with a report
/// containing `named`, and returns the report.
fn assert_stopped_naming(output: &Output, named: &str) -> String {
    assert_eq!(output.status.code(), Some(1), "{}", stderr(output));
    assert!(stdout(output).is_empty(), "the script ran: {}", stdout(output));
    let message = stderr(output);
    assert!(message.contains(named), "{message}");
    message
}

/// Without grant flags a script cannot even be loaded, so running it at all shows the read
/// grant came from the ceiling, as did the variable it sees.
#[test]
fn a_launch_without_grant_flags_runs_with_the_inherited_ceiling() {
    let temp = project();
    let ceiling = Grants {
        read: paths(&[temp.root().to_owned()]),
        env: Scope::only(["GREETING".to_owned()]),
        ..Grants::none()
    };

    let output = under_ceiling(&temp, &ceiling, &["run", "show.mjs"])
        .env("GREETING", "inherited")
        .output()
        .unwrap();

    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(stdout(&output), "ran inherited\n");
}

#[test]
fn a_language_model_request_beyond_the_inherited_grant_fails_before_the_script_runs() {
    let temp = project();

    let output =
        under_ceiling(&temp, &Grants::none(), &["run", "--allow-language-model", "show.mjs"])
            .output()
            .unwrap();

    let message = assert_stopped_naming(&output, "--allow-language-model");
    assert!(message.contains("inherited language-model grant: none"), "{message}");
}

#[test]
fn a_wider_request_fails_before_the_script_runs_naming_the_flag_and_the_limit() {
    let temp = project();
    let ceiling = Grants { read: paths(&[temp.path("data")]), ..Grants::none() };

    let output =
        under_ceiling(&temp, &ceiling, &["run", "--allow-read=.", "show.mjs"]).output().unwrap();

    let message =
        assert_stopped_naming(&output, &format!("--allow-read={}", temp.root().display()));
    assert!(message.contains(&temp.path("data").display().to_string()), "{message}");
}

#[test]
fn a_whole_kind_beyond_the_inherited_grant_names_the_bare_flag() {
    let temp = project();
    let ceiling = Grants { read: paths(&[temp.root().to_owned()]), ..Grants::none() };

    let output =
        under_ceiling(&temp, &ceiling, &["run", "--allow-write", "show.mjs"]).output().unwrap();

    let message = assert_stopped_naming(&output, "--allow-write");
    assert!(message.contains("none"), "{message}");
}

#[test]
fn an_inherited_empty_allow_list_is_reported_as_no_grant() {
    let temp = project();
    let ceiling = Grants {
        read: paths(&[temp.root().to_owned()]),
        write: Scope::Only(BTreeSet::new()),
        ..Grants::none()
    };

    let output =
        under_ceiling(&temp, &ceiling, &["run", "--allow-write=.", "show.mjs"]).output().unwrap();

    let message = assert_stopped_naming(&output, "--allow-write");
    assert!(message.trim_end().ends_with("inherited write grant: none"), "{message}");
}

/// Every flag but the malformed read grant comes with a read grant, so a script that ran
/// despite the malformed flag would print.
#[test]
fn malformed_grant_flags_fail_before_the_script_runs() {
    let temp = project();
    let malformed: [(&[&str], &str); 6] = [
        (&["--allow-read="], "--allow-read"),
        (&["--allow-read=.", "--allow-net=localhost:http"], "--allow-net"),
        (&["--allow-read=.", "--allow-env=PORT,,NODE_ENV"], "--allow-env"),
        (&["--allow-read=.", "--allow-net=localhost/path"], "--allow-net"),
        (&["--allow-read=.", "--allow-net=[bogus]:18540"], "--allow-net"),
        (&["--allow-read=.", "--allow-net=white space"], "--allow-net"),
    ];

    for (flags, named) in malformed {
        let output = temp.bento(&[&["run"], flags, &["show.mjs"]].concat()).output().unwrap();

        assert_stopped_naming(&output, named);
    }
}

#[test]
fn a_grant_flag_given_twice_is_rejected_rather_than_merged() {
    let temp = project();

    let output =
        temp.bento(&["run", "--allow-read", "--allow-read=data", "show.mjs"]).output().unwrap();

    assert_stopped_naming(&output, "--allow-read");
}

#[test]
fn an_unreadable_inherited_ceiling_fails_closed() {
    let temp = project();

    let output = temp
        .bento(&["run", "--allow-read=.", "show.mjs"])
        .env("BENTO_GRANT_CEILING", "not a policy")
        .output()
        .unwrap();

    assert_stopped_naming(&output, "BENTO_GRANT_CEILING");
}
