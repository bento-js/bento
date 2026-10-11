use std::path::Path;

use bento_compiler::{CompilerSettings, transform};

#[test]
fn syntax_errors_report_path_line_column_and_parser_message() {
    let source = "const ok = 1;\nconst broken: number = ;\n";
    let error =
        transform(Path::new("/project/src/broken.ts"), source, &CompilerSettings::default())
            .expect_err("syntax error");

    assert_eq!(error.path, Path::new("/project/src/broken.ts"));
    assert_eq!((error.line, error.column), (2, 24));
    let message = error.to_string();
    assert!(message.starts_with("/project/src/broken.ts:2:24: "), "{message}");
    assert!(message.contains("Unexpected token"), "{message}");
}

#[test]
fn syntax_errors_in_tsx_point_at_the_offending_token() {
    let source = "export const el = <div>\n";
    let error = transform(
        Path::new("/project/src/open.tsx"),
        source,
        &CompilerSettings {
            jsx: bento_compiler::JsxMode::Automatic {
                import_source: "remix/component".to_owned(),
                development: false,
            },
            ..CompilerSettings::default()
        },
    )
    .expect_err("unclosed element");
    assert_eq!(error.path, Path::new("/project/src/open.tsx"));
    assert_eq!(error.line, 1);
    assert!(error.column >= 19, "{error}");
}

#[test]
fn unsupported_extensions_are_rejected_before_parsing() {
    let error =
        transform(Path::new("/project/src/plain.js"), "let x = 1;", &CompilerSettings::default())
            .expect_err("plain JavaScript is not transformed");
    assert!(error.to_string().contains("plain.js"), "{error}");
    assert!(error.to_string().contains(".tsx"), "{error}");
}
