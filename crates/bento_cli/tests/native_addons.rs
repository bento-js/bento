mod support;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::LazyLock;

use support::{bento_in, path_without_other_runtimes, stderr, stdout};

const FIXTURE_COPY: &str = "/tmp/bento-cli/native-fixture";

/// Loads Lightning CSS, a Node-API addon the guest book's asset pipeline depends on, and
/// minifies a stylesheet with it.
const MINIFY_WITH_LIGHTNING_CSS: &str = r#"
import { createRequire } from 'node:module';
const require = createRequire(import.meta.resolve('remix/assets'));
const { transform } = require('lightningcss');
const { code } = transform({ filename: 'probe.css', code: Buffer.from('.probe {\n  color: red;\n}\n'), minify: true });
console.log(code.toString());
"#;

/// A copy of the guest-book fixture with its locked dependencies installed. Reusing one
/// location keeps repeated runs from accumulating installations.
fn installed_fixture() -> &'static Path {
    static FIXTURE: LazyLock<PathBuf> = LazyLock::new(|| {
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/remix-guest-book/");
        fs::create_dir_all(FIXTURE_COPY).unwrap();
        let copied = Command::new("rsync")
            .args(["-a", "--delete", "--exclude", "node_modules", "--exclude", "db/*.db"])
            .arg(source)
            .arg(FIXTURE_COPY)
            .status()
            .unwrap();
        assert!(copied.success());
        let installed = Command::new("pnpm")
            .args(["install", "--frozen-lockfile"])
            .current_dir(FIXTURE_COPY)
            .status()
            .unwrap();
        assert!(installed.success());
        Path::new(FIXTURE_COPY).canonicalize().unwrap()
    });
    &FIXTURE
}

#[test]
fn a_script_loads_a_native_addon_from_the_fixture_with_only_bento_on_path() {
    let root = installed_fixture();
    fs::write(root.join("minify.mjs"), MINIFY_WITH_LIGHTNING_CSS).unwrap();

    let output =
        bento_in(root, &["run", "--allow-read=.", "--allow-addons=./node_modules", "minify.mjs"])
            .env("PATH", path_without_other_runtimes())
            .output()
            .unwrap();

    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(stdout(&output), ".probe{color:red}\n");
}

/// Lightning CSS's own entry falls back to another file when its binding package fails to
/// load, which hides the reason, so this loads the binding package directly.
const LOAD_LIGHTNING_CSS_BINDING: &str = r#"
import { createRequire } from 'node:module';
const require = createRequire(import.meta.resolve('remix/assets'));
const binding = require(`lightningcss-${process.platform}-${process.arch}`);
console.log(typeof binding.transform);
"#;

#[test]
fn without_an_addon_grant_loading_the_addon_is_a_permission_denial() {
    let root = installed_fixture();
    fs::write(root.join("binding.mjs"), LOAD_LIGHTNING_CSS_BINDING).unwrap();

    let denied = bento_in(root, &["run", "--allow-read=.", "binding.mjs"]).output().unwrap();
    let granted =
        bento_in(root, &["run", "--allow-read=.", "--allow-addons=./node_modules", "binding.mjs"])
            .output()
            .unwrap();

    assert_eq!(denied.status.code(), Some(1));
    let message = stderr(&denied);
    assert!(message.contains("permission denied"), "{message}");
    assert!(stdout(&denied).is_empty(), "{}", stdout(&denied));
    assert!(granted.status.success(), "{}", stderr(&granted));
    assert_eq!(stdout(&granted), "function\n");
}
