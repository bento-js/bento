mod support;

use std::env;
use std::fs;
use std::io;
use std::net::{SocketAddr, TcpListener};
use std::process;
use std::thread;
use std::time::{Duration, Instant};

use serde_json::json;

use bento_host::Grants;
use bento_task::{Cancellation, Plan, Signal, TaskError};
use support::{SYSTEM_PATH, TempProject};

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

fn wait_until(deadline: Duration, mut condition: impl FnMut() -> bool) {
    let start = Instant::now();
    while !condition() {
        assert!(start.elapsed() < deadline, "condition not reached within {deadline:?}");
        thread::sleep(Duration::from_millis(20));
    }
}

fn port_is_taken(address: SocketAddr) -> bool {
    matches!(TcpListener::bind(address), Err(error) if error.kind() == io::ErrorKind::AddrInUse)
}

fn process_is_gone(pid: i32) -> bool {
    // SAFETY: signal 0 only checks whether the process exists.
    let result = unsafe { libc::kill(pid, 0) };
    result == -1 && io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH)
}

/// Asserts that `pid` has exited, killing it first if not so that a failure leaves nothing running.
fn assert_stopped(pid: i32) {
    if process_is_gone(pid) {
        return;
    }
    // SAFETY: `kill` has no memory-safety preconditions; `pid` is still alive, so not reused.
    unsafe { libc::kill(pid, libc::SIGKILL) };
    panic!("process {pid} outlived its command");
}

/// Starts `task` on another thread and returns once `ready` holds, with the
/// cancellation handle and the thread's eventual result.
fn start(
    temp: &TempProject,
    task: &str,
    grace_period: Duration,
    ready: impl FnMut() -> bool,
) -> (Cancellation, thread::JoinHandle<Result<(), TaskError>>) {
    let plan = Plan::new(temp.project().manifest(), task, &[]).unwrap();
    let runner = temp.runner(&[("PATH", SYSTEM_PATH)], &Grants::none()).grace_period(grace_period);
    let cancellation = Cancellation::new();
    let handle = {
        let cancellation = cancellation.clone();
        thread::spawn(move || runner.run(&plan, &cancellation))
    };
    wait_until(Duration::from_secs(10), ready);
    (cancellation, handle)
}

fn read_pid(temp: &TempProject, file: &str) -> i32 {
    temp.read(file).trim().parse().unwrap()
}

#[test]
fn cancellation_stops_a_grandchild_listener_and_frees_its_port() {
    let port = free_port();
    let address = SocketAddr::from(([127, 0, 0, 1], port));
    let temp = TempProject::with_tasks(json!({
        "serve": format!("nc -l 127.0.0.1 {port} & echo $! > listener.pid; wait"),
    }));

    let (cancellation, handle) = start(&temp, "serve", Duration::from_secs(5), || {
        temp.path("listener.pid").exists() && port_is_taken(address)
    });
    let listener = read_pid(&temp, "listener.pid");
    let started = Instant::now();
    cancellation.cancel(Signal::Terminate);
    let result = handle.join().unwrap();

    assert!(
        matches!(&result, Err(TaskError::Cancelled { task, signal: Signal::Terminate, .. }) if task == "serve"),
        "{result:?}"
    );
    assert!(started.elapsed() < Duration::from_secs(5), "the group ignored the signal");
    assert!(process_is_gone(listener));
    TcpListener::bind(address).expect("the port is free again");
}

/// A server that closes itself when interrupted ends the run with its own status.
#[test]
fn a_command_that_handles_the_signal_reports_the_status_it_ended_with() {
    let temp = TempProject::with_tasks(json!({
        "graceful": "trap 'exit 0' INT; touch ready; while :; do sleep 0.05; done",
    }));

    let (cancellation, handle) =
        start(&temp, "graceful", Duration::from_secs(5), || temp.path("ready").exists());
    cancellation.cancel(Signal::Interrupt);
    let result = handle.join().unwrap();

    let Err(TaskError::Cancelled { signal: Signal::Interrupt, status: Some(status), .. }) = result
    else {
        panic!("the command's status is missing: {result:?}");
    };
    assert!(status.success(), "{status:?}");
}

#[test]
fn processes_that_ignore_the_signal_are_killed_after_the_grace_period() {
    let port = free_port();
    let address = SocketAddr::from(([127, 0, 0, 1], port));
    let temp = TempProject::with_tasks(json!({
        "stubborn": format!("trap '' INT TERM; nc -l 127.0.0.1 {port} & echo $! > listener.pid; wait"),
    }));
    let grace_period = Duration::from_millis(500);

    let (cancellation, handle) = start(&temp, "stubborn", grace_period, || {
        temp.path("listener.pid").exists() && port_is_taken(address)
    });
    let listener = read_pid(&temp, "listener.pid");
    let started = Instant::now();
    cancellation.cancel(Signal::Interrupt);
    let result = handle.join().unwrap();
    let elapsed = started.elapsed();

    assert!(
        matches!(result, Err(TaskError::Cancelled { signal: Signal::Interrupt, status: None, .. })),
        "a killed command reported a status of its own: {result:?}"
    );
    assert!(elapsed >= grace_period, "killed before the grace period: {elapsed:?}");
    assert!(
        elapsed < grace_period + Duration::from_secs(2),
        "grace period not bounded: {elapsed:?}"
    );
    assert!(process_is_gone(listener));
    TcpListener::bind(address).expect("the port is free again");
}

