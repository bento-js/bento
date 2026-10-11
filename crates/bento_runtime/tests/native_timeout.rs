mod support;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use serde_json::Value;
use tempfile::tempdir;

use bento_runtime::{HostModule, RunOutcome, ShutdownOutcome, start};
use support::{completed, config, run};

#[test]
fn blocking_host_code_reports_timeout_without_claiming_disposal() {
    let root = tempdir().unwrap();
    let entered = Arc::new(AtomicBool::new(false));
    let observed = entered.clone();
    let mut settings = config(root.path());
    settings.shutdown_deadline = Duration::from_millis(50);
    settings.host_modules = vec![HostModule::new("host:blocking").function("block", move |_| {
        let observed = observed.clone();
        async move {
            observed.store(true, Ordering::Release);
            std::thread::sleep(Duration::from_millis(600));
            Ok(Value::Null)
        }
    })];
    let entry = root.path().join("blocking.mjs");
    std::fs::write(&entry, "import { block } from 'host:blocking'; await block();").unwrap();
    let runtime = start(settings, &entry).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !entered.load(Ordering::Acquire) {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }
    let start = Instant::now();
    let report = runtime.handle().shutdown();
    assert_eq!(report.outcome, ShutdownOutcome::TimedOut);
    assert!(start.elapsed() < Duration::from_millis(500));
    assert!(
        matches!(runtime.wait(), RunOutcome::Shutdown(report) if report.outcome == ShutdownOutcome::TimedOut)
    );
    completed(run(config(root.path()), "replacement.mjs", "await Promise.resolve();"));
}
