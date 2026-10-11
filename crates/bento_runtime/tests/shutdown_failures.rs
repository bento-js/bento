mod support;

use std::path::Path;
use std::time::{Duration, Instant};

use tempfile::tempdir;

use bento_runtime::{ErrorKind, RunOutcome, ShutdownOutcome, start};
use support::{config, run};

fn ready(path: &Path) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !path.exists() {
        assert!(Instant::now() < deadline, "guest did not become ready");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn errors_and_rejections_during_shutdown_remain_observable() {
    for handler in [
        "() => { throw new Error('shutdown failed'); }",
        "async () => { await Promise.resolve(); throw new Error('shutdown failed'); }",
        "() => { setTimeout(() => { throw new Error('shutdown failed'); }, 5); }",
    ] {
        let root = tempdir().unwrap();
        let entry = root.path().join("entry.mjs");
        std::fs::write(
            &entry,
            format!(
                r#"
import {{ writeFileSync }} from 'node:fs';
process.on('SIGTERM', {handler});
writeFileSync('ready', 'yes');
setInterval(() => {{}}, 1000);
"#
            ),
        )
        .unwrap();
        let runtime = start(config(root.path()), &entry).unwrap();
        ready(&root.path().join("ready"));
        let report = runtime.handle().shutdown();
        let ShutdownOutcome::Failed(reported) = report.outcome else {
            panic!("guest failure reported incorrectly: {report:?}");
        };
        let RunOutcome::Failed(error) = runtime.wait() else {
            panic!("shutdown discarded the guest error");
        };
        assert_eq!(error.kind, ErrorKind::Uncaught);
        assert!(error.message.contains("shutdown failed"), "{error}");
        assert!(error.message.contains("entry.mjs:"), "{error}");
        assert_eq!(reported, error);
    }
}

#[test]
fn shutdown_after_a_guest_failure_does_not_report_graceful() {
    let root = tempdir().unwrap();
    let entry = root.path().join("entry.mjs");
    std::fs::write(&entry, "throw new Error('already failed');").unwrap();
    let runtime = start(config(root.path()), &entry).unwrap();
    let handle = runtime.handle();
    let RunOutcome::Failed(error) = runtime.wait() else {
        panic!("expected failure");
    };
    assert_eq!(handle.shutdown().outcome, ShutdownOutcome::Failed(error));
}

#[test]
fn failed_source_files_remain_in_the_watch_input() {
    for (name, source, statement) in [
        ("broken.cjs", "module.exports = {", "import './broken.cjs';"),
        ("broken.ts", "const answer: number = ;", "import './broken.ts';"),
        (
            "broken.cts",
            "const answer: number = ;",
            "import { createRequire } from 'node:module'; createRequire(import.meta.url)('./broken.cts');",
        ),
    ] {
        let root = tempdir().unwrap();
        std::fs::write(root.path().join(name), source).unwrap();
        let entry = root.path().join("entry.mjs");
        std::fs::write(&entry, statement).unwrap();
        let runtime = start(config(root.path()), &entry).unwrap();
        let handle = runtime.handle();
        assert!(matches!(runtime.wait(), RunOutcome::Failed(_)));
        assert!(
            handle.watch_inputs().contains(&root.path().canonicalize().unwrap().join(name)),
            "failed file omitted: {:?}",
            handle.watch_inputs()
        );
    }
}

#[test]
fn an_explicit_nonzero_exit_during_shutdown_is_not_a_failure() {
    let root = tempdir().unwrap();
    let mut settings = config(root.path());
    settings.shutdown_deadline = Duration::from_millis(100);
    let entry = root.path().join("exit.mjs");
    std::fs::write(&entry, "import { writeFileSync } from 'node:fs'; process.on('SIGTERM', () => process.exit(1)); writeFileSync('ready', 'yes'); setInterval(() => {}, 1000);").unwrap();
    let runtime = start(settings, &entry).unwrap();
    ready(&root.path().join("ready"));
    assert_eq!(
        runtime.handle().shutdown().outcome,
        ShutdownOutcome::Graceful { exit_code: Some(1) }
    );
    assert!(matches!(runtime.wait(), RunOutcome::Shutdown(_)));
    assert!(matches!(
        run(config(root.path()), "ordinary-exit.mjs", "process.exit(1);"),
        RunOutcome::Exited(1)
    ));
}
