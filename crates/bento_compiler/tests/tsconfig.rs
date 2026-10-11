mod common;

use std::path::Path;

use bento_compiler::{CompilerSettings, Diagnostic, JsxMode, Severity};
use common::project;

fn load(root: &Path) -> (CompilerSettings, Vec<Diagnostic>) {
    CompilerSettings::load(root).unwrap_or_else(|error| panic!("{error}"))
}

fn remix_automatic() -> JsxMode {
    JsxMode::Automatic { import_source: "remix/component".to_owned(), development: false }
}

#[test]
fn missing_tsconfig_yields_defaults_without_diagnostics() {
    let dir = project(&[("package.json", "{}")]);
    let (settings, diagnostics) = load(dir.path());
    assert_eq!(settings, CompilerSettings::default());
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    assert_eq!(settings.jsx, JsxMode::Unavailable { configured: None });
    assert!(!settings.verbatim_module_syntax);
    assert!(!settings.experimental_decorators);
    assert!(settings.use_define_for_class_fields);
}

#[test]
fn jsonc_comments_and_trailing_commas_are_accepted() {
    let dir = project(&[(
        "tsconfig.json",
        r#"{
    // Line comment
    "compilerOptions": {
        /* block comment */
        "jsx": "react-jsx",
        "jsxImportSource": "remix/component",
        "verbatimModuleSyntax": true,
    },
}"#,
    )]);
    let (settings, _) = load(dir.path());
    assert_eq!(settings.jsx, remix_automatic());
    assert!(settings.verbatim_module_syntax);
}

