use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use notify::{Event, EventKind};
use parking_lot::Mutex;

use super::scope::WatchScope;

/// How long the watched files must stay unchanged before a burst of changes is reported.
const QUIET_PERIOD: Duration = Duration::from_millis(100);

pub type Notification = notify::Result<Event>;

/// Reports each burst of changes in `scope` among `notifications` to `on_change`, once,
/// after the files have been quiet for `QUIET_PERIOD`. Returns when the notifications end.
pub fn report_bursts(
    notifications: &Receiver<Notification>,
    scope: &Mutex<WatchScope>,
    mut on_change: impl FnMut(Vec<PathBuf>),
) {
    while let Some(mut burst) = first_change(notifications, scope) {
        let mut quiet_from = Instant::now() + QUIET_PERIOD;
        loop {
            let remaining = quiet_from.saturating_duration_since(Instant::now());
            match notifications.recv_timeout(remaining) {
                Ok(notification) => {
                    let changed = changed_paths(notification, scope);
                    if !changed.is_empty() {
                        burst.extend(changed);
                        quiet_from = Instant::now() + QUIET_PERIOD;
                    }
                }
                Err(RecvTimeoutError::Timeout) => break,
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }
        on_change(burst.into_iter().collect());
    }
}

fn first_change(
    notifications: &Receiver<Notification>,
    scope: &Mutex<WatchScope>,
) -> Option<BTreeSet<PathBuf>> {
    loop {
        let changed = changed_paths(notifications.recv().ok()?, scope);
        if !changed.is_empty() {
            return Some(changed);
        }
    }
}

fn changed_paths(notification: Notification, scope: &Mutex<WatchScope>) -> BTreeSet<PathBuf> {
    match notification {
        Ok(event) => paths_in_scope(event, &scope.lock()),
        Err(error) => {
            eprintln!("warning: watching for changes: {error}");
            BTreeSet::new()
        }
    }
}

fn paths_in_scope(event: Event, scope: &WatchScope) -> BTreeSet<PathBuf> {
    if matches!(event.kind, EventKind::Access(_)) {
        return BTreeSet::new();
    }
    event.paths.into_iter().filter(|path| scope.includes(path)).collect()
}
