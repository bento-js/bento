mod support;

use std::collections::{BTreeMap, BTreeSet};

use serde_json::json;

use bento_host::{CEILING_ENV, GrantRequest, Grants, Scope};
use bento_task::{Cancellation, Plan};
use bento_workspace::{FILE_KEYS_ENV, LaunchEnvironment, LaunchKind};
use support::{SYSTEM_PATH, TempProject, parse_env_dump};

const NESTED_WATCH: &str = "bento run --watch server.ts";

/// A backslash ending a line, which the shell removes to join the line to the next.
const LINE_CONTINUATION: &str = "\\\n";

/// Runs `task` from an outer, non-watch launch and returns the environment the `bento` it
/// started received.
fn nested_bento_environment(
    temp: &TempProject,
    task: &str,
    inherited: &[(&str, &str)],
    grants: &Grants,
) -> BTreeMap<String, String> {
    let plan = Plan::new(temp.project().manifest(), task, &[]).unwrap();
    temp.runner(inherited, grants).run(&plan, &Cancellation::new()).unwrap();
    parse_env_dump(&temp.read("bento.env"))
}

fn nested_watch_launch(
    temp: &TempProject,
    inherited: BTreeMap<String, String>,
) -> LaunchEnvironment {
    LaunchEnvironment::load(temp.root(), inherited, LaunchKind::Watch).unwrap()
}

fn value<'a>(environment: &'a LaunchEnvironment, name: &str) -> Option<&'a str> {
    environment.vars().get(name).map(String::as_str)
}

/// The environment the nested watch launch in `command` resolves when the task runs from a
/// project whose `.env` sets `DATABASE_URL=base` and whose `.env.development` sets
/// `DATABASE_URL=dev`.
fn nested_launch_started_by(command: &str) -> LaunchEnvironment {
    nested_launch_in(&TempProject::with_tasks(json!({ "dev": command })))
}

/// Like [`nested_launch_started_by`], from a project whose `bin` directory holds a stand-in
/// for `env`, a `test` script that runs `bento`, and a `command` script that ignores its
/// arguments and runs `bento`.
fn nested_launch_with_project_scripts_started_by(command: &str) -> LaunchEnvironment {
    let temp = TempProject::with_tasks(json!({ "dev": command }));
    temp.write_executable("bin/env", "#!/bin/sh\nexec /usr/bin/env \"$@\"\n");
    temp.write_executable("bin/test", "#!/bin/sh\nexec bento \"$@\"\n");
    temp.write_executable("bin/command", &format!("#!/bin/sh\nexec {NESTED_WATCH}\n"));
    nested_launch_in(&temp)
}

fn nested_launch_in(temp: &TempProject) -> LaunchEnvironment {
    temp.write(".env", "DATABASE_URL=base\n");
    temp.write(".env.development", "DATABASE_URL=dev\n");
    let child = nested_bento_environment(temp, "dev", &[("PATH", SYSTEM_PATH)], &Grants::none());
    nested_watch_launch(temp, child)
}

#[test]
fn task_children_receive_the_launch_grants_as_their_ceiling() {
    let temp = TempProject::with_tasks(json!({ "dev": NESTED_WATCH }));
    let grants = GrantRequest {
        read: Some(Scope::Only(BTreeSet::from([temp.root().to_path_buf()]))),
        env: Some(Scope::Only(BTreeSet::from(["DATABASE_URL".to_string()]))),
        ..GrantRequest::default()
    }
    .resolve(None)
    .unwrap();

    let child = nested_bento_environment(&temp, "dev", &[("PATH", SYSTEM_PATH)], &grants);

    assert_eq!(Grants::from_ceiling_value(&child[CEILING_ENV]).unwrap(), grants);
    assert!(child.contains_key(FILE_KEYS_ENV));
}

