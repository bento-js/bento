mod support;

use serde_json::json;

use support::{TempProject, stderr};

#[test]
fn a_malformed_package_json_is_shown_with_its_source_at_the_error() {
    let temp = TempProject::new(json!({}));
    temp.write(
        "package.json",
        "{\n  \"name\": \"guests\",\n  \"type\": \"module\"\n  \"tasks\": {}\n}\n",
    );
    temp.write("server.mjs", "console.log('never runs');\n");

    let output = temp.bento(&["run", "--allow-read=.", "server.mjs"]).output().unwrap();

    assert_eq!(output.status.code(), Some(1));
    let report = stderr(&output);
    assert!(report.contains("is not valid JSON"), "{report}");
    assert!(report.contains("package.json:4:3"), "{report}");
    assert!(report.contains("  \"tasks\": {}"), "{report}");
    assert!(report.contains("expected `,` or `}`"), "{report}");
    assert!(!report.contains('\u{1b}'), "styled output when stderr is not a terminal: {report}");
}

#[test]
fn a_malformed_environment_file_names_the_line_without_showing_its_contents() {
    let temp = TempProject::new(json!({ "type": "module" }));
    temp.write(".env", "PORT=3000\nSESSION_SECRET=\"hunter2\n");
    temp.write("server.mjs", "console.log('never runs');\n");

    let output = temp.bento(&["run", "--allow-read=.", "server.mjs"]).output().unwrap();

    assert_eq!(output.status.code(), Some(1));
    let report = stderr(&output);
    assert!(report.contains(".env:2"), "{report}");
    assert!(!report.contains("hunter2"), "the secret was printed: {report}");
    assert!(!report.contains("3000"), "the file's contents were printed: {report}");
}

#[test]
fn a_compiler_option_bento_ignores_is_reported_once_on_stderr() {
    let temp = TempProject::new(json!({ "type": "module" }));
    temp.write("tsconfig.json", r#"{"compilerOptions":{"baseUrl":"."}}"#);
    temp.write("server.ts", "const ready: boolean = true;\nconsole.log(ready);\n");

    let output = temp.bento(&["run", "--allow-read=.", "server.ts"]).output().unwrap();

    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let report = stderr(&output);
    let warnings: Vec<&str> = report.lines().filter(|line| line.contains(": warning:")).collect();
    assert_eq!(warnings.len(), 1, "{report}");
    assert!(warnings[0].contains("baseUrl"), "{report}");
}

/// Runs the TypeScript module `source` as `name`, expects it to fail, and returns the report.
/// A styling terminal type is set, as in an ordinary shell, so that a report still free of
/// escape codes shows that piped output stays plain.
fn failure_report(name: &str, source: &str) -> String {
    let temp = TempProject::new(json!({ "type": "module" }));
    temp.write(name, source);

    let output = temp
        .bento(&["run", "--allow-read=.", name])
        .env("TERM", "xterm-256color")
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    let report = stderr(&output);
    assert!(!report.contains('\u{1b}'), "styled output when stderr is not a terminal: {report}");
    report
}

/// Asserts that `report` quotes `source_line` once and marks the start of `marked` in it
/// with `label`.
fn assert_marked(report: &str, source_line: &str, marked: &str, label: &str) {
    let lines: Vec<&str> = report.lines().collect();
    let quoted: Vec<usize> =
        (0..lines.len()).filter(|&at| lines[at].contains(source_line)).collect();
    assert_eq!(quoted.len(), 1, "`{source_line}` is not quoted exactly once: {report}");
    let quote = lines[quoted[0]];
    let marker = lines.get(quoted[0] + 1).and_then(|line| line.find('┬'));
    assert_eq!(marker, quote.find(marked), "`{marked}` is not marked: {report}");
    let labelled =
        lines.get(quoted[0] + 2).is_some_and(|line| line.ends_with(&format!("╰── {label}")));
    assert!(labelled, "the marker is not labelled `{label}`: {report}");
}

#[test]
fn a_typescript_syntax_error_is_shown_at_its_place_in_the_source() {
    let report = failure_report("syntax.ts", "\n\nconst value: number = ;\n");

    assert!(report.contains("a module could not be transformed"), "{report}");
    assert!(report.contains("syntax.ts:3:23"), "{report}");
    assert_marked(&report, "const value: number = ;", ";", "cannot be transformed here");
}

/// The enum becomes several lines of JavaScript, so only a report that maps the error back
/// to the TypeScript source shows the throwing line with its type annotation.
#[test]
fn an_uncaught_throw_in_typescript_is_shown_on_the_original_line() {
    let source = "enum Answer { Value = 42 }\n\nconst answer: number = Answer.Value;\n\n\n\
                  const failure: Error = new Error(`no ${answer}`);\nthrow failure;\n";

    let report = failure_report("throw.ts", source);

    assert!(report.contains("uncaught exception"), "{report}");
    assert!(report.contains("Error: no 42"), "{report}");
    assert!(report.contains("throw.ts:6:24"), "{report}");
    assert_marked(
        &report,
        "const failure: Error = new Error(`no ${answer}`);",
        "new",
        "thrown here",
    );
    let after_quote = report.split_once('╯').map_or("", |(_, after)| after);
    assert!(
        after_quote.contains("    at file://") && after_quote.contains("throw.ts:6:24"),
        "{report}"
    );
}

#[test]
fn an_unresolved_import_is_shown_at_the_import() {
    let source = "enum Answer { Value = 42 }\n\nimport answer from './absent.ts';\n\
                  console.log(answer, Answer.Value);\n";

    let report = failure_report("missing.ts", source);

    assert!(report.contains("a module could not be resolved"), "{report}");
    assert!(report.contains("./absent.ts"), "{report}");
    assert!(report.contains("missing.ts:3:20"), "{report}");
    assert_marked(&report, "import answer from './absent.ts';", "'", "imported here");
}
