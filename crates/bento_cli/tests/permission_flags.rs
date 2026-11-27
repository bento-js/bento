//! What `bento run` accepts as permission flags, what each asks for, and how it reports the
//! ones it rejects. A task records the permissions its launch hands to nested launches.

mod support;

use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::process::Output;

use serde_json::json;

use bento_host::{EnvPattern, PathPattern, PermissionSet, Permissions, Scope};
use support::{TempProject, bento_in, stderr, stdout};

const COMMA_WARNING: &str = " CLI flag has changed. Passing a comma-separated list of paths is no longer valid. Documentation can be found at https://nodejs.org/api/permissions.html#file-system-permissions";

const BOOLEAN_FLAGS: [&str; 10] = [
    "--allow-net",
    "--allow-child-process",
    "--allow-worker",
    "--allow-addons",
    "--allow-wasi",
    "--allow-ffi",
    "--allow-inspector",
    "--allow-openssl-store",
    "--allow-fs-vfs",
    "--allow-language-model",
];

/// A project whose `ceiling` task records the permissions its launch hands to task commands.
fn project() -> TempProject {
    let temp = TempProject::with_tasks(json!({
        "ceiling": "printf '%s' \"$BENTO_PERMISSION_CEILING\" > ceiling.json",
    }));
    temp.write("data/inner/entry.txt", "");
    temp.write("db/.keep", "");
    temp.write("show.mjs", "console.log('ran');\n");
    temp
}

fn recorded(temp: &TempProject) -> Permissions {
    Permissions::from_ceiling_value(&temp.read("ceiling.json")).unwrap()
}

/// Runs the `ceiling` task restricted by `flags` and returns what its launch held.
fn restricted_by(temp: &TempProject, flags: &[&str]) -> Permissions {
    let output = temp.bento(&[&["run", "--permission"], flags, &["ceiling"]].concat()).output();
    let output = output.unwrap();
    assert!(output.status.success(), "{flags:?}: {}", stderr(&output));
    recorded(temp)
}

/// Runs `bento <arguments>` and asserts that it failed with status 1 before running anything.
fn rejected(temp: &TempProject, arguments: &[&str]) -> String {
    let output: Output = temp.bento(arguments).output().unwrap();
    assert_eq!(output.status.code(), Some(1), "{arguments:?}: {}", stderr(&output));
    assert!(!temp.exists("ceiling.json"), "{arguments:?} ran the task");
    assert_eq!(stdout(&output), "", "{arguments:?} ran the script");
    stderr(&output)
}

/// The prefix pattern a directory's value names: the directory and everything beneath it.
fn beneath(directory: &Path) -> PathPattern {
    PathPattern::Prefix(PathBuf::from(format!("{}/", directory.display())))
}

#[test]
fn allow_flags_without_permission_fail_before_anything_runs() {
    let temp = project();
    let value_flags = ["--allow-fs-read=.", "--allow-fs-write=*", "--allow-env=PORT"];

    for flag in value_flags.into_iter().chain(BOOLEAN_FLAGS) {
        for target in ["ceiling", "show.mjs"] {
            let message = rejected(&temp, &["run", flag, target]);

            assert_eq!(message, "error: --permission is required\n", "{flag} {target}");
        }
    }
}

#[test]
fn a_value_flag_without_a_value_requires_an_argument() {
    let temp = project();

    for flag in ["--allow-fs-read", "--allow-fs-write", "--allow-env"] {
        let expected = format!("error: {flag} requires an argument\n");
        assert_eq!(rejected(&temp, &["run", "--permission", flag]), expected);
        for next in ["--allow-net", "-x", "-", "--unknown", "--"] {
            assert_eq!(
                rejected(&temp, &["run", "--permission", flag, next, "ceiling"]),
                expected,
                "{flag} {next}"
            );
        }

        let empty = format!("{flag}=");
        let message = rejected(&temp, &["run", "--permission", &empty, "ceiling"]);
        assert_eq!(message, format!("error: {flag}= requires an argument\n"));
    }

    let output = temp.bento(&["run", "--permission", "show.mjs", "--allow-fs-read", "-x"]).output();
    let output = output.unwrap();
    assert!(output.status.success(), "the script's own words: {}", stderr(&output));
    assert_eq!(stdout(&output), "ran\n");
}