#[test]
fn nested_watch_command_uses_its_own_mode_file() {
    let temp = TempProject::with_tasks(json!({ "dev": NESTED_WATCH }));
    temp.write(".env", "DATABASE_URL=base\n");
    temp.write(".env.development", "DATABASE_URL=dev\n");

    let child = nested_bento_environment(&temp, "dev", &[("PATH", SYSTEM_PATH)], &Grants::none());
    let nested = nested_watch_launch(&temp, child.clone());

    assert_eq!(child["DATABASE_URL"], "base");
    assert!(!child.contains_key("NODE_ENV"));
    assert_eq!(value(&nested, "DATABASE_URL"), Some("dev"));
    assert_eq!(value(&nested, "NODE_ENV"), Some("development"));
}

#[test]
fn exported_value_still_wins_in_the_nested_launch() {
    let temp = TempProject::with_tasks(json!({ "dev": NESTED_WATCH }));
    temp.write(".env", "DATABASE_URL=base\n");
    temp.write(".env.development", "DATABASE_URL=dev\n");

    let inherited = [("PATH", SYSTEM_PATH), ("DATABASE_URL", "exported")];
    let child = nested_bento_environment(&temp, "dev", &inherited, &Grants::none());
    let nested = nested_watch_launch(&temp, child.clone());

    assert_eq!(child["DATABASE_URL"], "exported");
    assert_eq!(value(&nested, "DATABASE_URL"), Some("exported"));
}

#[test]
fn inline_assignment_in_the_task_wins_in_the_nested_launch() {
    let temp = TempProject::with_tasks(json!({
        "dev": format!("DATABASE_URL=inline NODE_ENV=test {NESTED_WATCH}"),
    }));
    temp.write(".env", "DATABASE_URL=base\nNODE_ENV=production\n");
    temp.write(".env.development", "DATABASE_URL=dev\n");
    temp.write(".env.test", "TEST_ONLY=1\n");

    let child = nested_bento_environment(&temp, "dev", &[("PATH", SYSTEM_PATH)], &Grants::none());
    let nested = nested_watch_launch(&temp, child);

    assert_eq!(value(&nested, "DATABASE_URL"), Some("inline"));
    assert_eq!(value(&nested, "NODE_ENV"), Some("test"));
    assert_eq!(value(&nested, "TEST_ONLY"), Some("1"));
}

#[test]
fn inline_assignment_of_the_file_value_still_wins_in_the_nested_launch() {
    for command in [
        format!("DATABASE_URL=base {NESTED_WATCH}"),
        format!("true && DATABASE_URL='base' {NESTED_WATCH}"),
        format!("DATABASE_URL=base exec {NESTED_WATCH}"),
        format!("export DATABASE_URL=base; {NESTED_WATCH}"),
        format!("DATABASE_URL=base; {NESTED_WATCH}"),
        format!("env DATABASE_URL=base {NESTED_WATCH}"),
    ] {
        let nested = nested_launch_started_by(&command);

        assert_eq!(value(&nested, "DATABASE_URL"), Some("base"), "{command}");
    }
}

#[test]
fn assignment_scoped_to_another_command_leaves_the_file_value_to_the_nested_mode() {
    for command in [
        format!("DATABASE_URL=base true; {NESTED_WATCH}"),
        format!("DATABASE_URL=base true && {NESTED_WATCH}"),
        format!("DATABASE_URL=base echo > /dev/null; {NESTED_WATCH}"),
        format!("DATABASE_URL=base test -d .; {NESTED_WATCH}"),
        format!("DATABASE_URL=base command true; {NESTED_WATCH}"),
        format!("env DATABASE_URL=base true; {NESTED_WATCH}"),
        format!("env -S 'DATABASE_URL=base true'; {NESTED_WATCH}"),
        format!("DATABASE_URL=base >/dev/null true; {NESTED_WATCH}"),
    ] {
        let nested = nested_launch_started_by(&command);

        assert_eq!(value(&nested, "DATABASE_URL"), Some("dev"), "{command}");
    }
}

