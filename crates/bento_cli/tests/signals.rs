mod support;

use std::env;
use std::fs;
use std::io;
use std::net::{SocketAddr, TcpListener};
use std::process::{self, Child, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::json;

use support::TempProject;

/// `serve.sh` stands in for a server script: it starts a listener as its own child, so the
/// listener is a grandchild of the task command.
const SERVE_SCRIPT: &str = r#"#!/bin/sh
echo $$ > server.pid
sh -c 'echo $$ > listener.pid; exec nc -l 127.0.0.1 "$1"' listener "$PORT"
"#;

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

fn port_is_taken(address: SocketAddr) -> bool {
    matches!(TcpListener::bind(address), Err(error) if error.kind() == io::ErrorKind::AddrInUse)
}

fn wait_until(deadline: Duration, mut condition: impl FnMut() -> bool) {
    let start = Instant::now();
    while !condition() {
        assert!(start.elapsed() < deadline, "condition not reached within {deadline:?}");
        thread::sleep(Duration::from_millis(20));
    }
}

fn wait_for_exit(child: &mut Child, deadline: Duration) -> ExitStatus {
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            return status;
        }
        if start.elapsed() > deadline {
            child.kill().unwrap();
            panic!("bento did not exit within {deadline:?} of the signal");
        }
        thread::sleep(Duration::from_millis(20));
    }
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
    panic!("process {pid} outlived the cancelled task");
}

fn read_pid(temp: &TempProject, file: &str) -> i32 {
    temp.read(file).trim().parse().unwrap()
}

/// Starts `bento run <task>`, delivers `signal` once the listener is up, and returns how
/// bento exited after checking that nothing from the task is left running.
fn signal_serving_task(task: &str, signal: libc::c_int) -> ExitStatus {
    let port = free_port();
    let address = SocketAddr::from(([127, 0, 0, 1], port));
    let temp =
        TempProject::with_tasks(json!({ "serve": "sh ./serve.sh", "dev": "bento run serve" }));
    temp.write_executable("serve.sh", SERVE_SCRIPT);
    let mut bento = temp
        .bento(&["run", task])
        .env("PORT", port.to_string())
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    wait_until(Duration::from_secs(10), || temp.exists("listener.pid") && port_is_taken(address));
    let pid = libc::pid_t::try_from(bento.id()).unwrap();

    // SAFETY: `kill` has no memory-safety preconditions; `bento` has not been reaped.
    unsafe { libc::kill(pid, signal) };
    let status = wait_for_exit(&mut bento, Duration::from_secs(20));

    assert_stopped(read_pid(&temp, "listener.pid"));
    assert_stopped(read_pid(&temp, "server.pid"));
    assert!(!port_is_taken(address), "the listener's port is still bound");
    status
}

#[test]
fn interrupt_stops_the_whole_task_group_and_exits_130() {
    let status = signal_serving_task("serve", libc::SIGINT);

    assert_eq!(status.code(), Some(130));
}

#[test]
fn termination_stops_the_whole_task_group_and_exits_143() {
    let status = signal_serving_task("serve", libc::SIGTERM);

    assert_eq!(status.code(), Some(143));
}

#[test]
fn interrupt_reaches_a_server_started_by_a_nested_bento() {
    let status = signal_serving_task("dev", libc::SIGINT);

    assert_eq!(status.code(), Some(130));
}

const LEFTOVER_PROBE: &str = "BENTO_CLI_LEFTOVER_PROBE";

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
fn interrupt_while_a_finished_command_s_leftovers_stop_exits_130() {
    if env::var_os(LEFTOVER_PROBE).is_some() {
        become_leftover();
        return;
    }
    let leftover_command = format!(
        "{LEFTOVER_PROBE}=1 '{}' --exact \
         interrupt_while_a_finished_command_s_leftovers_stop_exits_130 --nocapture",
        env::current_exe().unwrap().display()
    );
    let temp = TempProject::with_tasks(json!({
        "leave": format!(
            "echo $$ > leader.pid; {leftover_command} > /dev/null & \
             until [ -e finish ]; do sleep 0.05; done"
        ),
    }));
    let mut bento = temp.bento(&["run", "leave"]).stdout(Stdio::null()).spawn().unwrap();
    wait_until(Duration::from_secs(10), || temp.exists("leftover.pid"));
    let leftover = read_pid(&temp, "leftover.pid");
    let leader = read_pid(&temp, "leader.pid");
    temp.write("finish", "");
    wait_until(Duration::from_secs(5), || process_is_gone(leader));
    let pid = libc::pid_t::try_from(bento.id()).unwrap();

    let interrupted = Instant::now();
    // SAFETY: `kill` has no memory-safety preconditions; `bento` has not been reaped.
    unsafe { libc::kill(pid, libc::SIGINT) };
    let status = wait_for_exit(&mut bento, Duration::from_secs(20));
    let elapsed = interrupted.elapsed();

    assert_stopped(leftover);
    assert_eq!(status.code(), Some(130));
    assert!(elapsed < Duration::from_secs(5), "the interrupt waited for the grace period");
}
