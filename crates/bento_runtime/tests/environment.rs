mod support;

use std::collections::BTreeSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use tempfile::tempdir;

use bento_host::Scope;
use bento_runtime::{HostModule, start};
use support::{completed, config, run};

#[test]
fn environment_reads_enumeration_and_writes_stay_inside_each_instance() {
    let root = tempdir().unwrap();
    let key = "BENTO_LIFECYCLE_ENV_TEST";
    let host_before = std::env::var_os(key);
    for initial in ["first", "second"] {
        let mut settings = config(root.path());
        settings.env.insert(key.into(), initial.into());
        settings.env.insert("BENTO_UNGRANTED_ENV_TEST".into(), "hidden".into());
        settings.grants.env = Scope::Only(BTreeSet::from([key.into()]));
        completed(run(
            settings,
            "environment.mjs",
            &format!(
                r#"
import assert from 'node:assert/strict';
assert.equal(process.env.{key}, '{initial}');
assert.deepEqual(Object.keys(process.env), ['{key}']);
assert.deepEqual({{ ...Deno.env.toObject() }}, {{ {key}: '{initial}' }});
assert.equal(Deno.env.get('BENTO_UNGRANTED_ENV_TEST'), undefined);
process.env.{key} = 'changed';
assert.equal(Deno.env.get('{key}'), 'changed');
Deno.env.set('{key}', 'again');
assert.equal(process.env.{key}, 'again');
assert.throws(() => {{ process.env.BENTO_UNGRANTED_ENV_TEST = 'denied'; }}, /NotCapable|Requires/i);
assert.throws(() => Deno.env.set('BENTO_UNGRANTED_ENV_TEST', 'denied'), /NotCapable|Requires/i);
delete process.env.{key};
assert.equal(Deno.env.get('{key}'), undefined);
assert.deepEqual(Object.keys(process.env), []);
"#
            ),
        ));
        assert_eq!(std::env::var_os(key), host_before);
    }
}

#[test]
fn concurrent_instances_and_the_host_never_see_another_instances_writes() {
    let root = tempdir().unwrap();
    let key = "BENTO_CONCURRENT_ENV_TEST";
    let host_before = std::env::var_os(key);
    let changed = Arc::new(AtomicBool::new(false));
    let observed = changed.clone();
    let mut writer = config(root.path());
    writer.env.insert(key.into(), "writer".into());
    writer.grants.env = Scope::Only(BTreeSet::from([key.into()]));
    writer.host_modules = vec![HostModule::new("host:test").function("changed", move |_| {
        observed.store(true, Ordering::Release);
        async { Ok(serde_json::Value::Null) }
    })];
    let entry = root.path().join("writer.mjs");
    std::fs::write(&entry, format!("import {{ changed }} from 'host:test'; Deno.env.set('{key}', 'changed'); await changed(); setInterval(() => {{}}, 1000);")).unwrap();
    let runtime = start(writer, &entry).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !changed.load(Ordering::Acquire) {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(std::env::var_os(key), host_before);
    let mut reader = config(root.path());
    reader.env.insert(key.into(), "reader".into());
    reader.grants.env = Scope::Only(BTreeSet::from([key.into()]));
    completed(run(
        reader,
        "reader.mjs",
        &format!(
            "import assert from 'node:assert/strict'; assert.equal(Deno.env.get('{key}'), 'reader');"
        ),
    ));
    runtime.handle().shutdown();
    runtime.wait();
    assert_eq!(std::env::var_os(key), host_before);
}