#[test]
fn export_run_in_the_shell_through_a_wrapper_still_wins_in_the_nested_launch() {
    for command in [
        format!("command export DATABASE_URL=base; {NESTED_WATCH}"),
        format!("command readonly DATABASE_URL=base; {NESTED_WATCH}"),
        format!("command -p export DATABASE_URL=base; {NESTED_WATCH}"),
        format!("command command export DATABASE_URL=base; {NESTED_WATCH}"),
        format!("builtin export DATABASE_URL=base; {NESTED_WATCH}"),
        format!("command builtin readonly DATABASE_URL=base; {NESTED_WATCH}"),
    ] {
        let nested = nested_launch_started_by(&command);

        assert_eq!(value(&nested, "DATABASE_URL"), Some("base"), "{command}");
    }
}

#[test]
fn export_executed_as_a_program_leaves_the_file_value_to_the_nested_mode() {
    for command in [
        format!("env export DATABASE_URL=base > /dev/null 2>&1; {NESTED_WATCH}"),
        format!("/usr/bin/env export DATABASE_URL=base > /dev/null 2>&1; {NESTED_WATCH}"),
        format!("nohup export DATABASE_URL=base > /dev/null 2>&1; {NESTED_WATCH}"),
        format!("/usr/bin/nohup readonly DATABASE_URL=base > /dev/null 2>&1; {NESTED_WATCH}"),
        format!("command env export DATABASE_URL=base > /dev/null 2>&1; {NESTED_WATCH}"),
        format!("env command export DATABASE_URL=base > /dev/null 2>&1; {NESTED_WATCH}"),
        format!("nohup command export DATABASE_URL=base > /dev/null 2>&1; {NESTED_WATCH}"),
    ] {
        let nested = nested_launch_started_by(&command);

        assert_eq!(value(&nested, "DATABASE_URL"), Some("dev"), "{command}");
    }
}

#[test]
fn command_described_rather_than_run_leaves_the_file_value_to_the_nested_mode() {
    for command in [
        format!("command -v export DATABASE_URL=base > /dev/null; {NESTED_WATCH}"),
        format!("command -V readonly DATABASE_URL=base > /dev/null; {NESTED_WATCH}"),
        format!("command -pv export DATABASE_URL=base > /dev/null; {NESTED_WATCH}"),
        format!("command -p -V export DATABASE_URL=base > /dev/null; {NESTED_WATCH}"),
        format!("command command -v export DATABASE_URL=base > /dev/null; {NESTED_WATCH}"),
        format!("DATABASE_URL=base command -v bento > /dev/null; {NESTED_WATCH}"),
    ] {
        let nested = nested_launch_started_by(&command);

        assert_eq!(value(&nested, "DATABASE_URL"), Some("dev"), "{command}");
    }
}

#[test]
fn assignment_on_a_special_builtin_persists_for_the_nested_launch() {
    let nested = nested_launch_started_by(&format!("DATABASE_URL=base :; {NESTED_WATCH}"));

    assert_eq!(value(&nested, "DATABASE_URL"), Some("base"));
}

#[test]
fn quoted_and_option_separated_assignments_still_win_in_the_nested_launch() {
    for command in [
        format!("env \"DATABASE_URL=base\" {NESTED_WATCH}"),
        format!("env 'DATABASE_URL'=base {NESTED_WATCH}"),
        format!("env -u UNUSED DATABASE_URL=base {NESTED_WATCH}"),
        format!("env -uUNUSED DATABASE_URL=base {NESTED_WATCH}"),
        format!("env -- DATABASE_URL=base {NESTED_WATCH}"),
        format!("env -u UNUSED env DATABASE_URL=base {NESTED_WATCH}"),
        format!("exec env DATABASE_URL=base {NESTED_WATCH}"),
        format!("command env DATABASE_URL=base {NESTED_WATCH}"),
        format!("export \"DATABASE_URL=base\"; {NESTED_WATCH}"),
    ] {
        let nested = nested_launch_started_by(&command);

        assert_eq!(value(&nested, "DATABASE_URL"), Some("base"), "{command}");
    }
}