#[test]
fn cancellation_before_a_command_starts_runs_nothing() {
    let temp = TempProject::with_tasks(json!({ "build": "touch built" }));
    let plan = Plan::new(temp.project().manifest(), "build", &[]).unwrap();
    let cancellation = Cancellation::new();
    cancellation.cancel(Signal::Interrupt);

    let result = temp.runner(&[("PATH", SYSTEM_PATH)], &Grants::none()).run(&plan, &cancellation);

    assert!(matches!(result, Err(TaskError::Cancelled { status: None, .. })), "{result:?}");
    assert!(!temp.path("built").exists());
}

#[test]
fn processes_a_finished_command_left_in_the_background_are_stopped() {
    let port = free_port();
    let address = SocketAddr::from(([127, 0, 0, 1], port));
    let temp = TempProject::with_tasks(json!({
        "serve": format!(
            "nc -l 127.0.0.1 {port} & echo $! > listener.pid; until [ -e finish ]; do sleep 0.05; done"
        ),
    }));

    let (_cancellation, handle) = start(&temp, "serve", Duration::from_secs(5), || {
        temp.path("listener.pid").exists() && port_is_taken(address)
    });
    let listener = read_pid(&temp, "listener.pid");
    temp.write("finish", "");
    let result = handle.join().unwrap();

    assert!(result.is_ok(), "{result:?}");
    assert_stopped(listener);
    TcpListener::bind(address).expect("the port is free again");
}

#[test]
fn leftover_processes_that_ignore_termination_are_killed_after_the_grace_period() {
    let temp = TempProject::with_tasks(json!({
        "leave": "(trap '' TERM; touch ready; exec sleep 30) & echo $! > stubborn.pid; \
                  until [ -e finish ]; do sleep 0.05; done; exit 3",
    }));
    let grace_period = Duration::from_millis(500);

    let (_cancellation, handle) = start(&temp, "leave", grace_period, || {
        temp.path("stubborn.pid").exists() && temp.path("ready").exists()
    });
    let stubborn = read_pid(&temp, "stubborn.pid");
    temp.write("finish", "");
    let finished = Instant::now();
    let result = handle.join().unwrap();
    let elapsed = finished.elapsed();

    assert_eq!(result.as_ref().err().and_then(TaskError::exit_code), Some(3), "{result:?}");
    assert_stopped(stubborn);
    assert!(elapsed >= grace_period, "killed before the grace period: {elapsed:?}");
    assert!(
        elapsed < grace_period + Duration::from_secs(2),
        "grace period not bounded: {elapsed:?}"
    );
}

const LEFTOVER_PROBE: &str = "BENTO_TASK_LEFTOVER_PROBE";

/// Stands in for a process a finished command left running that ignores SIGTERM but, unlike
/// a shell's background commands, not SIGINT. Publishes its pid in `leftover.pid`.
fn become_leftover() {
    // SAFETY: changing these signal dispositions has no memory-safety preconditions.
    unsafe {
        libc::signal(libc::SIGTERM, libc::SIG_IGN);
        libc::signal(libc::SIGINT, libc::SIG_DFL);
    }
    fs::write("leftover.pid.partial", process::id().to_string()).unwrap();
    fs::rename("leftover.pid.partial", "leftover.pid").unwrap();
    thread::sleep(Duration::from_secs(30));
}

#[test]
fn interrupting_while_leftovers_stop_forwards_the_signal_and_cancels_the_task() {
    if env::var_os(LEFTOVER_PROBE).is_some() {
        become_leftover();
        return;
    }
    let leftover_command = format!(
        "{LEFTOVER_PROBE}=1 '{}' --exact \
         interrupting_while_leftovers_stop_forwards_the_signal_and_cancels_the_task --nocapture",
        env::current_exe().unwrap().display()
    );
    let temp = TempProject::with_tasks(json!({
        "leave": format!(
            "echo $$ > leader.pid; {leftover_command} > /dev/null & \
             until [ -e finish ]; do sleep 0.05; done"
        ),
    }));

    let (cancellation, handle) =
        start(&temp, "leave", Duration::from_secs(10), || temp.path("leftover.pid").exists());
    let leftover = read_pid(&temp, "leftover.pid");
    let leader = read_pid(&temp, "leader.pid");
    temp.write("finish", "");
    wait_until(Duration::from_secs(5), || process_is_gone(leader));
    let interrupted = Instant::now();
    cancellation.cancel(Signal::Interrupt);
    let result = handle.join().unwrap();
    let elapsed = interrupted.elapsed();

    assert_stopped(leftover);
    assert!(
        matches!(&result, Err(TaskError::Cancelled { task, signal: Signal::Interrupt, status: None }) if task == "leave"),
        "{result:?}"
    );
    assert!(elapsed < Duration::from_secs(5), "the interrupt waited for the grace period");
}
