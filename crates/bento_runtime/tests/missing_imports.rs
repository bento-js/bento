mod support;

use bento_runtime::{ErrorKind, RunOutcome, start};
use support::{completed, config, project, run};

#[test]
fn failed_relative_and_absolute_imports_remain_watch_inputs() {
    let root = project();
    std::fs::create_dir_all(root.path().join("app")).unwrap();
    std::fs::create_dir(root.path().join("sibling")).unwrap();
    let app = root.path().join("app");
    let expected = root.path().canonicalize().unwrap().join("sibling/missing.mjs");
    for specifier in ["../sibling/missing.mjs".to_owned(), expected.to_string_lossy().into_owned()]
    {
        let entry = app.join("entry.mjs");
        std::fs::write(&entry, format!("import {specifier:?};")).unwrap();
        let mut settings = config(&app);
        settings.grants.read = bento_host::Scope::All;
        let runtime = start(settings, &entry).unwrap();
        let handle = runtime.handle();
        let outcome = runtime.wait();
        assert!(
            matches!(outcome, RunOutcome::Failed(error) if error.kind == ErrorKind::Resolution)
        );
        assert!(
            handle.watch_inputs().contains(&expected),
            "missing candidate not recorded: {:?}",
            handle.watch_inputs()
        );
    }
    std::fs::write(&expected, "export const repaired = true;").unwrap();
    let mut settings = config(&app);
    settings.grants.read = bento_host::Scope::All;
    completed(run(settings, "repaired.mjs", "import '../sibling/missing.mjs';"));
}

fn watch_inputs(source: &str, commonjs: bool) -> (tempfile::TempDir, Vec<std::path::PathBuf>) {
    let root = project();
    let app = root.path().join("app");
    std::fs::create_dir(&app).unwrap();
    std::fs::write(app.join("package.json"), r#"{"type":"module"}"#).unwrap();
    let entry = app.join(if commonjs { "entry.cjs" } else { "entry.mjs" });
    std::fs::write(
        &entry,
        source.replace("$ROOT", root.path().canonicalize().unwrap().to_str().unwrap()),
    )
    .unwrap();
    let mut settings = config(&app);
    settings.grants.read = bento_host::Scope::All;
    let runtime = start(settings, &entry).unwrap();
    let handle = runtime.handle();
    let _ = runtime.wait();
    (root, handle.watch_inputs())
}

#[test]
fn failed_commonjs_resolution_records_every_file_candidate() {
    for (source, commonjs) in [
        ("require('../shared/deep/missing');", true),
        ("require('$ROOT/shared/deep/missing');", true),
        ("require.resolve('../shared/deep/missing');", true),
        ("require('file://$ROOT/shared/deep/missing');", true),
    ] {
        let (root, inputs) = watch_inputs(source, commonjs);
        for candidate in ["missing", "missing.js", "missing.json", "missing.node"] {
            let expected = root.path().canonicalize().unwrap().join("shared/deep").join(candidate);
            assert!(inputs.contains(&expected), "{source}: missing {expected:?} in {inputs:?}");
        }
    }
}

#[test]
fn failed_file_url_and_dynamic_imports_remain_watch_inputs() {
    for (source, commonjs) in [
        ("import 'file://$ROOT/shared/deep/missing.mjs';", false),
        ("await import('../shared/deep/missing.mjs');", false),
        ("import('file://$ROOT/shared/deep/missing.mjs');", true),
    ] {
        let (root, inputs) = watch_inputs(source, commonjs);
        let expected = root.path().canonicalize().unwrap().join("shared/deep/missing.mjs");
        assert!(inputs.contains(&expected), "{source}: {inputs:?}");
    }
}

#[test]
fn external_package_scopes_and_missing_nearer_scopes_are_watch_inputs() {
    let root = project();
    let app = root.path().join("app");
    std::fs::create_dir(&app).unwrap();
    std::fs::create_dir_all(root.path().join("shared/nested")).unwrap();
    std::fs::write(app.join("package.json"), r#"{"type":"module"}"#).unwrap();
    std::fs::write(root.path().join("shared/package.json"), r#"{"type":"module"}"#).unwrap();
    std::fs::write(root.path().join("shared/nested/value.js"), "export const message = 'scope';")
        .unwrap();
    let entry = app.join("entry.mjs");
    std::fs::write(&entry, "import '../shared/nested/value.js';").unwrap();
    let mut settings = config(&app);
    settings.grants.read = bento_host::Scope::All;
    let runtime = start(settings, &entry).unwrap();
    let handle = runtime.handle();
    completed(runtime.wait());
    let inputs = handle.watch_inputs();
    let base = root.path().canonicalize().unwrap();
    for relative in ["app/package.json", "shared/package.json", "shared/nested/package.json"] {
        assert!(inputs.contains(&base.join(relative)), "{relative}: {inputs:?}");
    }
    assert!(!inputs.contains(&base.join("package.json")), "{inputs:?}");
}

#[test]
fn successful_require_resolve_does_not_add_sources_or_package_scopes() {
    let root = project();
    let app = root.path().join("app");
    std::fs::create_dir(&app).unwrap();
    std::fs::create_dir_all(root.path().join("shared/nested")).unwrap();
    std::fs::write(app.join("package.json"), "{}").unwrap();
    std::fs::write(root.path().join("shared/package.json"), "{}").unwrap();
    let source = root.path().join("shared/nested/value.cjs");
    std::fs::write(&source, "exports.message = 'scope';").unwrap();
    let entry = app.join("entry.cjs");
    std::fs::write(&entry, "require.resolve('../shared/nested/value.cjs');").unwrap();
    let mut settings = config(&app);
    settings.grants.read = bento_host::Scope::All;
    let runtime = start(settings, &entry).unwrap();
    let handle = runtime.handle();
    completed(runtime.wait());
    let inputs = handle.watch_inputs();
    let base = root.path().canonicalize().unwrap();
    for relative in ["shared/nested/value.cjs", "shared/package.json", "shared/nested/package.json"]
    {
        assert!(!inputs.contains(&base.join(relative)), "{relative}: {inputs:?}");
    }
}

#[test]
fn explicit_sources_beside_the_project_are_not_ancestor_scope_searches() {
    for (source, commonjs) in
        [("import '../missing.mjs';", false), ("require('../missing.cjs');", true)]
    {
        let (root, inputs) = watch_inputs(source, commonjs);
        let base = root.path().canonicalize().unwrap();
        let candidate = base.join(if commonjs { "missing.cjs" } else { "missing.mjs" });
        assert!(inputs.contains(&candidate), "{source}: {inputs:?}");
        assert!(!inputs.contains(&base.join("package.json")), "{inputs:?}");
    }
}
