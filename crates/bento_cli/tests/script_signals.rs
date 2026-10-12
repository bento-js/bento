mod support;

use std::time::{Duration, Instant};

use serde_json::json;

use support::TempProject;
use support::running::{Background, loopback, port_is_taken};

/// Listens on a port the system chooses, so that no other process can be holding it, and
/// prints it.
const SERVER: &str = "import http from 'node:http';\n\
const server = http.createServer((request, response) => response.end('served'));\n\
server.listen(0, '127.0.0.1', () => console.log(`listening on ${server.address().port}`));\n";

/// How long a script has to stop after the first signal.
const SHUTDOWN_DEADLINE: Duration = Duration::from_secs(5);
/// How long after the deadline `bento` may take to exit, for scheduling and polling.
const DEADLINE_TOLERANCE: Duration = Duration::from_secs(1);

/// A server script that ends with `handlers`, running in `bento`, and the port it listens on.
struct Serving {
    bento: Background,
    port: u16,
    _temp: TempProject,
}

fn serve(handlers: &str) -> Serving {
    let temp = TempProject::new(json!({ "type": "module" }));
    temp.write("server.mjs", &format!("{SERVER}{handlers}"));
    let command = temp.bento(&["run", "--allow-read=.", "--allow-net", "server.mjs"]);
    let bento = Background::spawn(command);
    let listening = bento.wait_for_stdout_starting("listening on ");
    let port = listening.trim_start_matches("listening on ").parse().unwrap();
    Serving { bento, port, _temp: temp }
}

impl Serving {
    /// Delivers `signal` and returns bento's exit code once it has exited within `deadline`,
    /// after checking that the server's port is free.
    fn stop_with(&mut self, signal: libc::c_int, deadline: Duration) -> Option<i32> {
        self.bento.signal(signal);
        let status = self.bento.wait(deadline);
        assert!(!port_is_taken(loopback(self.port)), "the server's port is still bound");
        status.code()
    }

    /// Delivers SIGINT, expects `bento` to exit at the shutdown deadline, neither before nor
    /// long after it, and returns its exit code.
    fn stop_at_the_deadline(&mut self) -> Option<i32> {
        let interrupted = Instant::now();
        let code = self.stop_with(libc::SIGINT, SHUTDOWN_DEADLINE * 3);
        let elapsed = interrupted.elapsed();
        assert!(elapsed >= SHUTDOWN_DEADLINE, "stopped {elapsed:?} after the signal");
        assert!(
            elapsed < SHUTDOWN_DEADLINE + DEADLINE_TOLERANCE,
            "stopped {elapsed:?} after the signal, past the deadline"
        );
        code
    }
}

#[test]
fn interrupt_runs_the_script_s_handler_and_bento_exits_with_its_code() {
    let mut serving = serve(
        "process.on('SIGINT', () => server.close(() => { console.log('closed'); process.exit(0); }));\n",
    );

    let code = serving.stop_with(libc::SIGINT, Duration::from_secs(4));

    assert_eq!(code, Some(0), "{}", serving.bento.stderr.text());
    assert_eq!(serving.bento.stdout.count("closed"), 1);
}

#[test]
fn a_termination_handler_s_exit_code_becomes_bento_s() {
    let mut serving = serve("process.on('SIGTERM', () => server.close(() => process.exit(7)));\n");

    let code = serving.stop_with(libc::SIGTERM, Duration::from_secs(4));

    assert_eq!(code, Some(7), "{}", serving.bento.stderr.text());
}

/// With no interrupt listener the script is shut down, and shutting down tells its
/// termination listener.
#[test]
fn a_termination_handler_s_exit_code_during_shutdown_becomes_bento_s() {
    let mut serving = serve("process.on('SIGTERM', () => server.close(() => process.exit(7)));\n");

    let code = serving.stop_with(libc::SIGINT, Duration::from_secs(4));

    assert_eq!(code, Some(7), "{}", serving.bento.stderr.text());
}

