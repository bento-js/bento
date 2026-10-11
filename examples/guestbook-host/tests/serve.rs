//! `serve` as the host owns it: Ctrl-C and `SIGTERM` to the host process end the running
//! instance within the shutdown deadline with a reported outcome and a free port, whether the
//! guest is serving or still starting.

mod support;

use std::os::raw::c_int;
use std::panic::{self, AssertUnwindSafe};
use std::path::Path;
use std::sync::{Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use signal_hook::consts::{SIGINT, SIGTERM};
use signal_hook::low_level::raise;

use bento_guestbook_host::launch::{Launch, Mode};
use bento_guestbook_host::lifecycle::{Served, serve};
use bento_guestbook_host::store::Access;
use bento_runtime::{RunOutcome, ShutdownOutcome};
use support::{
    assert_port_free, free_port, guest_project, guests, http, scratch_dir, wait_for_listener,
};

const DEADLINE: Duration = Duration::from_secs(5);

/// The host's signal handlers are process-wide, so one served instance at a time.
static ONE_INSTANCE: Mutex<()> = Mutex::new(());

/// Serves `entry` on another thread, raises `signal` in this process once `started` has observed
/// the guest, and returns how the instance ended with the time from the signal to the report.
/// The signal is raised even when the observation fails, so a failing test ends instead of
/// serving forever.
fn serve_until(
    launch: &Launch,
    entry: &Path,
    signal: c_int,
    started: impl FnOnce(),
) -> (Served, Duration) {
    let _one_instance = ONE_INSTANCE.lock().unwrap_or_else(PoisonError::into_inner);
    thread::scope(|scope| {
        let served = scope.spawn(|| serve(launch, Access::ReadWrite, entry).unwrap());
        let observed = panic::catch_unwind(AssertUnwindSafe(started));
        let signaled = Instant::now();
        raise(signal).unwrap();
        let served = served.join().unwrap();
        if let Err(failure) = observed {
            panic::resume_unwind(failure);
        }
        (served, signaled.elapsed())
    })
}

fn assert_graceful(served: &Served, signal: c_int, exit_code: Option<i32>) {
    let graceful = ShutdownOutcome::Graceful { exit_code };
    assert!(
        matches!(&served.outcome, RunOutcome::Shutdown(report) if report.outcome == graceful),
        "{served:?}"
    );
    assert_eq!(served.signal, Some(signal));
}

#[test]
fn sigterm_and_ctrl_c_each_end_a_serving_guest_gracefully_and_free_its_port() {
    let scratch = scratch_dir();
    let project = guest_project(scratch.path(), "server.mjs", guests::SERVER);
    let database = scratch.path().join("book.db");
    let port = free_port();
    let launch = Launch::prepare(&project, &database, port, Mode::Development).unwrap();
    let entry = project.join("server.mjs");

    for (signal, message) in [(SIGTERM, "Terminated"), (SIGINT, "Interrupted")] {
        let (served, took) = serve_until(&launch, &entry, signal, || {
            wait_for_listener(port);
            let signed = http::post_form(port, "/", &[("name", "Ada"), ("message", message)]);
            assert_eq!(signed.status, 303);
        });
        assert_graceful(&served, signal, Some(0));
        assert!(took < DEADLINE, "{took:?}");
        assert_port_free(port);
    }

    let (served, _) = serve_until(&launch, &entry, SIGTERM, || {
        wait_for_listener(port);
        let listed: serde_json::Value = serde_json::from_str(&http::get(port, "/").body).unwrap();
        let messages: Vec<_> =
            listed.as_array().unwrap().iter().map(|entry| entry["message"].clone()).collect();
        assert_eq!(messages, ["Terminated", "Interrupted"]);
    });
    assert_graceful(&served, SIGTERM, Some(0));
}

/// A guest that has not installed a signal handler yet is ended by the runtime's default
/// shutdown, so there is no guest exit code to report.
#[test]
fn a_signal_while_the_guest_is_still_starting_ends_the_instance_within_the_deadline() {
    let scratch = scratch_dir();
    let project = guest_project(scratch.path(), "starting.mjs", guests::STARTING);
    let database = scratch.path().join("book.db");
    let port = free_port();
    let launch = Launch::prepare(&project, &database, port, Mode::Development).unwrap();

    let (served, took) =
        serve_until(&launch, &project.join("starting.mjs"), SIGTERM, || wait_for_listener(port));
    assert_graceful(&served, SIGTERM, None);
    assert!(took < DEADLINE, "{took:?}");
    assert_port_free(port);
}
