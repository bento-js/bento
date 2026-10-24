//! Ports for test servers, and the waiting that goes with them.

use std::io;
use std::net::{Ipv4Addr, SocketAddr, TcpListener};
use std::thread;
use std::time::{Duration, Instant};

/// How long a wait sleeps between two checks.
const POLL_INTERVAL: Duration = Duration::from_millis(10);

/// The local addresses a port has to be free on.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Addresses {
    /// `127.0.0.1` alone, for a server that binds the loopback interface.
    Loopback,
    /// Every local address, so a server binding `::` or `0.0.0.0` can take the port.
    Every,
}

/// A port nothing is listening on, found by binding one from the ephemeral range and dropping
/// the listener again.
///
/// The port is free when it is returned, not held: a caller is racing anything else on the
/// machine that binds an ephemeral port before its own server starts.
///
/// # Panics
///
/// When the machine has no free ephemeral port.
#[must_use]
pub fn free_port(addresses: Addresses) -> u16 {
    let any = match addresses {
        Addresses::Loopback => "127.0.0.1:0",
        Addresses::Every => "[::]:0",
    };
    TcpListener::bind(any).unwrap().local_addr().unwrap().port()
}

/// The loopback address of `port`.
#[must_use]
pub fn loopback(port: u16) -> SocketAddr {
    SocketAddr::from((Ipv4Addr::LOCALHOST, port))
}

/// Whether something is already listening on `address`.
#[must_use]
pub fn port_is_taken(address: SocketAddr) -> bool {
    matches!(TcpListener::bind(address), Err(error) if error.kind() == io::ErrorKind::AddrInUse)
}

/// Blocks until `condition` holds.
///
/// # Panics
///
/// When `deadline` passes first.
pub fn wait_until(deadline: Duration, condition: impl FnMut() -> bool) {
    let reached = wait_for(deadline, condition, || false);
    assert!(reached, "condition not reached within {deadline:?}");
}

/// Blocks until `condition` holds (`true`), `abandoned` holds (`false`), or `deadline` passes
/// (`false`).
///
/// `abandoned` lets a caller that can tell the server is gone report that instead of waiting
/// out the deadline for an answer that will never come.
pub fn wait_for(
    deadline: Duration,
    mut condition: impl FnMut() -> bool,
    mut abandoned: impl FnMut() -> bool,
) -> bool {
    let expires = Instant::now() + deadline;
    while !condition() {
        if abandoned() || Instant::now() >= expires {
            return false;
        }
        thread::sleep(POLL_INTERVAL);
    }
    true
}
