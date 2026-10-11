mod support;

use std::path::Path;
use std::time::{Duration, Instant};

use tempfile::tempdir;

use bento_runtime::{RunOutcome, ShutdownOutcome, start};
use support::{completed, config, run};

fn wait_for(path: &Path) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !path.exists() {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn shutdown_delivers_sigterm_during_pending_top_level_await() {
    let root = tempdir().unwrap();
    let entry = root.path().join("startup.mjs");
    std::fs::write(
        &entry,
        r#"
import { writeFileSync } from 'node:fs';
process.on('SIGTERM', () => { writeFileSync('signalled', 'yes'); process.exit(0); });
setInterval(() => {}, 1000);
writeFileSync('ready', 'yes');
await new Promise(() => {});
"#,
    )
    .unwrap();
    let mut settings = config(root.path());
    settings.shutdown_deadline = Duration::from_millis(200);
    let runtime = start(settings, &entry).unwrap();
    wait_for(&root.path().join("ready"));
    assert_eq!(
        runtime.handle().shutdown().outcome,
        ShutdownOutcome::Graceful { exit_code: Some(0) }
    );
    assert!(root.path().join("signalled").exists());
    assert!(matches!(runtime.wait(), RunOutcome::Shutdown(_)));
}

#[test]
fn ignored_sigterm_with_pending_async_work_is_not_reported_graceful() {
    let root = tempdir().unwrap();
    let entry = root.path().join("pending.mjs");
    std::fs::write(&entry, "import { writeFileSync } from 'node:fs'; process.on('SIGTERM', () => {}); setInterval(() => {}, 1000); writeFileSync('ready', 'yes');").unwrap();
    let mut settings = config(root.path());
    settings.shutdown_deadline = Duration::from_millis(200);
    let runtime = start(settings, &entry).unwrap();
    wait_for(&root.path().join("ready"));
    let start = Instant::now();
    assert_eq!(runtime.handle().shutdown().outcome, ShutdownOutcome::Interrupted);
    assert!(start.elapsed() < Duration::from_secs(1));
    assert!(matches!(runtime.wait(), RunOutcome::Shutdown(_)));
    completed(run(config(root.path()), "replacement.mjs", "await Promise.resolve();"));
}