#[test]
fn an_environment_value_of_any_other_form_is_rejected() {
    let temp = project();

    for value in ["A*B", "A=B", "*PORT", "PORT,A=B"] {
        let flag = format!("--allow-env={value}");
        let message = rejected(&temp, &["run", "--permission", &flag, "ceiling"]);

        assert_eq!(
            message,
            "error: --allow-env must be '*', a variable name, or a variable name prefix followed by '*'\n",
            "{flag}"
        );
    }
}

#[test]
fn boolean_flags_reject_a_value_and_name_the_flag() {
    let temp = project();
    let with_values = [
        "--allow-net=127.0.0.1:3000",
        "--allow-addons=./node_modules",
        "--allow-language-model=false",
    ];
    let every_flag = BOOLEAN_FLAGS.map(|flag| format!("{flag}=true"));

    for flag in with_values.iter().copied().chain(every_flag.iter().map(String::as_str)) {
        let message = rejected(&temp, &["run", "--permission", flag, "ceiling"]);

        let name = flag.split_once('=').unwrap().0;
        assert!(message.contains(&format!("'{name}'")), "{flag}: {message}");
    }
    let message = rejected(&temp, &["run", "--permission=true", "ceiling"]);
    assert!(message.contains("'--permission'"), "{message}");
}

#[test]
fn bento_s_earlier_flags_are_unknown_options() {
    let temp = project();

    for flag in ["--allow-read", "--allow-read=.", "--allow-write=db"] {
        let message = rejected(&temp, &["run", "--permission", flag, "ceiling"]);

        let name = flag.split('=').next().unwrap();
        assert!(message.contains(&format!("unexpected argument '{name}")), "{flag}: {message}");
    }
}

#[test]
fn permission_alone_allows_a_task_launch_nothing() {
    let temp = project();

    let permissions = restricted_by(&temp, &[]);

    assert_eq!(permissions, Permissions::restricted(PermissionSet::default()));
}

#[test]
fn each_boolean_flag_allows_exactly_its_own_permission() {
    let temp = project();

    for flag in BOOLEAN_FLAGS {
        let mut expected = PermissionSet::default();
        let allowed = match flag {
            "--allow-net" => &mut expected.net,
            "--allow-child-process" => &mut expected.child_process,
            "--allow-worker" => &mut expected.worker,
            "--allow-addons" => &mut expected.addons,
            "--allow-wasi" => &mut expected.wasi,
            "--allow-ffi" => &mut expected.ffi,
            "--allow-inspector" => &mut expected.inspector,
            "--allow-openssl-store" => &mut expected.openssl_store,
            "--allow-fs-vfs" => &mut expected.fs_vfs,
            "--allow-language-model" => &mut expected.language_model,
            other => unreachable!("{other} is not a boolean flag"),
        };
        *allowed = true;

        assert_eq!(restricted_by(&temp, &[flag]), Permissions::restricted(expected), "{flag}");
    }
}

#[test]
fn flags_may_be_repeated() {
    let temp = project();

    let permissions = restricted_by(&temp, &["--permission", "--allow-net", "--allow-net"]);

    let expected = PermissionSet { net: true, ..PermissionSet::default() };
    assert_eq!(permissions, Permissions::restricted(expected));
}

#[test]
fn path_values_resolve_from_the_invocation_directory_to_real_locations() {
    let temp = project();
    symlink(temp.path("data"), temp.path("linked-data")).unwrap();
    let invocation_directory = temp.linked_root().join("data");
    let through_tmp_link = format!("--allow-fs-read={}", temp.linked_root().join("db").display());
    let flags = [
        "--allow-fs-read=../linked-data",
        &through_tmp_link,
        "--allow-fs-read",
        "inner/entry.txt",
        "--allow-fs-read=../missing",
        "--allow-fs-write=../build/*",
        "--allow-fs-write=../dist-*ignored",
    ];

    let arguments = [&["run", "--permission"], &flags[..], &["ceiling"]].concat();
    let output = bento_in(&invocation_directory, &arguments).output().unwrap();

    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(stderr(&output), "");
    let expected = PermissionSet {
        fs_read: Scope::only([
            beneath(&temp.path("data")),
            beneath(&temp.path("db")),
            PathPattern::Exact(temp.path("data/inner/entry.txt")),
            PathPattern::Exact(temp.path("missing")),
        ]),
        fs_write: Scope::only([
            beneath(&temp.path("build")),
            PathPattern::Prefix(temp.path("dist-")),
        ]),
        ..PermissionSet::default()
    };
    assert_eq!(recorded(&temp), Permissions::restricted(expected));
}

