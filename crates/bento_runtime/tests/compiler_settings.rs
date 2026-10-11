mod support;

use std::collections::BTreeSet;
use std::fs;

use bento_host::Scope;
use bento_runtime::{ErrorKind, RunOutcome};
use support::{completed, config, run};

#[test]
fn settings_are_loaded_once_and_warnings_are_printed_once_per_instance() {
    if let Some(root) = std::env::var_os("BENTO_SETTINGS_TEST_ROOT") {
        compiler_settings_child(std::path::Path::new(&root));
        return;
    }
    let root = support::project();
    std::fs::write(root.path().join("package.json"), r#"{"type":"module"}"#).unwrap();
    std::fs::write(
        root.path().join("tsconfig.json"),
        r#"{"compilerOptions":{"baseUrl":".","useDefineForClassFields":false}}"#,
    )
    .unwrap();
    for name in ["first.ts", "second.ts"] {
        std::fs::write(root.path().join(name), "class Base { set field(value: string) { this.seen = value; } } export const instance = new (class extends Base { field = 'assigned'; })();").unwrap();
    }
    let child = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "settings_are_loaded_once_and_warnings_are_printed_once_per_instance",
            "--nocapture",
        ])
        .env("BENTO_SETTINGS_TEST_ROOT", root.path())
        .output()
        .unwrap();
    assert!(
        child.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&child.stdout),
        String::from_utf8_lossy(&child.stderr)
    );
    let stderr = String::from_utf8(child.stderr).unwrap();
    assert_eq!(stderr.matches(": warning:").count(), 1, "{stderr}");
    assert!(stderr.contains("baseUrl"), "{stderr}");
    eprintln!("captured instance warning: {}", stderr.trim());
}

fn compiler_settings_child(root: &std::path::Path) {
    completed(run(
        config(root),
        "settings.mjs",
        r#"
import assert from 'node:assert/strict';
import { writeFileSync } from 'node:fs';
import { instance as first } from './first.ts';
assert.equal(first.seen, 'assigned');
writeFileSync('tsconfig.json', JSON.stringify({ compilerOptions: { useDefineForClassFields: true } }));
const { instance: second } = await import('./second.ts');
assert.equal(second.seen, 'assigned');
"#,
    ));
}

#[test]
fn malformed_tsconfig_fails_start_as_a_transform_error() {
    let root = support::project();
    let entry = root.path().join("entry.ts");
    std::fs::write(&entry, "console.log('must not execute');").unwrap();
    std::fs::write(root.path().join("tsconfig.json"), "{ malformed").unwrap();
    let error = match bento_runtime::start(config(root.path()), &entry) {
        Ok(_) => panic!("malformed tsconfig started execution"),
        Err(error) => error,
    };
    assert_eq!(error.kind, bento_runtime::ErrorKind::Transform, "{error:?}");
    assert!(error.message.contains("tsconfig.json"), "{error}");
}

#[test]
fn external_tsx_uses_project_settings_when_included() {
    completed(external_tsx_outcome(
        r#"{"compilerOptions":{"jsx":"react","jsxFactory":"h"},"include":["*.ts","../shared/view.tsx"]}"#,
    ));
}

#[test]
fn external_tsx_uses_project_settings_without_include() {
    completed(external_tsx_outcome(r#"{"compilerOptions":{"jsx":"react","jsxFactory":"h"}}"#));
}

fn external_tsx_outcome(tsconfig: &str) -> RunOutcome {
    let workspace = support::project();
    let root = workspace.path().join("app");
    let sibling = workspace.path().join("shared");
    fs::create_dir_all(&root).unwrap();
    fs::create_dir_all(&sibling).unwrap();
    fs::write(workspace.path().join("package.json"), r#"{"type":"module"}"#).unwrap();
    fs::write(root.join("tsconfig.json"), tsconfig).unwrap();
    fs::write(
        sibling.join("view.tsx"),
        r#"
function h(tag: string, props: unknown, ...children: unknown[]) { return { tag, children }; }
export const view = <section>external</section>;
"#,
    )
    .unwrap();
    let mut launch = config(&root);
    launch.grants.read = Scope::Only(BTreeSet::from([workspace.path().canonicalize().unwrap()]));
    run(
        launch,
        "entry.ts",
        r#"
import assert from 'node:assert/strict';
import { view } from '../shared/view.tsx';
assert.deepEqual(view, { tag: 'section', children: ['external'] });
"#,
    )
}

#[test]
fn external_node_modules_typescript_uses_default_class_field_settings() {
    let workspace = support::project();
    let root = workspace.path().join("app");
    let dependency = workspace.path().join("node_modules/fields");
    fs::create_dir_all(&root).unwrap();
    fs::create_dir_all(&dependency).unwrap();
    fs::write(workspace.path().join("package.json"), r#"{"type":"module"}"#).unwrap();
    fs::write(
        root.join("tsconfig.json"),
        r#"{"compilerOptions":{"useDefineForClassFields":false}}"#,
    )
    .unwrap();
    fs::write(
        dependency.join("entry.ts"),
        "class Base { set field(value: string) { throw new Error('dependency inherited project settings'); } } class Derived extends Base { field = 'own'; } export default new Derived().field;",
    )
    .unwrap();
    let mut launch = config(&root);
    launch.grants.read = Scope::Only(BTreeSet::from([workspace.path().canonicalize().unwrap()]));
    completed(run(
        launch,
        "entry.ts",
        r#"
import assert from 'node:assert/strict';
import field from '../node_modules/fields/entry.ts';
assert.equal(field, 'own');
"#,
    ));
}

#[test]
fn node_modules_tsx_does_not_inherit_the_project_jsx_settings() {
    let workspace = support::project();
    let root = workspace.path().join("app");
    fs::create_dir_all(&root).unwrap();
    fs::write(workspace.path().join("package.json"), r#"{"type":"module"}"#).unwrap();
    fs::write(
        root.join("tsconfig.json"),
        r#"{"compilerOptions":{"jsx":"react","jsxFactory":"h"}}"#,
    )
    .unwrap();
    for directory in [root.join("node_modules/view"), workspace.path().join("node_modules/view")] {
        fs::create_dir_all(&directory).unwrap();
        fs::write(
            directory.join("entry.tsx"),
            "function h(tag, props, child) { return child; } export default <div>dependency</div>;",
        )
        .unwrap();
    }
    let mut launch = config(&root);
    launch.grants.read = Scope::Only(BTreeSet::from([workspace.path().canonicalize().unwrap()]));
    for import in ["./node_modules/view/entry.tsx", "../node_modules/view/entry.tsx"] {
        let source = format!("import view from '{import}'; void view;");
        let RunOutcome::Failed(error) = run(launch.clone(), "entry.ts", &source) else {
            panic!("dependency JSX inherited project settings: {import}");
        };
        assert_eq!(error.kind, ErrorKind::Transform, "{error:?}");
        assert!(error.message.contains("compilerOptions \"jsx\" is not set"), "{error:?}");
    }
}
