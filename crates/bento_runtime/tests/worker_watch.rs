mod support;

use bento_host::Scope;
use bento_runtime::start;
use support::{completed, config, project, run};

fn worker_entry(node: bool, target: &str, expected: &str) -> String {
    let launch = if node {
        format!(
            "const worker = new Worker(new URL('{target}', import.meta.url));\n\
             worker.once('error', () => resolve('WORKER FAILED'));\n\
             worker.once('message', message => {{ worker.terminate(); resolve(message); }});"
        )
    } else {
        format!(
            "const worker = new Worker(new URL('{target}', import.meta.url), {{ type: 'module' }});\n\
             worker.onerror = event => {{ event.preventDefault(); worker.terminate(); resolve('WORKER FAILED'); }};\n\
             worker.onmessage = event => {{ worker.terminate(); resolve(event.data); }};"
        )
    };
    let import = if node { "import { Worker } from 'node:worker_threads';" } else { "" };
    format!(
        "import assert from 'node:assert/strict';\n{import}\n\
         const answer = await new Promise(resolve => {{ {launch} }});\n\
         assert.equal(answer, '{expected}');"
    )
}

fn missing_worker_recovers(node: bool, target: &str) {
    let root = project();
    let app = root.path().join("app");
    std::fs::create_dir(&app).unwrap();
    std::fs::write(app.join("package.json"), r#"{"type":"module"}"#).unwrap();
    let worker = app.join(target);
    std::fs::create_dir_all(worker.parent().unwrap()).unwrap();
    let entry = app.join("main.mjs");
    std::fs::write(&entry, worker_entry(node, target, "WORKER FAILED")).unwrap();
    let mut settings = config(&app);
    settings.grants.read = Scope::All;
    let runtime = start(settings.clone(), &entry).unwrap();
    let handle = runtime.handle();
    completed(runtime.wait());
    let worker = worker.parent().unwrap().canonicalize().unwrap().join("worker.mjs");
    assert!(handle.watch_inputs().contains(&worker), "{:?}", handle.watch_inputs());

    let source = if node {
        "import { parentPort } from 'node:worker_threads'; parentPort.postMessage('WORKER RECOVERED');"
    } else {
        "postMessage('WORKER RECOVERED');"
    };
    std::fs::write(&worker, source).unwrap();
    completed(run(settings, "main.mjs", &worker_entry(node, target, "WORKER RECOVERED")));
}

#[test]
fn missing_external_web_worker_is_retained_and_recovers() {
    missing_worker_recovers(false, "../shared/worker.mjs");
}

#[test]
fn missing_external_node_worker_is_retained_and_recovers() {
    missing_worker_recovers(true, "../shared/worker.mjs");
}

#[test]
fn missing_web_worker_in_dist_is_retained_and_recovers() {
    missing_worker_recovers(false, "./dist/worker.mjs");
}

#[test]
fn missing_node_worker_in_dist_is_retained_and_recovers() {
    missing_worker_recovers(true, "./dist/worker.mjs");
}

#[test]
fn missing_worker_in_node_modules_is_excluded() {
    for node in [false, true] {
        let root = project();
        let app = root.path().join("app");
        std::fs::create_dir_all(app.join("node_modules/workers")).unwrap();
        std::fs::write(app.join("package.json"), r#"{"type":"module"}"#).unwrap();
        let entry = app.join("main.mjs");
        std::fs::write(
            &entry,
            worker_entry(node, "./node_modules/workers/worker.mjs", "WORKER FAILED"),
        )
        .unwrap();
        let runtime = start(config(&app), &entry).unwrap();
        let handle = runtime.handle();
        completed(runtime.wait());
        let target = app.canonicalize().unwrap().join("node_modules/workers/worker.mjs");
        assert!(!handle.watch_inputs().contains(&target), "{:?}", handle.watch_inputs());
    }
}
