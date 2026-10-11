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