#[test]
fn extends_chain_resolves_files_and_packages_with_nearer_files_winning() {
    let dir = project(&[
        (
            "tsconfig.json",
            r#"{
    "extends": ["./config/base-a.json", "./config/base-b.json"],
    "compilerOptions": { "jsxImportSource": "remix/component", "experimentalDecorators": false }
}"#,
        ),
        ("config/base-a.json", r#"{ "compilerOptions": { "jsx": "react", "jsxFactory": "h" } }"#),
        (
            "config/base-b.json",
            r#"{ "extends": "shared-config", "compilerOptions": { "jsx": "react-jsx", "jsxImportSource": "preact" } }"#,
        ),
        (
            "node_modules/shared-config/package.json",
            r#"{ "name": "shared-config", "exports": { ".": "./tsconfig.json" } }"#,
        ),
        (
            "node_modules/shared-config/tsconfig.json",
            r#"{ "extends": "@acme/presets/strict.json", "compilerOptions": { "verbatimModuleSyntax": true } }"#,
        ),
        ("node_modules/@acme/presets/package.json", r#"{ "name": "@acme/presets" }"#),
        (
            "node_modules/@acme/presets/strict.json",
            r#"{ "compilerOptions": { "experimentalDecorators": true, "emitDecoratorMetadata": true, "useDefineForClassFields": false } }"#,
        ),
    ]);
    let (settings, diagnostics) = load(dir.path());

    assert_eq!(settings.jsx, remix_automatic(), "{diagnostics:?}");
    assert!(settings.verbatim_module_syntax);
    assert!(!settings.experimental_decorators);
    assert!(settings.emit_decorator_metadata);
    assert!(!settings.use_define_for_class_fields);
}

#[test]
fn malformed_tsconfig_is_a_config_error_naming_the_file() {
    let dir = project(&[("tsconfig.json", "{ \"compilerOptions\": { \"jsx\": } }")]);
    let error = CompilerSettings::load(dir.path()).expect_err("malformed tsconfig");
    let expected = dir.path().join("tsconfig.json");
    assert_eq!(error.path, expected);
    assert!(error.to_string().contains(&expected.display().to_string()), "{error}");
    assert!(error.message.contains("line 1 column 31"), "{error}");
}

#[test]
fn malformed_extended_tsconfig_names_the_extended_file() {
    let dir = project(&[
        ("tsconfig.json", r#"{ "extends": "./broken.json" }"#),
        ("broken.json", "{ nope"),
    ]);
    let error = CompilerSettings::load(dir.path()).expect_err("malformed base");
    assert_eq!(error.path, dir.path().join("broken.json"));
}

#[test]
fn missing_extends_target_is_a_config_error() {
    let dir = project(&[("tsconfig.json", r#"{ "extends": "nonexistent-preset" }"#)]);
    let error = CompilerSettings::load(dir.path()).expect_err("missing extends target");
    assert!(error.to_string().contains("nonexistent-preset"), "{error}");
}

#[test]
fn tsconfig_directory_is_a_config_error_naming_the_path() {
    let dir = project(&[("tsconfig.json/nested.txt", "")]);
    let error = CompilerSettings::load(dir.path()).expect_err("directory named tsconfig.json");
    let expected = dir.path().join("tsconfig.json");
    assert_eq!(error.path, expected);
    assert!(error.to_string().contains(&expected.display().to_string()), "{error}");
    assert!(error.message.contains("not a file"), "{error}");
}

#[cfg(unix)]
#[test]
fn dangling_tsconfig_symlink_is_a_config_error_naming_the_path() {
    let dir = project(&[("package.json", "{}")]);
    let expected = dir.path().join("tsconfig.json");
    std::os::unix::fs::symlink("missing-base.json", &expected).unwrap();
    let error = CompilerSettings::load(dir.path()).expect_err("dangling tsconfig symlink");
    assert_eq!(error.path, expected);
    assert!(error.to_string().contains(&expected.display().to_string()), "{error}");
    assert!(error.message.contains("missing-base.json"), "{error}");
}

#[cfg(unix)]
#[test]
fn unreadable_tsconfig_is_a_config_error_naming_the_path() {
    use std::os::unix::fs::PermissionsExt;

    let dir = project(&[("tsconfig.json", "{}")]);
    let expected = dir.path().join("tsconfig.json");
    std::fs::set_permissions(&expected, std::fs::Permissions::from_mode(0o000)).unwrap();
    let error = CompilerSettings::load(dir.path()).expect_err("unreadable tsconfig");
    assert_eq!(error.path, expected);
    assert!(error.to_string().contains(&expected.display().to_string()), "{error}");
    assert!(error.message.to_lowercase().contains("permission denied"), "{error}");
}

#[test]
fn fixture_tsconfig_loads_without_error_diagnostics() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/remix-guest-book");
    let (settings, diagnostics) = load(&fixture);

    assert_eq!(settings.jsx, remix_automatic());
    assert!(settings.verbatim_module_syntax);
    assert!(settings.use_define_for_class_fields);
    assert!(!settings.experimental_decorators);
    assert!(
        diagnostics.iter().all(|diagnostic| diagnostic.severity != Severity::Error),
        "{diagnostics:?}"
    );
}

#[test]
fn type_checker_only_options_produce_no_diagnostics() {
    let dir = project(&[(
        "tsconfig.json",
        r##"{
    "compilerOptions": {
        "target": "es2020",
        "module": "nodenext",
        "moduleResolution": "nodenext",
        "lib": ["ES2024", "DOM"],
        "types": ["@types/node"],
        "strict": true,
        "skipLibCheck": true,
        "noEmit": true,
        "allowImportingTsExtensions": true,
        "paths": { "#app/*": ["./app/*"] }
    }
}"##,
    )]);
    let (settings, diagnostics) = load(dir.path());
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    assert!(!settings.use_define_for_class_fields, "target es2020 implies Set semantics");
}

#[test]
fn resolution_changing_options_warn_but_do_not_error() {
    let dir = project(&[(
        "tsconfig.json",
        r##"{
    "compilerOptions": {
        "baseUrl": ".",
        "paths": { "@/*": ["./src/*"], "#app/*": ["./app/*"] },
        "module": "commonjs"
    }
}"##,
    )]);
    let (_, diagnostics) = load(dir.path());
    assert!(diagnostics.iter().all(|diagnostic| diagnostic.severity == Severity::Warning));
    let options: Vec<&str> =
        diagnostics.iter().map(|diagnostic| diagnostic.option.as_str()).collect();
    assert_eq!(options, ["baseUrl", "paths", "module"], "{diagnostics:?}");
    let paths_warning = &diagnostics[1];
    assert!(paths_warning.message.contains("@/*"), "{paths_warning}");
    assert!(!paths_warning.message.contains("#app/*"), "{paths_warning}");
}

#[test]
fn unexecutable_jsx_settings_are_error_diagnostics_at_load() {
    for configured in ["preserve", "react-native", "bogus"] {
        let dir = project(&[(
            "tsconfig.json",
            &format!(r#"{{ "compilerOptions": {{ "jsx": "{configured}" }} }}"#),
        )]);
        let (settings, diagnostics) = load(dir.path());
        assert_eq!(settings.jsx, JsxMode::Unavailable { configured: Some(configured.to_owned()) });
        let errors: Vec<&Diagnostic> = diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.severity == Severity::Error)
            .collect();
        assert_eq!(errors.len(), 1, "{diagnostics:?}");
        assert_eq!(errors[0].option, "jsx");
        assert!(errors[0].message.contains(configured), "{}", errors[0]);
    }
}

#[test]
fn classic_jsx_settings_default_to_react_factories() {
    let dir = project(&[("tsconfig.json", r#"{ "compilerOptions": { "jsx": "react" } }"#)]);
    let (settings, _) = load(dir.path());
    assert_eq!(
        settings.jsx,
        JsxMode::Classic {
            factory: "React.createElement".to_owned(),
            fragment_factory: "React.Fragment".to_owned(),
        }
    );

    let dir = project(&[("tsconfig.json", r#"{ "compilerOptions": { "jsx": "react-jsxdev" } }"#)]);
    let (settings, _) = load(dir.path());
    assert_eq!(
        settings.jsx,
        JsxMode::Automatic { import_source: "react".to_owned(), development: true }
    );
}