/// A script that exits with 0 while being shut down ended cleanly by its own choice, so `bento`
/// reports that rather than the signal.
#[test]
fn a_termination_handler_that_exits_0_during_shutdown_makes_bento_exit_0() {
    let mut serving = serve("process.on('SIGTERM', () => server.close(() => process.exit(0)));\n");

    let code = serving.stop_with(libc::SIGINT, Duration::from_secs(4));

    assert_eq!(code, Some(0), "{}", serving.bento.stderr.text());
}

#[test]
fn interrupt_without_a_handler_shuts_the_script_down_with_130() {
    let mut serving = serve("");

    let code = serving.stop_with(libc::SIGINT, Duration::from_secs(6));

    assert_eq!(code, Some(130), "{}", serving.bento.stderr.text());
}

#[test]
fn termination_without_a_handler_shuts_the_script_down_with_143() {
    let mut serving = serve("");

    let code = serving.stop_with(libc::SIGTERM, Duration::from_secs(6));

    assert_eq!(code, Some(143), "{}", serving.bento.stderr.text());
}

#[test]
fn a_second_interrupt_forces_a_script_that_keeps_running() {
    let mut serving = serve("process.on('SIGINT', () => console.log('still here'));\n");
    serving.bento.signal(libc::SIGINT);
    serving.bento.wait_for_stdout("still here", 1);

    let code = serving.stop_with(libc::SIGINT, Duration::from_secs(2));

    assert_eq!(code, Some(130), "{}", serving.bento.stderr.text());
}

#[test]
fn a_script_still_running_after_the_shutdown_deadline_is_shut_down() {
    let mut serving = serve("process.on('SIGINT', () => console.log('still here'));\n");

    let code = serving.stop_at_the_deadline();

    assert_eq!(code, Some(130), "{}", serving.bento.stderr.text());
    assert_eq!(serving.bento.stdout.count("still here"), 1);
}

/// The deadline counts from the signal: a termination listener that would finish later does
/// not extend it for a script that kept running after the interrupt.
#[test]
fn a_slow_termination_handler_does_not_extend_the_deadline() {
    let mut serving = serve(
        "process.on('SIGINT', () => console.log('still here'));\n\
         process.on('SIGTERM', () => setTimeout(() => process.exit(0), 3000));\n",
    );

    let code = serving.stop_at_the_deadline();

    assert_eq!(code, Some(130), "{}", serving.bento.stderr.text());
}

#[test]
fn a_script_stuck_in_javascript_is_interrupted_at_the_deadline() {
    let mut serving = serve("setTimeout(() => { console.log('spinning'); for (;;) {} }, 100);\n");
    serving.bento.wait_for_stdout("spinning", 1);

    let code = serving.stop_at_the_deadline();

    assert_eq!(code, Some(130), "{}", serving.bento.stderr.text());
}

#[test]
fn a_script_stuck_in_javascript_with_a_listener_is_interrupted_at_the_deadline() {
    let mut serving = serve(
        "process.on('SIGINT', () => console.log('never runs'));\n\
         setTimeout(() => { console.log('spinning'); for (;;) {} }, 100);\n",
    );
    serving.bento.wait_for_stdout("spinning", 1);

    let code = serving.stop_at_the_deadline();

    assert_eq!(code, Some(130), "{}", serving.bento.stderr.text());
}

#[test]
fn a_handler_that_throws_exits_1_with_the_error() {
    let mut serving =
        serve("process.on('SIGINT', () => { throw new Error('cleanup failed'); });\n");

    let code = serving.stop_with(libc::SIGINT, Duration::from_secs(4));

    assert_eq!(code, Some(1));
    let message = support::unstyled(&serving.bento.stderr.text());
    assert!(message.contains("uncaught exception"), "{message}");
    assert!(message.contains("cleanup failed"), "{message}");
}
