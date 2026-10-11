mod support;

use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use serde_json::{Value, json};
use tempfile::tempdir;

use bento_runtime::{ErrorKind, HostError, HostModule, RunOutcome};
use support::{completed, config, run};

#[test]
fn async_host_services_round_trip_json_errors_and_keep_timers_running() {
    let root = tempdir().unwrap();
    let calls = Arc::new(Mutex::new(Vec::new()));
    let recorded = calls.clone();
    let mut settings = config(root.path());
    settings.host_modules = vec![
        HostModule::new("host:demo")
            .function("echo", move |arguments| {
                let recorded = recorded.clone();
                async move {
                    recorded.lock().push(arguments.clone());
                    Ok(arguments)
                }
            })
            .function("slow", |_| async {
                tokio::time::sleep(Duration::from_millis(100)).await;
                Ok(json!("finished"))
            })
            .function("refuse", |_| async {
                Err(HostError { name: "ReadOnlyStore".into(), message: "writes denied".into() })
            }),
    ];
    completed(run(
        settings,
        "host.mjs",
        r#"
import assert from 'node:assert/strict';
import { echo, slow, refuse } from 'host:demo';
const input = [{ nested: [null, true, 7, 'text'] }, 42];
assert.deepEqual(await echo(...input), input);
let timerFired = false;
setTimeout(() => { timerFired = true; }, 10);
assert.equal(await slow(), 'finished');
assert.equal(timerFired, true);
await assert.rejects(refuse(), error => error instanceof Error && error.name === 'ReadOnlyStore' && error.message === 'writes denied');
"#,
    ));
    assert_eq!(*calls.lock(), vec![json!([{ "nested": [null, true, 7, "text"] }, 42])]);
}

#[test]
fn host_registrations_and_handlers_are_instance_local() {
    let root = tempdir().unwrap();
    let barrier = Arc::new(tokio::sync::Barrier::new(2));
    let mut instances = Vec::new();
    for expected in ["first", "second"] {
        let mut settings = config(root.path());
        let barrier = barrier.clone();
        settings.host_modules =
            vec![HostModule::new("host:demo").function("identity", move |_| {
                let barrier = barrier.clone();
                async move {
                    barrier.wait().await;
                    Ok(Value::String(expected.into()))
                }
            })];
        let entry = root.path().join(format!("{expected}.mjs"));
        std::fs::write(&entry, format!(
            "import assert from 'node:assert/strict'; import {{ identity }} from 'host:demo'; assert.equal(await identity(), '{expected}');"
        )).unwrap();
        instances.push(bento_runtime::start(settings, &entry).unwrap());
    }
    for instance in instances {
        completed(instance.wait());
    }
    let RunOutcome::Failed(error) =
        run(config(root.path()), "unregistered.mjs", "import 'host:demo';")
    else {
        panic!("unregistered host module imported successfully");
    };
    assert_eq!(error.kind, ErrorKind::Resolution);
    assert!(error.message.contains("host:demo"));
}
