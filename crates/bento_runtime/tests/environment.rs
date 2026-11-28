mod support;

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use tempfile::tempdir;

use bento_host::{EnvPattern, PermissionSet, Permissions, Scope};
use bento_runtime::{HostModule, RuntimeConfig, start};
use support::{completed, config_with, project_access, run};

const UNCOVERED: &str = "BENTO_UNCOVERED_ENV_TEST";

/// A restricted instance for `root` whose permissions cover only the variable `key`.
fn covering(root: &Path, key: &str) -> RuntimeConfig {
    config_with(
        root,
        Permissions::restricted(PermissionSet {
            env: Scope::only([EnvPattern::parse(key).unwrap()]),
            ..project_access(root)
        }),
    )
}

#[test]
fn uncovered_variables_are_removed_at_startup_and_writes_stay_inside_each_instance() {
    let root = tempdir().unwrap();
    let key = "BENTO_LIFECYCLE_ENV_TEST";
    let host_before = std::env::var_os(key);
    let uncovered_before = std::env::var_os(UNCOVERED);
    for initial in ["first", "second"] {
        let mut settings = covering(root.path(), key);
        settings.env.insert(key.into(), initial.into());
        settings.env.insert(UNCOVERED.into(), "removed".into());
        completed(run(
            settings,
            "environment.mjs",
            &format!(
                r#"
import assert from 'node:assert/strict';
assert.equal(process.env.{key}, '{initial}');
assert.deepEqual(Object.keys(process.env), ['{key}']);
assert.deepEqual({{ ...Deno.env.toObject() }}, {{ {key}: '{initial}' }});
assert.equal(Deno.env.get('{UNCOVERED}'), undefined);
assert.equal(process.env.{UNCOVERED}, undefined);
process.env.{key} = 'changed';
assert.equal(Deno.env.get('{key}'), 'changed');
Deno.env.set('{key}', 'again');
assert.equal(process.env.{key}, 'again');
process.env.{UNCOVERED} = 'set by the guest';
assert.equal(Deno.env.get('{UNCOVERED}'), 'set by the guest');
Deno.env.set('{UNCOVERED}', 'set again');
assert.equal(process.env.{UNCOVERED}, 'set again');
assert.deepEqual(Object.keys(process.env).sort(), ['{key}', '{UNCOVERED}']);
delete process.env.{key};
assert.equal(Deno.env.get('{key}'), undefined);
Deno.env.delete('{UNCOVERED}');
assert.equal(process.env.{UNCOVERED}, undefined);
assert.deepEqual(Object.keys(process.env), []);
"#
            ),
        ));
        assert_eq!(std::env::var_os(key), host_before);
        assert_eq!(std::env::var_os(UNCOVERED), uncovered_before);
    }
}

#[test]
fn an_unrestricted_instance_sees_and_changes_the_whole_environment_inside_itself() {
    let root = tempdir().unwrap();
    let uncovered_before = std::env::var_os(UNCOVERED);
    let mut settings = config_with(root.path(), Permissions::unrestricted());
    settings.env.insert(UNCOVERED.into(), "visible".into());
    settings.env.insert("DATABASE_URL".into(), "sqlite:guest.db".into());
    completed(run(
        settings,
        "environment.mjs",
        &format!(
            r#"
import assert from 'node:assert/strict';
assert.equal(process.permission, undefined);
assert.deepEqual({{ ...Deno.env.toObject() }}, {{ DATABASE_URL: 'sqlite:guest.db', {UNCOVERED}: 'visible' }});
process.env.{UNCOVERED} = 'changed';
assert.equal(Deno.env.get('{UNCOVERED}'), 'changed');
delete process.env.DATABASE_URL;
assert.deepEqual(Object.keys(process.env), ['{UNCOVERED}']);
"#
        ),
    ));
    assert_eq!(std::env::var_os(UNCOVERED), uncovered_before);
}

#[test]
fn concurrent_instances_and_the_host_never_see_another_instances_writes() {
    let root = tempdir().unwrap();
    let key = "BENTO_CONCURRENT_ENV_TEST";
    let host_before = std::env::var_os(key);
    let uncovered_before = std::env::var_os(UNCOVERED);
    let changed = Arc::new(AtomicBool::new(false));
    let observed = changed.clone();
    let mut writer = covering(root.path(), key);
    writer.env.insert(key.into(), "writer".into());
    writer.host_modules = vec![HostModule::new("host:test").function("changed", move |_| {
        observed.store(true, Ordering::Release);
        async { Ok(serde_json::Value::Null) }
    })];
    let entry = root.path().join("writer.mjs");
    std::fs::write(&entry, format!("import {{ changed }} from 'host:test'; Deno.env.set('{key}', 'changed'); Deno.env.set('{UNCOVERED}', 'written'); await changed(); setInterval(() => {{}}, 1000);")).unwrap();
    let runtime = start(writer, &entry).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !changed.load(Ordering::Acquire) {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(std::env::var_os(key), host_before);
    assert_eq!(std::env::var_os(UNCOVERED), uncovered_before);
    let mut reader = covering(root.path(), key);
    reader.env.insert(key.into(), "reader".into());
    completed(run(
        reader,
        "reader.mjs",
        &format!(
            "import assert from 'node:assert/strict'; assert.equal(Deno.env.get('{key}'), 'reader'); assert.equal(Deno.env.get('{UNCOVERED}'), undefined);"
        ),
    ));
    runtime.handle().shutdown();
    runtime.wait();
    assert_eq!(std::env::var_os(key), host_before);
    assert_eq!(std::env::var_os(UNCOVERED), uncovered_before);
}
