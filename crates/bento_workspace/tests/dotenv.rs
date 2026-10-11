mod support;

use std::collections::BTreeMap;

use bento_workspace::{EnvironmentError, LaunchEnvironment, LaunchKind};
use support::TempProject;

fn load_base_file(
    temp: &TempProject,
    contents: &str,
) -> Result<LaunchEnvironment, EnvironmentError> {
    temp.write(".env", contents);
    LaunchEnvironment::load(temp.root(), BTreeMap::new(), LaunchKind::Run)
}

fn value<'a>(environment: &'a LaunchEnvironment, name: &str) -> &'a str {
    environment.vars().get(name).unwrap_or_else(|| panic!("{name} is not set")).as_str()
}

#[test]
fn comments_blank_lines_and_export_prefix() {
    let temp = TempProject::new();
    let source = "# leading comment\n\n   \nexport EXPORTED=1\nSPACED = two \n  INDENTED=three # trailing comment\nEMPTY=\nexport  ALSO_EXPORTED='x'\n";

    let environment = load_base_file(&temp, source).unwrap();

    assert_eq!(value(&environment, "EXPORTED"), "1");
    assert_eq!(value(&environment, "SPACED"), "two");
    assert_eq!(value(&environment, "INDENTED"), "three");
    assert_eq!(value(&environment, "EMPTY"), "");
    assert_eq!(value(&environment, "ALSO_EXPORTED"), "x");
    assert_eq!(environment.vars().len(), 5);
}

#[test]
fn quoted_values_keep_their_own_rules() {
    let temp = TempProject::new();
    let source = concat!(
        "SINGLE='literal \\n $HOME \"double\"'\n",
        "DOUBLE=\"tab\\tline\\nquote\\\" backslash\\\\ dollar\\$\"\n",
        "BACKTICK=`it's \"both\" quotes`\n",
        "HASH_IN_DOUBLE=\"before # after\"\n",
        "HASH_IN_SINGLE='a#b'\n",
        "QUOTED_THEN_COMMENT=\"kept\" # dropped\n",
        "PADDED='  spaces  '\n",
    );

    let environment = load_base_file(&temp, source).unwrap();

    assert_eq!(value(&environment, "SINGLE"), "literal \\n $HOME \"double\"");
    assert_eq!(value(&environment, "DOUBLE"), "tab\tline\nquote\" backslash\\ dollar\\$");
    assert_eq!(value(&environment, "BACKTICK"), "it's \"both\" quotes");
    assert_eq!(value(&environment, "HASH_IN_DOUBLE"), "before # after");
    assert_eq!(value(&environment, "HASH_IN_SINGLE"), "a#b");
    assert_eq!(value(&environment, "QUOTED_THEN_COMMENT"), "kept");
    assert_eq!(value(&environment, "PADDED"), "  spaces  ");
}

#[test]
fn quoted_values_may_span_lines() {
    let temp = TempProject::new();
    let source =
        "KEY=\"-----BEGIN KEY-----\nabc\n-----END KEY-----\"\nSINGLE='one\ntwo'\nAFTER=after\n";

    let environment = load_base_file(&temp, source).unwrap();

    assert_eq!(value(&environment, "KEY"), "-----BEGIN KEY-----\nabc\n-----END KEY-----");
    assert_eq!(value(&environment, "SINGLE"), "one\ntwo");
    assert_eq!(value(&environment, "AFTER"), "after");
}

#[test]
fn windows_line_endings_are_accepted() {
    let temp = TempProject::new();

    let environment = load_base_file(&temp, "FIRST=1\r\nSECOND=\"two\"\r\n").unwrap();

    assert_eq!(value(&environment, "FIRST"), "1");
    assert_eq!(value(&environment, "SECOND"), "two");
}

#[test]
fn values_are_never_expanded_or_executed() {
    let temp = TempProject::new();
    let marker = temp.path("substituted");
    let source = format!(
        "COMMAND=$(touch {marker})\nQUOTED_COMMAND=\"$(touch {marker})\"\nBACKTICK=`touch {marker}`\nVARIABLE=$HOME\nBRACED=${{HOME}}/x\n",
        marker = marker.display()
    );

    let environment = load_base_file(&temp, &source).unwrap();

    assert_eq!(value(&environment, "COMMAND"), format!("$(touch {})", marker.display()));
    assert_eq!(value(&environment, "QUOTED_COMMAND"), format!("$(touch {})", marker.display()));
    assert_eq!(value(&environment, "BACKTICK"), format!("touch {}", marker.display()));
    assert_eq!(value(&environment, "VARIABLE"), "$HOME");
    assert_eq!(value(&environment, "BRACED"), "${HOME}/x");
    assert!(!marker.exists());
}

#[test]
fn malformed_lines_report_file_and_line_without_the_value() {
    let cases = [
        ("OK=1\nTOKEN hunter2-secret\n", 2),
        ("OK=1\n\nPASSWORD=\"hunter2-secret\nstill open\n", 3),
        ("TOKEN='hunter2-secret' trailing\n", 1),
        ("OK=1\nBAD NAME=hunter2-secret\n", 2),
        ("=hunter2-secret\n", 1),
    ];
    for (source, line) in cases {
        let temp = TempProject::new();
        let error = load_base_file(&temp, source).unwrap_err();

        assert!(matches!(error, EnvironmentError::Dotenv(_)), "{error:?}");
        let message = error.to_string();
        let location = format!("{}:{line}:", temp.path(".env").display());
        assert!(message.contains(&location), "expected {location} in {message}");
        assert!(!message.contains("hunter2"), "value leaked: {message}");
        assert!(!format!("{error:?}").contains("hunter2"), "value leaked: {error:?}");
    }
}