#[test]
fn text_that_only_resembles_an_assignment_leaves_the_file_value_to_the_nested_mode() {
    for command in [
        format!("echo DATABASE_URL=base > /dev/null; {NESTED_WATCH}"),
        format!("echo 'x; DATABASE_URL=base' > /dev/null; {NESTED_WATCH}"),
        format!("\"DATABASE_URL=base\" 2> /dev/null; {NESTED_WATCH}"),
        format!("{NESTED_WATCH} # DATABASE_URL=base"),
        format!("env UNUSED=1 echo DATABASE_URL=base > /dev/null; {NESTED_WATCH}"),
    ] {
        let nested = nested_launch_started_by(&command);

        assert_eq!(value(&nested, "DATABASE_URL"), Some("dev"), "{command}");
    }
}

#[test]
fn env_that_runs_no_utility_leaves_the_file_value_to_the_nested_mode() {
    for command in [
        format!("env DATABASE_URL=base > /dev/null; {NESTED_WATCH}"),
        format!("env DATABASE_URL=base >/dev/null; {NESTED_WATCH}"),
        format!("env -i DATABASE_URL=base 2>&1 >/dev/null; {NESTED_WATCH}"),
        format!("env -u UNUSED DATABASE_URL=base > /dev/null; {NESTED_WATCH}"),
        format!("DATABASE_URL=base env > /dev/null; {NESTED_WATCH}"),
        format!("env -S 'DATABASE_URL=base' > /dev/null; {NESTED_WATCH}"),
        format!("env -S 'DATABASE_URL=base # {NESTED_WATCH}' > /dev/null; {NESTED_WATCH}"),
    ] {
        let nested = nested_launch_started_by(&command);

        assert_eq!(value(&nested, "DATABASE_URL"), Some("dev"), "{command}");
    }
}

#[test]
fn assignment_separated_from_the_nested_launch_by_a_redirection_still_wins() {
    for command in [
        format!("env DATABASE_URL=base >/dev/null {NESTED_WATCH}"),
        format!("env DATABASE_URL=base 2> /dev/null {NESTED_WATCH}"),
        format!("DATABASE_URL=base </dev/null {NESTED_WATCH}"),
        format!("DATABASE_URL=base 2>&1 {NESTED_WATCH}"),
    ] {
        let nested = nested_launch_started_by(&command);

        assert_eq!(value(&nested, "DATABASE_URL"), Some("base"), "{command}");
    }
}

#[test]
fn env_split_string_assignments_still_win_in_the_nested_launch() {
    for command in [
        format!("env -S 'DATABASE_URL=base {NESTED_WATCH}'"),
        format!("env -S'DATABASE_URL=base {NESTED_WATCH}'"),
        format!("env -S '\"DATABASE_URL=base\" {NESTED_WATCH}'"),
        format!("env -S 'DATABASE_URL=base\\_{NESTED_WATCH}'"),
        format!("env -S '-u UNUSED DATABASE_URL=base {NESTED_WATCH}'"),
        format!("env -u UNUSED -S 'DATABASE_URL=base {NESTED_WATCH}'"),
        format!("env -S 'DATABASE_URL=base' {NESTED_WATCH}"),
    ] {
        let nested = nested_launch_started_by(&command);

        assert_eq!(value(&nested, "DATABASE_URL"), Some("base"), "{command}");
    }
}

#[test]
fn assignment_after_a_line_continuation_still_wins_in_the_nested_launch() {
    for command in [
        format!("{LINE_CONTINUATION}DATABASE_URL=base {NESTED_WATCH}"),
        format!("UNUSED=1 {LINE_CONTINUATION} DATABASE_URL=base {NESTED_WATCH}"),
        format!("true && {LINE_CONTINUATION}DATABASE_URL=base {NESTED_WATCH}"),
        format!("export {LINE_CONTINUATION}DATABASE_URL=base; {NESTED_WATCH}"),
        format!("true # comment {LINE_CONTINUATION}DATABASE_URL=base {NESTED_WATCH}"),
    ] {
        let nested = nested_launch_started_by(&command);

        assert_eq!(value(&nested, "DATABASE_URL"), Some("base"), "{command:?}");
    }
}

