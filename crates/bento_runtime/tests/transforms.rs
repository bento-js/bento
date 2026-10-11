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
