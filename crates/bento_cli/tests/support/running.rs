//! Driving a `bento` that keeps running: its output as it arrives, the signals it receives,
//! and the port it serves.

use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use parking_lot::Mutex;

const POLL_INTERVAL: Duration = Duration::from_millis(20);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(2);

pub fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

pub fn loopback(port: u16) -> SocketAddr {
    SocketAddr::from((Ipv4Addr::LOCALHOST, port))
}

pub fn port_is_taken(address: SocketAddr) -> bool {
    matches!(TcpListener::bind(address), Err(error) if error.kind() == io::ErrorKind::AddrInUse)
}

pub fn wait_until(deadline: Duration, mut condition: impl FnMut() -> bool) {
    let start = Instant::now();
    while !condition() {
        assert!(start.elapsed() < deadline, "condition not reached within {deadline:?}");
        thread::sleep(POLL_INTERVAL);
    }
}

pub fn wait_for_exit(child: &mut Child, deadline: Duration) -> ExitStatus {
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            return status;
        }
        if start.elapsed() > deadline {
            child.kill().unwrap();
            panic!("bento did not exit within {deadline:?}");
        }
        thread::sleep(POLL_INTERVAL);
    }
}

/// The response to `GET /` on the loopback `port`, or `None` when nothing answers there.
pub fn get(port: u16) -> Option<String> {
    let mut connection = TcpStream::connect_timeout(&loopback(port), REQUEST_TIMEOUT).ok()?;
    connection.set_read_timeout(Some(REQUEST_TIMEOUT)).unwrap();
    let request = "GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n";
    connection.write_all(request.as_bytes()).ok()?;
    let mut response = String::new();
    connection.read_to_string(&mut response).ok()?;
    Some(response).filter(|response| !response.is_empty())
}

/// Lines a child writes to one of its output streams, collected as they arrive.
#[derive(Clone, Default)]
pub struct Lines(Arc<Mutex<Vec<String>>>);

impl Lines {
    fn collect(stream: impl Read + Send + 'static) -> Lines {
        let lines = Lines::default();
        let collected = lines.clone();
        thread::spawn(move || {
            for line in BufReader::new(stream).lines() {
                let Ok(line) = line else { return };
                collected.0.lock().push(line);
            }
        });
        lines
    }

    pub fn all(&self) -> Vec<String> {
        self.0.lock().clone()
    }

    pub fn text(&self) -> String {
        self.all().join("\n")
    }

    pub fn count(&self, line: &str) -> usize {
        self.all().iter().filter(|written| *written == line).count()
    }
}

/// A `bento` started in the background. Dropping it kills `bento` if it is still running,
/// so that a failing test leaves nothing behind.
pub struct Background {
    child: Child,
    pub stdout: Lines,
    pub stderr: Lines,
}

impl Background {
    pub fn spawn(mut command: Command) -> Background {
        let mut child = command.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
        let stdout = Lines::collect(child.stdout.take().unwrap());
        let stderr = Lines::collect(child.stderr.take().unwrap());
        Background { child, stdout, stderr }
    }

    /// Waits until `bento` has written `line` to standard output `count` times in all.
    pub fn wait_for_stdout(&self, line: &str, count: usize) {
        self.wait_for(&format!("`{line}` printed {count} times"), || {
            (self.stdout.count(line) >= count).then_some(())
        });
    }

    /// Waits for a line of standard output that starts with `prefix`, and returns it.
    pub fn wait_for_stdout_starting(&self, prefix: &str) -> String {
        self.wait_for(&format!("a line starting `{prefix}`"), || {
            self.stdout.all().into_iter().find(|line| line.starts_with(prefix))
        })
    }

    /// Waits until standard error contains `text`.
    pub fn wait_for_stderr(&self, text: &str) {
        self.wait_for(&format!("`{text}` on stderr"), || {
            self.stderr.text().contains(text).then_some(())
        });
    }

    fn wait_for<T>(&self, expected: &str, mut found: impl FnMut() -> Option<T>) -> T {
        let start = Instant::now();
        loop {
            if let Some(found) = found() {
                return found;
            }
            assert!(
                start.elapsed() < Duration::from_secs(20),
                "never saw {expected}; stdout:\n{}\nstderr:\n{}",
                self.stdout.text(),
                self.stderr.text()
            );
            thread::sleep(POLL_INTERVAL);
        }
    }

    pub fn signal(&self, signal: libc::c_int) {
        let pid = libc::pid_t::try_from(self.child.id()).unwrap();
        // SAFETY: `kill` has no memory-safety preconditions; `bento` has not been reaped.
        unsafe { libc::kill(pid, signal) };
    }

    pub fn is_running(&mut self) -> bool {
        self.child.try_wait().unwrap().is_none()
    }

    pub fn wait(&mut self, deadline: Duration) -> ExitStatus {
        wait_for_exit(&mut self.child, deadline)
    }
}

impl Drop for Background {
    fn drop(&mut self) {
        if self.is_running() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}