#[test]
fn env_operands_after_a_line_continuation_still_win_in_the_nested_launch() {
    for command in [
        format!("env {LINE_CONTINUATION} DATABASE_URL=base {NESTED_WATCH}"),
        format!("/usr/bin/env {LINE_CONTINUATION} DATABASE_URL=base {NESTED_WATCH}"),
        format!("env -u UNUSED {LINE_CONTINUATION}DATABASE_URL=base {NESTED_WATCH}"),
        format!("env -S {LINE_CONTINUATION}'DATABASE_URL=base {NESTED_WATCH}'"),
    ] {
        let nested = nested_launch_started_by(&command);

        assert_eq!(value(&nested, "DATABASE_URL"), Some("base"), "{command:?}");
    }
}

#[test]
fn env_word_split_by_a_line_continuation_still_passes_its_assignments_to_the_nested_launch() {
    for command in [
        format!("e{LINE_CONTINUATION}nv DATABASE_URL=base {NESTED_WATCH}"),
        format!("/usr/bin/en{LINE_CONTINUATION}v DATABASE_URL=base {NESTED_WATCH}"),
        format!("exec e{LINE_CONTINUATION}nv DATABASE_URL=base {NESTED_WATCH}"),
    ] {
        let nested = nested_launch_started_by(&command);

        assert_eq!(value(&nested, "DATABASE_URL"), Some("base"), "{command:?}");
    }
}

#[test]
fn line_continuation_inside_double_quotes_joins_the_assignment() {
    for command in [
        format!("DATABASE_URL=\"ba{LINE_CONTINUATION}se\" {NESTED_WATCH}"),
        format!("env \"DATABASE_URL=ba{LINE_CONTINUATION}se\" {NESTED_WATCH}"),
        format!("env \"DATABASE_{LINE_CONTINUATION}URL=base\" {NESTED_WATCH}"),
    ] {
        let nested = nested_launch_started_by(&command);

        assert_eq!(value(&nested, "DATABASE_URL"), Some("base"), "{command:?}");
    }
}

#[test]
fn backslash_newline_inside_single_quotes_stays_literal() {
    let other_name = nested_launch_started_by(&format!(
        "env 'DATABASE_{LINE_CONTINUATION}URL=base' {NESTED_WATCH}"
    ));
    let literal_value =
        nested_launch_started_by(&format!("DATABASE_URL='ba{LINE_CONTINUATION}se' {NESTED_WATCH}"));

    assert_eq!(value(&other_name, "DATABASE_URL"), Some("dev"));
    assert_eq!(value(&literal_value, "DATABASE_URL"), Some("ba\\\nse"));
}

#[test]
fn redirection_operator_split_by_a_line_continuation_leaves_the_assignment_on_its_command() {
    let nested = nested_launch_started_by(&format!(
        "env DATABASE_URL=base 2>{LINE_CONTINUATION}&1 {NESTED_WATCH}"
    ));

    assert_eq!(value(&nested, "DATABASE_URL"), Some("base"));
}

#[test]
fn here_document_body_leaves_the_file_value_to_the_nested_mode() {
    for command in [
        format!("cat <<EOF >/dev/null\nDATABASE_{LINE_CONTINUATION}URL=base\nEOF\n{NESTED_WATCH}"),
        format!(
            "cat <<'EOF' >/dev/null\nDATABASE_{LINE_CONTINUATION}URL=base\nEOF\n{NESTED_WATCH}"
        ),
        format!("cat <<EOF >/dev/null\nDATABASE_URL=base\nEOF\n{NESTED_WATCH}"),
        format!("cat <<-EOF >/dev/null\n\tDATABASE_URL=base\n\tEOF\n{NESTED_WATCH}"),
        format!(
            "cat <<EOF >/dev/null\nx{LINE_CONTINUATION}EOF\nDATABASE_URL=base\nEOF\n{NESTED_WATCH}"
        ),
        format!("cat <<E{LINE_CONTINUATION}OF >/dev/null\nDATABASE_URL=base\nEOF\n{NESTED_WATCH}"),
        format!("cat <<EOF >/dev/null # comment\nDATABASE_URL=base\nEOF\n{NESTED_WATCH}"),
        format!("cat <<A >/dev/null; cat <<B >/dev/null\nA\nDATABASE_URL=base\nB\n{NESTED_WATCH}"),
        format!("echo $(cat <<EOF\nDATABASE_URL=base\nEOF\n) >/dev/null\n{NESTED_WATCH}"),
        format!("cat <<EOF >/dev/null\nDATABASE_URL=$HOME\nEOF\n{NESTED_WATCH}"),
        format!("cat <<'EOF' >/dev/null\n$(DATABASE_URL=base {NESTED_WATCH})\nEOF\n{NESTED_WATCH}"),
        format!(
            "cat <<\"EOF\" >/dev/null\n$(DATABASE_URL=base {NESTED_WATCH})\nEOF\n{NESTED_WATCH}"
        ),
        format!("cat <<\\EOF >/dev/null\n`DATABASE_URL=base {NESTED_WATCH}`\nEOF\n{NESTED_WATCH}"),
        format!("cat <<E'O'F >/dev/null\n$(DATABASE_URL=base {NESTED_WATCH})\nEOF\n{NESTED_WATCH}"),
        format!("cat <<EOF >/dev/null\n$(DATABASE_URL=base true)\nEOF\n{NESTED_WATCH}"),
    ] {
        let nested = nested_launch_started_by(&command);

        assert_eq!(value(&nested, "DATABASE_URL"), Some("dev"), "{command:?}");
    }
}

