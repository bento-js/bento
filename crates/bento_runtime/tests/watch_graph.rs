mod support;

use std::path::{Path, PathBuf};

use bento_runtime::{RunOutcome, start};
use support::{completed, config, project, run};

fn inputs(app: &Path, entry: &str) -> (RunOutcome, Vec<PathBuf>) {
    let mut settings = config(app);
    settings.grants.read = bento_host::Scope::All;
    let runtime = start(settings, &app.join(entry)).unwrap();
    let handle = runtime.handle();
    (runtime.wait(), handle.watch_inputs())
}

#[test]
fn esm_package_map_targets_remain_watch_inputs_when_missing() {
    for (entry, target, value) in [
        ("main.mjs", "target.mjs", "export { message } from '#message';"),
        ("main.mjs", "deep/target.mjs", "export { message } from '#message';"),
        ("main.mjs", "deep/target.mjs", "export { message } from 'workspace/message';"),
        ("main.mjs", "deep/target.mjs", "await import(import.meta.resolve('#message'));"),
        ("main.cjs", "deep/target.mjs", "export { message } from '#message';"),
    ] {
        let root = project();
        let app = root.path().join("app");
        std::fs::create_dir(&app).unwrap();
        std::fs::create_dir(root.path().join("shared")).unwrap();
        std::fs::write(app.join("package.json"), r#"{"type":"module"}"#).unwrap();
        let package = serde_json::json!({
            "name": "workspace",
            "type": "module",
            "imports": { "#message": format!("./{target}") },
            "exports": { "./message": format!("./{target}") }
        });
        std::fs::write(root.path().join("shared/package.json"), package.to_string()).unwrap();
        let source = if entry.ends_with(".cjs") {
            "import('../shared/value.mjs');"
        } else {
            "import '../shared/value.mjs';"
        };
        std::fs::write(root.path().join("shared/value.mjs"), value).unwrap();
        std::fs::write(app.join(entry), source).unwrap();

        let (outcome, recorded) = inputs(&app, entry);
        assert!(matches!(outcome, RunOutcome::Failed(_)), "{outcome:?}");
        let expected = root.path().canonicalize().unwrap().join("shared").join(target);
        assert!(
            recorded.contains(&expected),
            "{entry}, {value}: {expected:?} absent from {recorded:?}"
        );
        std::fs::create_dir_all(expected.parent().unwrap()).unwrap();
        std::fs::write(expected, "export const message = 'repaired';").unwrap();
        let mut settings = config(&app);
        settings.grants.read = bento_host::Scope::All;
        completed(run(settings, entry, source));
    }
}

#[test]
fn successful_import_meta_resolve_does_not_add_watch_inputs() {
    let root = project();
    let app = root.path().join("app");
    std::fs::create_dir(&app).unwrap();
    std::fs::create_dir(root.path().join("shared")).unwrap();
    std::fs::write(app.join("package.json"), r#"{"type":"module"}"#).unwrap();
    std::fs::write(root.path().join("shared/value.mjs"), "throw new Error('not loaded');").unwrap();
    std::fs::write(app.join("main.mjs"), "import.meta.resolve('../shared/value.mjs');").unwrap();

    let (outcome, recorded) = inputs(&app, "main.mjs");
    completed(outcome);
    let expected = root.path().canonicalize().unwrap().join("shared/value.mjs");
    assert!(!recorded.contains(&expected), "{expected:?} unexpectedly present in {recorded:?}");
}

#[test]
fn external_scope_creation_is_limited_to_the_source_directory() {
    let app_root = project();
    std::fs::write(app_root.path().join("package.json"), r#"{"type":"module"}"#).unwrap();
    let workspace = project();
    std::fs::create_dir_all(workspace.path().join("x/y/z")).unwrap();
    let source = workspace.path().join("x/y/z/value.mjs");
    std::fs::write(&source, "export {};").unwrap();
    std::fs::write(
        app_root.path().join("main.mjs"),
        format!("import {:?};", source.to_str().unwrap()),
    )
    .unwrap();

    let (outcome, recorded) = inputs(app_root.path(), "main.mjs");
    completed(outcome);
    let source = source.canonicalize().unwrap();
    assert!(recorded.contains(&source.parent().unwrap().join("package.json")), "{recorded:?}");
    for directory in source.parent().unwrap().ancestors().skip(1) {
        let package = directory.join("package.json");
        if !package.exists() {
            assert!(!recorded.contains(&package), "unrelated ancestor {package:?}: {recorded:?}");
        }
    }
}

#[test]
fn external_existing_scope_does_not_add_missing_intermediate_scopes() {
    let root = project();
    let app = root.path().join("app");
    std::fs::create_dir(&app).unwrap();
    std::fs::create_dir_all(root.path().join("shared/x/y/z")).unwrap();
    std::fs::write(app.join("package.json"), r#"{"type":"module"}"#).unwrap();
    std::fs::write(root.path().join("shared/package.json"), r#"{"type":"module"}"#).unwrap();
    std::fs::write(root.path().join("shared/x/y/z/value.mjs"), "export {};").unwrap();
    std::fs::write(app.join("main.mjs"), "import '../shared/x/y/z/value.mjs';").unwrap();

    let (outcome, recorded) = inputs(&app, "main.mjs");
    completed(outcome);
    let base = root.path().canonicalize().unwrap();
    for expected in ["shared/package.json", "shared/x/y/z/package.json"] {
        assert!(recorded.contains(&base.join(expected)), "{recorded:?}");
    }
    for excluded in ["shared/x/package.json", "shared/x/y/package.json"] {
        assert!(!recorded.contains(&base.join(excluded)), "{excluded}: {recorded:?}");
    }
}

#[test]
fn successful_commonjs_directory_resolution_discards_missing_probes() {
    let root = project();
    let app = root.path().join("app");
    std::fs::create_dir(&app).unwrap();
    std::fs::create_dir(root.path().join("shared")).unwrap();
    std::fs::write(app.join("package.json"), "{}").unwrap();
    std::fs::write(root.path().join("shared/index.js"), "exports.message = 'INDEX';").unwrap();
    std::fs::write(app.join("main.cjs"), "require('../shared');").unwrap();

    let (outcome, recorded) = inputs(&app, "main.cjs");
    completed(outcome);
    let base = root.path().canonicalize().unwrap();
    assert!(recorded.contains(&base.join("shared/index.js")), "{recorded:?}");
    assert!(!recorded.contains(&base.join("shared.js")), "{recorded:?}");
    assert!(!recorded.contains(&base.join("package.json")), "{recorded:?}");
}

#[test]
fn dependency_resolution_probes_are_not_watch_inputs() {
    for (entry, dependency, source) in [
        (
            "main.cjs",
            "index.cjs",
            "const path = require('node:path'); module.exports = (start) => { for (let dir = start; ; dir = path.dirname(dir)) { try { return require.resolve(path.join(dir, 'probe.config.js')); } catch {} if (dir === path.dirname(dir)) return; } };",
        ),
        ("main.mjs", "index.mjs", "await import('../../../probe.config.js');"),
        ("main.mjs", "index.cjs", "module.exports = require('../../../probe.config.js');"),
    ] {
        let root = project();
        let app = root.path().join("app");
        let package = app.join("node_modules/config-loader");
        std::fs::create_dir_all(&package).unwrap();
        std::fs::write(app.join("package.json"), "{}").unwrap();
        std::fs::write(
            package.join("package.json"),
            serde_json::json!({ "name": "config-loader", "main": dependency }).to_string(),
        )
        .unwrap();
        std::fs::write(package.join(dependency), source).unwrap();
        std::fs::write(
            app.join(entry),
            if entry.ends_with(".cjs") {
                "require('config-loader')(__dirname);"
            } else {
                "import 'config-loader';"
            },
        )
        .unwrap();

        let (_, recorded) = inputs(&app, entry);
        let base = root.path().canonicalize().unwrap();
        assert!(!recorded.contains(&base.join("probe.config.js")), "{recorded:?}");
        assert!(!recorded.contains(&PathBuf::from("/probe.config.js")), "{recorded:?}");
        assert!(recorded.iter().all(|path| !path.to_string_lossy().contains("node_modules")));
    }
}

#[test]
fn commonjs_package_map_failures_retain_non_dependency_targets() {
    for request in ["#message", "workspace/message"] {
        let root = project();
        let app = root.path().join("app");
        std::fs::create_dir(&app).unwrap();
        std::fs::create_dir(root.path().join("shared")).unwrap();
        std::fs::write(app.join("package.json"), "{}").unwrap();
        std::fs::write(
            root.path().join("shared/package.json"),
            r##"{"name":"workspace","imports":{"#message":"./deep/missing.cjs"},"exports":{"./message":"./deep/missing.cjs"}}"##,
        )
        .unwrap();
        std::fs::write(
            root.path().join("shared/value.cjs"),
            format!("module.exports = require({request:?});"),
        )
        .unwrap();
        std::fs::write(app.join("main.cjs"), "require('../shared/value.cjs');").unwrap();

        let (outcome, recorded) = inputs(&app, "main.cjs");
        assert!(matches!(outcome, RunOutcome::Failed(_)), "{outcome:?}");
        let missing = root.path().canonicalize().unwrap().join("shared/deep/missing.cjs");
        assert!(
            recorded.contains(&missing),
            "{request}: {missing:?} absent from {recorded:?}: {outcome:?}"
        );
    }
}

#[test]
fn commonjs_reexports_loaded_by_esm_keep_failed_targets() {
    for (request, target) in [
        ("./missing.cjs", "missing.cjs"),
        ("./missing", "missing.js"),
        ("./missing", "missing.json"),
    ] {
        let root = project();
        let app = root.path().join("app");
        std::fs::create_dir(&app).unwrap();
        std::fs::create_dir(root.path().join("shared")).unwrap();
        std::fs::write(app.join("package.json"), r#"{"type":"module"}"#).unwrap();
        std::fs::write(
            root.path().join("shared/value.cjs"),
            format!("module.exports = require({request:?});"),
        )
        .unwrap();
        std::fs::write(app.join("main.mjs"), "import '../shared/value.cjs';").unwrap();

        let (outcome, recorded) = inputs(&app, "main.mjs");
        assert!(matches!(outcome, RunOutcome::Failed(_)), "{outcome:?}");
        let missing = root.path().canonicalize().unwrap().join("shared").join(target);
        assert!(recorded.contains(&missing), "{missing:?} absent from {recorded:?}: {outcome:?}");
    }
}
