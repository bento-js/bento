mod support;

use std::path::{Path, PathBuf};

use bento_runtime::start;
use support::{completed, config, project, run};

fn watch_inputs(app: &Path, entry: &str) -> Vec<PathBuf> {
    let mut settings = config(app);
    settings.grants.read = bento_host::Scope::All;
    let runtime = start(settings, &app.join(entry)).unwrap();
    let handle = runtime.handle();
    completed(runtime.wait());
    handle.watch_inputs()
}

fn dependency_walk_inputs(entry: &str, dependency_source: &str) {
    let root = project();
    let app = root.path().join("app");
    let dependency = app.join("node_modules/config-loader");
    std::fs::create_dir_all(&dependency).unwrap();
    std::fs::write(app.join("package.json"), "{}").unwrap();
    let dependency_entry = if entry.ends_with(".cjs") { "index.cjs" } else { "index.mjs" };
    std::fs::write(
        dependency.join("package.json"),
        serde_json::json!({ "name": "config-loader", "main": dependency_entry }).to_string(),
    )
    .unwrap();
    std::fs::write(dependency.join(dependency_entry), dependency_source).unwrap();
    std::fs::write(
        app.join(entry),
        if entry.ends_with(".cjs") {
            "require('config-loader')(__dirname).then(console.log);"
        } else {
            "import { find } from 'config-loader'; console.log(await find(import.meta.dirname));"
        },
    )
    .unwrap();

    let recorded = watch_inputs(&app, entry);
    let base = root.path().canonicalize().unwrap();
    for ancestor in base.join("app").ancestors() {
        assert!(
            !recorded.contains(&ancestor.join("probe.config.mjs")),
            "{entry}: ancestor {ancestor:?} in {recorded:?}"
        );
    }
    assert_eq!(recorded, vec![base.join("app").join(entry), base.join("app/package.json")]);
}

#[test]
fn proposal_0001_dependency_esm_dynamic_file_url_walk_records_no_ancestor_inputs() {
    dependency_walk_inputs(
        "main.mjs",
        "import { dirname, join } from 'node:path'; import { pathToFileURL } from 'node:url'; export async function find(start) { for (let dir = start; ; dir = dirname(dir)) { try { return await import(pathToFileURL(join(dir, 'probe.config.mjs')).href); } catch {} if (dir === dirname(dir)) return 'NO CONFIG'; } }",
    );
}

#[test]
fn proposal_0001_dependency_cjs_dynamic_file_url_walk_records_no_ancestor_inputs() {
    dependency_walk_inputs(
        "main.cjs",
        "const { dirname, join } = require('node:path'); const { pathToFileURL } = require('node:url'); module.exports = async (start) => { for (let dir = start; ; dir = dirname(dir)) { try { return await import(pathToFileURL(join(dir, 'probe.config.mjs')).href); } catch {} if (dir === dirname(dir)) return 'NO CONFIG'; } };",
    );
}

#[test]
fn proposal_0001_dependency_dynamic_import_of_application_supplied_url_records_nothing() {
    let root = project();
    let app = root.path().join("app");
    let dependency = app.join("node_modules/plugin-host");
    std::fs::create_dir_all(&dependency).unwrap();
    std::fs::write(app.join("package.json"), r#"{"type":"module"}"#).unwrap();
    std::fs::write(
        dependency.join("package.json"),
        r#"{"name":"plugin-host","type":"module","exports":"./index.js"}"#,
    )
    .unwrap();
    std::fs::write(
        dependency.join("index.js"),
        "export async function load(url) { try { await import(url); } catch {} }",
    )
    .unwrap();
    std::fs::write(
        app.join("main.mjs"),
        "import { load } from 'plugin-host'; await load(new URL('../optional-plugin.mjs', import.meta.url).href);",
    )
    .unwrap();

    let recorded = watch_inputs(&app, "main.mjs");
    let base = root.path().canonicalize().unwrap();
    assert_eq!(recorded, vec![base.join("app/main.mjs"), base.join("app/package.json")]);
}

#[test]
fn proposal_0001_application_dynamic_file_url_failure_retains_target_and_recovers() {
    for entry in ["main.mjs", "main.cjs"] {
        let root = project();
        let app = root.path().join("app");
        std::fs::create_dir(&app).unwrap();
        std::fs::write(app.join("package.json"), "{}").unwrap();
        let source = if entry.ends_with(".cjs") {
            "const { pathToFileURL } = require('node:url'); const path = require('node:path'); import(pathToFileURL(path.join(__dirname, '../optional-plugin.mjs')).href).catch(() => {});"
        } else {
            "try { await import(new URL('../optional-plugin.mjs', import.meta.url).href); } catch {}"
        };
        std::fs::write(app.join(entry), source).unwrap();

        let recorded = watch_inputs(&app, entry);
        let target = root.path().canonicalize().unwrap().join("optional-plugin.mjs");
        assert!(recorded.contains(&target), "{entry}: {recorded:?}");
        std::fs::write(target, "export default 'RECOVERED';").unwrap();
        let repaired = if entry.ends_with(".cjs") {
            "const { pathToFileURL } = require('node:url'); const path = require('node:path'); import(pathToFileURL(path.join(__dirname, '../optional-plugin.mjs')).href).then(plugin => require('node:assert/strict').equal(plugin.default, 'RECOVERED'));"
        } else {
            "import assert from 'node:assert/strict'; assert.equal((await import(new URL('../optional-plugin.mjs', import.meta.url).href)).default, 'RECOVERED');"
        };
        let mut settings = config(&app);
        settings.grants.read = bento_host::Scope::All;
        completed(run(settings, entry, repaired));
    }
}

#[test]
fn proposal_0001_dependency_import_of_application_resolved_url_retains_failed_target() {
    let root = project();
    let app = root.path().join("app");
    let dependency = app.join("node_modules/plugin-host");
    std::fs::create_dir_all(&dependency).unwrap();
    std::fs::write(app.join("package.json"), r#"{"type":"module"}"#).unwrap();
    std::fs::write(
        dependency.join("package.json"),
        r#"{"name":"plugin-host","type":"module","exports":"./index.js"}"#,
    )
    .unwrap();
    std::fs::write(
        dependency.join("index.js"),
        "export async function load(url) { try { await import(url); } catch {} }",
    )
    .unwrap();
    std::fs::write(
        app.join("main.mjs"),
        "import { load } from 'plugin-host'; const url = import.meta.resolve(new URL('../optional-plugin.mjs', import.meta.url).href); await load(url);",
    )
    .unwrap();

    let recorded = watch_inputs(&app, "main.mjs");
    let base = root.path().canonicalize().unwrap();
    assert_eq!(
        recorded,
        vec![
            base.join("app/main.mjs"),
            base.join("app/package.json"),
            base.join("optional-plugin.mjs")
        ]
    );
}