#[test]
fn assignment_in_a_here_document_command_substitution_still_wins_in_the_nested_launch() {
    for command in [
        format!("cat <<EOF\n$(DATABASE_URL=base {NESTED_WATCH})\nEOF"),
        format!("cat <<EOF >/dev/null\n`DATABASE_URL=base {NESTED_WATCH}`\nEOF"),
        format!(
            "cat <<EOF >/dev/null\nname: app\nurl: $(env DATABASE_URL=base {NESTED_WATCH})\nEOF"
        ),
        format!("cat <<-EOF >/dev/null\n\t$(DATABASE_URL=base {NESTED_WATCH})\n\tEOF"),
        format!(
            "cat <<EOF >/dev/null\n$(DATABASE_URL=base {LINE_CONTINUATION}{NESTED_WATCH})\nEOF"
        ),
        format!(
            "cat <<EOF >/dev/null\n${LINE_CONTINUATION}(DATABASE_URL=base {NESTED_WATCH})\nEOF"
        ),
        format!(
            "cat <<A >/dev/null; cat <<B >/dev/null\nA\n$(DATABASE_URL=base {NESTED_WATCH})\nB"
        ),
        format!("cat <<EOF\n\\\\$(DATABASE_URL=base {NESTED_WATCH})\nEOF"),
        format!("cat <<EOF\n$((echo) ; DATABASE_URL=base {NESTED_WATCH})\nEOF"),
        format!("echo $(cat <<EOF\n$(DATABASE_URL=base {NESTED_WATCH})\nEOF\n) >/dev/null"),
    ] {
        let nested = nested_launch_started_by(&command);

        assert_eq!(value(&nested, "DATABASE_URL"), Some("base"), "{command:?}");
    }
}

#[test]
fn here_document_text_does_not_hide_a_command_substitution_from_the_nested_launch() {
    for command in [
        format!("cat <<EOF\n# $(DATABASE_URL=base {NESTED_WATCH})\nEOF"),
        format!("cat <<EOF\n\"$(DATABASE_URL=base {NESTED_WATCH})\"\nEOF"),
        format!("cat <<EOF\n'$(DATABASE_URL=base {NESTED_WATCH})'\nEOF"),
        format!("cat <<EOF >/dev/null\n# `DATABASE_URL=base {NESTED_WATCH}`\nEOF"),
        format!("cat <<EOF >/dev/null\nit's\n$(DATABASE_URL=base {NESTED_WATCH})\nEOF"),
    ] {
        let nested = nested_launch_started_by(&command);

        assert_eq!(value(&nested, "DATABASE_URL"), Some("base"), "{command:?}");
    }
}

