mod support;

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