#[test]
fn a_star_allows_every_path_or_variable() {
    let temp = project();

    let permissions = restricted_by(
        &temp,
        &["--allow-fs-read=*", "--allow-fs-read=data", "--allow-fs-write", "*", "--allow-env=*"],
    );

    let expected = PermissionSet {
        fs_read: Scope::All,
        fs_write: Scope::All,
        env: Scope::All,
        ..PermissionSet::default()
    };
    assert_eq!(permissions, Permissions::restricted(expected));
}

#[test]
fn environment_values_are_comma_separated_and_repeatable_and_skip_empty_items() {
    let temp = project();

    let permissions = restricted_by(&temp, &["--allow-env=PORT,,HOST", "--allow-env", "APP_*"]);

    let names = ["PORT", "HOST", "APP_*"].map(|name| EnvPattern::parse(name).unwrap());
    let expected = PermissionSet { env: Scope::only(names), ..PermissionSet::default() };
    assert_eq!(permissions, Permissions::restricted(expected));
}

#[test]
fn a_comma_in_a_lone_path_value_warns_and_stays_part_of_one_path() {
    let temp = project();

    for flag in ["--allow-fs-read", "--allow-fs-write"] {
        let value = format!("{flag}=data,db");
        let output = temp.bento(&["run", "--permission", &value, "ceiling"]).output().unwrap();

        assert!(output.status.success(), "{}", stderr(&output));
        assert_eq!(stderr(&output), format!("warning: The {flag}{COMMA_WARNING}\n"));
        let one_path = Scope::only([PathPattern::Exact(temp.path("data,db"))]);
        let expected = match flag {
            "--allow-fs-read" => PermissionSet { fs_read: one_path, ..PermissionSet::default() },
            _ => PermissionSet { fs_write: one_path, ..PermissionSet::default() },
        };
        assert_eq!(recorded(&temp), Permissions::restricted(expected));

        let repeated = temp.bento(&["run", "--permission", &value, &value, "ceiling"]).output();
        assert_eq!(stderr(&repeated.unwrap()), "", "{flag} given twice, as Node counts it");
    }

    let script = |flag: &str| {
        let value = format!("{flag}=data,db");
        stderr(&temp.bento(&["run", "--permission", &value, "show.mjs"]).output().unwrap())
    };
    assert_eq!(script("--allow-fs-read"), "", "the script counts as a read value, as in Node");
    assert_eq!(
        script("--allow-fs-write"),
        format!("warning: The --allow-fs-write{COMMA_WARNING}\n")
    );
}

#[test]
fn a_path_that_cannot_be_resolved_names_the_flag_and_value() {
    let temp = project();
    symlink("loop", temp.path("loop")).unwrap();

    let message = rejected(&temp, &["run", "--permission", "--allow-fs-write=loop/db", "ceiling"]);

    assert!(message.starts_with("error: "), "{message}");
    assert!(message.contains("--allow-fs-write=loop/db"), "{message}");
}

#[test]
fn help_lists_the_permission_flags_under_their_own_heading() {
    let temp = project();

    let output = temp.bento(&["run", "--help"]).output().unwrap();

    assert_eq!(output.status.code(), Some(0));
    let help = stdout(&output);
    let (_, permissions) = help.split_once("\nPermissions:\n").expect("a Permissions heading");
    let flags = ["--permission", "--allow-fs-read", "--allow-fs-write", "--allow-env"];
    for flag in flags.into_iter().chain(BOOLEAN_FLAGS) {
        assert!(permissions.contains(flag), "{flag} is not under Permissions: {help}");
    }
    assert!(described(permissions, "--allow-child-process").contains("full authority"), "{help}");
    assert!(
        described(permissions, "--allow-addons").contains("outside every permission"),
        "{help}"
    );
}

/// The help text beside `flag` in `options`, a help section listing one option per line or
/// with its description on the lines after it.
fn described<'a>(options: &'a str, flag: &str) -> String {
    let mut lines = options.lines().skip_while(|line| !line.trim_start().starts_with(flag));
    let first = lines.next().unwrap_or_else(|| panic!("{flag} is not listed: {options}"));
    let rest = lines.take_while(|line| !line.trim_start().starts_with('-'));
    [first].into_iter().chain(rest).collect::<Vec<&'a str>>().join(" ")
}