/// The shell reads the assignment line as input, but Bento does not find where a substitution
/// ends, so it keeps the user's value for every line of a body that runs one.
#[test]
fn assignment_line_in_a_here_document_that_runs_a_substitution_keeps_the_value() {
    let nested = nested_launch_started_by(&format!(
        "cat <<EOF >/dev/null\nit's\nDATABASE_URL=base\n$(true)\nEOF\n{NESTED_WATCH}"
    ));

    assert_eq!(value(&nested, "DATABASE_URL"), Some("base"));
}

#[test]
fn escaped_substitution_marker_leaves_the_here_document_body_as_input() {
    for command in [
        format!("cat <<EOF >/dev/null\nDATABASE_URL=base\n\\$(echo ignored)\nEOF\n{NESTED_WATCH}"),
        format!("cat <<EOF >/dev/null\nDATABASE_URL=base\n\\`echo ignored\\`\nEOF\n{NESTED_WATCH}"),
        format!("cat <<EOF >/dev/null\n\\$(DATABASE_URL=base {NESTED_WATCH})\nEOF\n{NESTED_WATCH}"),
    ] {
        let nested = nested_launch_started_by(&command);

        assert_eq!(value(&nested, "DATABASE_URL"), Some("dev"), "{command:?}");
    }
}

#[test]
fn assignment_around_a_here_document_still_wins_in_the_nested_launch() {
    for command in [
        format!("cat <<EOF >/dev/null\nbody\nEOF\nDATABASE_URL=base {NESTED_WATCH}"),
        format!(
            "cat <<'EOF' >/dev/null\nbody{LINE_CONTINUATION}EOF\nDATABASE_URL=base {NESTED_WATCH}"
        ),
        format!("cat <<EOF >/dev/null; DATABASE_URL=base {NESTED_WATCH}\nbody\nEOF"),
        format!("DATABASE_URL=base {NESTED_WATCH} <<EOF\nbody\nEOF"),
        format!("cat <<<body >/dev/null\nDATABASE_URL=base {NESTED_WATCH}"),
    ] {
        let nested = nested_launch_started_by(&command);

        assert_eq!(value(&nested, "DATABASE_URL"), Some("base"), "{command:?}");
    }
}

#[test]
fn wrapper_named_by_a_path_still_passes_its_assignments_to_the_nested_launch() {
    for command in [
        format!("/usr/bin/env DATABASE_URL=base {NESTED_WATCH}"),
        format!("/usr/bin/env -u UNUSED DATABASE_URL=base {NESTED_WATCH}"),
        format!("./bin/env DATABASE_URL=base {NESTED_WATCH}"),
        format!("/usr/bin/nohup env DATABASE_URL=base {NESTED_WATCH} > /dev/null"),
        format!("/usr/bin/nohup /usr/bin/env DATABASE_URL=base {NESTED_WATCH} > /dev/null"),
        "/usr/bin/env DATABASE_URL=base ./bento-bin/bento run --watch server.ts".to_owned(),
    ] {
        let nested = nested_launch_with_project_scripts_started_by(&command);

        assert_eq!(value(&nested, "DATABASE_URL"), Some("base"), "{command}");
    }
}

#[test]
fn wrapper_named_by_a_path_that_starts_no_launch_leaves_the_file_value_to_the_nested_mode() {
    for command in [
        format!("DATABASE_URL=base /usr/bin/env > /dev/null; {NESTED_WATCH}"),
        format!("DATABASE_URL=base ./bin/env > /dev/null; {NESTED_WATCH}"),
        format!("/usr/bin/env DATABASE_URL=base true; {NESTED_WATCH}"),
        format!("DATABASE_URL=base /usr/bin/nohup true > /dev/null; {NESTED_WATCH}"),
    ] {
        let nested = nested_launch_with_project_scripts_started_by(&command);

        assert_eq!(value(&nested, "DATABASE_URL"), Some("dev"), "{command}");
    }
}

#[test]
fn program_named_by_a_path_like_a_builtin_may_still_start_the_nested_launch() {
    for command in [
        "DATABASE_URL=base ./bin/test run --watch server.ts",
        "DATABASE_URL=base ./bin/command true",
    ] {
        let nested = nested_launch_with_project_scripts_started_by(command);

        assert_eq!(value(&nested, "DATABASE_URL"), Some("base"), "{command}");
    }
}
