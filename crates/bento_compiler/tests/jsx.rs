mod common;

use std::path::Path;

use bento_compiler::{CompilerSettings, JsxMode, transform};
use common::{automatic, classic, code};

const COMPONENT: &str = r#"
export function Greeting({ name }: { name: string }) {
    return (
        <>
            <h1 className="title">Hello {name}</h1>
            <p>Welcome</p>
        </>
    );
}
"#;

/// The names imported from `module`, as exported by that module (before any `as` alias).
fn imported_names(output: &str, module: &str) -> Vec<String> {
    let from = format!(" from \"{module}\"");
    let line = output
        .lines()
        .find(|line| line.starts_with("import {") && line.contains(&from))
        .unwrap_or_else(|| panic!("no import from {module} in:\n{output}"));
    let inside = &line["import {".len()..line.find('}').unwrap()];
    let mut names: Vec<String> = inside
        .split(',')
        .map(|specifier| specifier.trim().split(" as ").next().unwrap().to_owned())
        .collect();
    names.sort();
    names
}

#[test]
fn automatic_runtime_imports_from_the_configured_import_source() {
    let output = code("/project/app/greeting.tsx", COMPONENT, &automatic("remix/component"));

    assert_eq!(
        imported_names(&output, "remix/component/jsx-runtime"),
        ["Fragment", "jsx", "jsxs"],
        "{output}"
    );
    assert!(output.contains(r#"jsxs("h1", {"#), "{output}");
    assert!(output.contains(r#"jsx("p", {"#), "{output}");
    assert!(output.contains("Fragment, { children: ["), "{output}");
    assert!(!output.contains("<h1"), "{output}");
    assert!(!output.contains("React"), "{output}");
}

#[test]
fn automatic_runtime_imports_only_the_factories_it_uses() {
    let output = code(
        "/project/app/single.tsx",
        r#"export const el = <span>one</span>;"#,
        &automatic("remix/component"),
    );
    assert_eq!(imported_names(&output, "remix/component/jsx-runtime"), ["jsx"], "{output}");
    assert!(!output.contains("jsxs"), "{output}");
    assert!(!output.contains("Fragment"), "{output}");
}

#[test]
fn development_runtime_targets_the_dev_runtime_module() {
    let settings = CompilerSettings {
        jsx: JsxMode::Automatic { import_source: "remix/component".to_owned(), development: true },
        ..CompilerSettings::default()
    };
    let output = code("/project/app/greeting.tsx", COMPONENT, &settings);

    assert!(output.contains(r#"from "remix/component/jsx-dev-runtime""#), "{output}");
    assert!(output.contains("jsxDEV("), "{output}");
    assert!(output.contains("lineNumber"), "{output}");
}

#[test]
fn classic_runtime_honors_the_factory_and_fragment_factory() {
    let source = format!(
        "import {{ h, Frag }} from \"preact\";\n{}",
        COMPONENT.replace("<>", "<Frag>").replace("</>", "</Frag>")
    );
    let output = code("/project/app/greeting.tsx", &source, &classic("h", "Frag"));

    assert!(output.contains(r#"import { h, Frag } from "preact""#), "{output}");
    assert!(output.contains(r#"h("h1", { className: "title" }"#), "{output}");
    assert!(output.contains("h(Frag, null"), "{output}");
    assert!(!output.contains("jsx-runtime"), "{output}");
    assert!(!output.contains("React.createElement"), "{output}");
}

#[test]
fn classic_runtime_fragment_shorthand_uses_the_fragment_factory() {
    let output = code("/project/app/greeting.tsx", COMPONENT, &classic("h", "Frag"));
    assert!(output.contains("h(Frag, null"), "{output}");
}

#[test]
fn jsx_files_use_the_same_settings_as_tsx_files() {
    let output = code(
        "/project/app/legacy.jsx",
        "export const el = <main id=\"root\">{children}</main>;",
        &automatic("remix/component"),
    );
    assert_eq!(imported_names(&output, "remix/component/jsx-runtime"), ["jsx"], "{output}");
    assert!(output.contains(r#"jsx("main", {"#), "{output}");
}

#[test]
fn preserve_mode_rejects_jsx_files_with_an_actionable_error() {
    for configured in ["preserve", "react-native"] {
        let settings = CompilerSettings {
            jsx: JsxMode::Unavailable { configured: Some(configured.to_owned()) },
            ..CompilerSettings::default()
        };
        let error = transform(Path::new("/project/app/greeting.tsx"), COMPONENT, &settings)
            .expect_err(configured);
        let message = error.to_string();
        assert!(message.contains("/project/app/greeting.tsx:4:9"), "{message}");
        assert!(message.contains(&format!("\"jsx\": \"{configured}\"")), "{message}");
        assert!(message.contains("react-jsx"), "{message}");

        let error = transform(Path::new("/project/app/legacy.jsx"), "const a = <b/>;", &settings)
            .expect_err(configured);
        assert!(error.to_string().contains(configured), "{error}");
    }
}

#[test]
fn unset_jsx_setting_rejects_jsx_with_an_actionable_error() {
    let error =
        transform(Path::new("/project/app/greeting.tsx"), COMPONENT, &CompilerSettings::default())
            .expect_err("jsx without a jsx setting");
    let message = error.to_string();
    assert!(message.contains("/project/app/greeting.tsx:4:9"), "{message}");
    assert!(message.contains("\"jsx\""), "{message}");
    assert!(message.contains("not set"), "{message}");
}

#[test]
fn preserve_mode_still_transforms_typescript_without_jsx() {
    let settings = CompilerSettings {
        jsx: JsxMode::Unavailable { configured: Some("preserve".to_owned()) },
        ..CompilerSettings::default()
    };
    let output = code("/project/app/util.ts", "export const n: number = 1;", &settings);
    assert_eq!(output.trim(), "export const n = 1;");

    let output = code("/project/app/no-jsx.tsx", "export const n: number = 2;", &settings);
    assert_eq!(output.trim(), "export const n = 2;");
}
