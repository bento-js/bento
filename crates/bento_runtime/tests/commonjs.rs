mod support;

use tempfile::tempdir;

use support::{completed, config, run};

#[test]
fn commonjs_named_exports_and_transpiler_reexports() {
    let root = tempdir().unwrap();
    for (name, source) in [
        (
            "direct.cjs",
            "exports.direct = 1; exports['computed'] = 2; Object.defineProperty(exports, 'defined', { enumerable: true, get: () => 3 });",
        ),
        ("object.cjs", "module.exports = { object: 4, shorthand: 5 };"),
        (
            "babel.cjs",
            "Object.defineProperty(exports, '__esModule', { value: true }); Object.assign(exports, require('./direct.cjs'));",
        ),
        (
            "typescript.cjs",
            "function __exportStar(m, e) { for (const key of Object.keys(m)) e[key] = m[key]; } __exportStar(require('./direct.cjs'), exports);",
        ),
        (
            "esbuild.cjs",
            "const api = {}; function __export(target, all) { for (const name in all) Object.defineProperty(target, name, { get: all[name], enumerable: true }); } function __toCommonJS(target) { return target; } __export(api, { bundled: () => 6 }); module.exports = __toCommonJS(api);",
        ),
        ("inner.cjs", "function member() {} member.attached = 7; exports.member = member;"),
        ("member.cjs", "module.exports = require('./inner.cjs').member;"),
    ] {
        std::fs::write(root.path().join(name), source).unwrap();
    }
    completed(run(
        config(root.path()),
        "interop.mjs",
        r#"
import assert from 'node:assert/strict';
import { direct, computed, defined } from './direct.cjs';
import { object, shorthand } from './object.cjs';
import { direct as babel } from './babel.cjs';
import { direct as typescript } from './typescript.cjs';
import { bundled } from './esbuild.cjs';
import { attached } from './member.cjs';
assert.deepEqual([direct, computed, defined, object, shorthand, babel, typescript, bundled, attached], [1,2,3,4,5,1,1,6,7]);
"#,
    ));
}

#[test]
fn package_type_esm_and_fully_specified_relative_imports() {
    let root = tempdir().unwrap();
    std::fs::write(
        root.path().join("package.json"),
        r##"{"type":"module","imports":{"#local":"./module.js"}}"##,
    )
    .unwrap();
    std::fs::write(root.path().join("module.js"), "export const number = 42;").unwrap();
    completed(run(
        config(root.path()),
        "module.mjs",
        "import assert from 'node:assert/strict'; import { number } from '#local'; assert.equal(number, 42);",
    ));
    let outcome = run(config(root.path()), "extension.mjs", "import './module';");
    assert!(
        matches!(outcome, bento_runtime::RunOutcome::Failed(error) if error.kind == bento_runtime::ErrorKind::Resolution)
    );
}
