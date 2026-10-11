mod support;

use std::fs;

use bento_runtime::{ErrorKind, RunOutcome};
use support::{completed, config, run};

#[test]
fn transforms_all_typescript_extensions_and_require_without_emitting_files() {
    let root = support::project();
    fs::write(root.path().join("package.json"), r#"{"type":"module"}"#).unwrap();
    fs::write(root.path().join("value.mts"), "export enum Number { Answer = 42 }").unwrap();
    fs::write(root.path().join("legacy.cts"), "module.exports = { answer: 42 as number };")
        .unwrap();
    completed(run(
        config(root.path()),
        "entry.ts",
        r#"
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { Number } from './value.mts';
const require = createRequire(import.meta.url);
const answer: number = await Promise.resolve(Number.Answer);
assert.equal(answer, 42);
assert.equal(require('./legacy.cts').answer, 42);
"#,
    ));
    assert!(!root.path().join("entry.js").exists());
    assert!(!root.path().join("value.mjs").exists());
    assert!(!root.path().join("legacy.cjs").exists());
}

#[test]
fn project_settings_apply_to_tsx_and_jsx_but_not_node_modules() {
    let root = support::project();
    fs::write(root.path().join("package.json"), r#"{"type":"module"}"#).unwrap();
    fs::write(root.path().join("tsconfig.json"), r#"{"compilerOptions":{"jsx":"react","jsxFactory":"element","noEmit":true,"useDefineForClassFields":false}}"#).unwrap();
    for name in ["view.tsx", "view.jsx"] {
        fs::write(root.path().join(name), "function element(tag, props, child) { return child; } export default <div>rendered</div>;").unwrap();
    }
    let dependency = root.path().join("node_modules/fields");
    fs::create_dir_all(&dependency).unwrap();
    fs::write(dependency.join("package.json"), r#"{"type":"module","exports":"./entry.ts"}"#)
        .unwrap();
    fs::write(dependency.join("entry.ts"), "class Base { set field(value: string) { throw new Error('dependency inherited project settings'); } } class Derived extends Base { field = 'own'; } export default new Derived().field;").unwrap();
    completed(run(
        config(root.path()),
        "entry.ts",
        r#"
import assert from 'node:assert/strict';
import tsx from './view.tsx';
import jsx from './view.jsx';
import field from 'fields';
assert.equal(tsx, 'rendered'); assert.equal(jsx, 'rendered'); assert.equal(field, 'own');
"#,
    ));
}

#[test]
fn unexecutable_jsx_settings_fail_only_the_jsx_module_with_the_setting_named() {
    let root = support::project();
    fs::write(root.path().join("tsconfig.json"), r#"{"compilerOptions":{"jsx":"preserve"}}"#)
        .unwrap();
    completed(run(config(root.path()), "plain.ts", "let answer: number = 42; void answer;"));

    fs::write(root.path().join("view.tsx"), "export let view = <p>hello</p>;\n").unwrap();
    let RunOutcome::Failed(error) =
        run(config(root.path()), "entry.ts", "import { view } from './view.tsx';\nvoid view;")
    else {
        panic!("JSX under jsx: preserve executed");
    };
    assert_eq!(error.kind, ErrorKind::Transform);
    assert!(error.message.contains("jsx") && error.message.contains("preserve"), "{error:?}");
    let location = error.location().expect("location of the JSX element");
    assert!(location.path.ends_with("view.tsx"), "{location:?}");
    assert_eq!(location.line, 1);
}

#[test]
fn a_project_beneath_node_modules_keeps_its_settings_locations_and_watch_sources() {
    let directory = support::project();
    let root = directory.path().join("node_modules/project");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("package.json"), r#"{"type":"module"}"#).unwrap();
    fs::write(
        root.join("tsconfig.json"),
        r#"{"compilerOptions":{"jsx":"react","jsxFactory":"element"}}"#,
    )
    .unwrap();
    completed(run(
        config(&root),
        "view.tsx",
        "function element(tag, props, child) { return child; }\nconst view = <div>rendered</div>;\nif (view !== 'rendered') throw new Error('wrong JSX factory');\n",
    ));
    let entry = root.join("throw.ts");
    fs::write(&entry, "enum Answer { Value = 42 }\nthrow new Error('project failure');\n").unwrap();
    let runtime = bento_runtime::start(config(&root), &entry).unwrap();
    let handle = runtime.handle();
    let RunOutcome::Failed(error) = runtime.wait() else {
        panic!("project exception succeeded");
    };
    assert_eq!(error.kind, ErrorKind::Uncaught, "{error:?}");
    let location = error.location().expect("project source location");
    assert_eq!(location.path, entry.canonicalize().unwrap(), "{error:?}");
    assert_eq!((location.line, location.column), (2, 7));
    assert!(handle.loaded_modules().contains(&location.path));
}

#[test]
fn watch_sources_include_workspace_siblings_loaded_by_import_and_require() {
    let workspace = support::project();
    let root = workspace.path().join("app");
    let sibling = workspace.path().join("sibling");
    let dependency = workspace.path().join("node_modules/installed");
    fs::create_dir_all(root.join("node_modules")).unwrap();
    fs::create_dir_all(&sibling).unwrap();
    fs::create_dir_all(&dependency).unwrap();
    fs::write(root.join("package.json"), r#"{"type":"module"}"#).unwrap();
    fs::write(sibling.join("package.json"), r#"{"type":"module","exports":"./entry.mjs"}"#)
        .unwrap();
    fs::write(sibling.join("entry.mjs"), "export const answer = 42;").unwrap();
    fs::write(sibling.join("legacy.cjs"), "module.exports = 42;").unwrap();
    fs::write(dependency.join("entry.mjs"), "export default 42;").unwrap();
    std::os::unix::fs::symlink(&sibling, root.join("node_modules/sibling")).unwrap();
    let entry = root.join("entry.mjs");
    fs::write(
        &entry,
        r#"
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import installed from '../node_modules/installed/entry.mjs';
const require = createRequire(import.meta.url);
const { answer } = await import('sibling');
assert.equal(answer, installed);
assert.equal(require('../sibling/legacy.cjs'), answer);
"#,
    )
    .unwrap();
    let mut launch = config(&root);
    launch.grants.read = bento_host::Scope::Only(std::collections::BTreeSet::from([workspace
        .path()
        .canonicalize()
        .unwrap()]));
    let runtime = bento_runtime::start(launch, &entry).unwrap();
    let handle = runtime.handle();
    completed(runtime.wait());
    let loaded = handle.loaded_modules();
    for path in [entry, sibling.join("entry.mjs"), sibling.join("legacy.cjs")] {
        assert!(loaded.contains(&path.canonicalize().unwrap()), "{path:?} missing: {loaded:?}");
    }
    assert!(!loaded.contains(&dependency.join("entry.mjs").canonicalize().unwrap()), "{loaded:?}");
}
