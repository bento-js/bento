#![allow(dead_code, reason = "each integration test binary uses a different subset")]

pub(crate) mod document;
pub(crate) mod guests;
pub(crate) mod http;

use std::env;
use std::fs;
use std::io::Read;
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output};
use std::sync::LazyLock;
use std::time::Duration;

use rusqlite::Connection;
use tempfile::TempDir;

use bento_guestbook_host::launch::{Launch, Mode};
use bento_guestbook_host::store::{Access, GuestBookStore};
use bento_runtime::{RunningRuntime, start};
use bento_test_support::ports::{Addresses, wait_for};

/// Everything disposable the host's tests create lives here.
pub(crate) const SCRATCH: &str = "/tmp/bento-host";

pub(crate) fn scratch_dir() -> TempDir {
    fs::create_dir_all(SCRATCH).unwrap();
    tempfile::Builder::new().prefix("host-").tempdir_in(SCRATCH).unwrap()
}

/// `bento-guestbook-host <arguments>`, started in `directory` without `NODE_PATH`: the guest
/// book's native resolver reads that variable from the host process, not from the guest's
/// environment, and would follow it outside the project.
pub(crate) fn host_command(directory: &Path, arguments: &[&str]) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_bento-guestbook-host"));
    command.args(arguments).current_dir(directory).env_remove("NODE_PATH");
    command
}

pub(crate) fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

pub(crate) fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

const OTHER_RUNTIMES: [&str; 4] = ["node", "bun", "deno", "pnpm"];

/// The test's own `PATH` without every directory that holds `node`, `bun`, `deno`, or `pnpm`,
/// so a host that serves under it cannot be relying on one of them.
pub(crate) fn path_without_other_runtimes() -> String {
    let inherited = env::var_os("PATH").unwrap_or_default();
    let kept: Vec<PathBuf> = env::split_paths(&inherited)
        .filter(|directory| !OTHER_RUNTIMES.iter().any(|name| directory.join(name).exists()))
        .collect();
    let stripped = env::join_paths(kept).unwrap().into_string().unwrap();
    for runtime in OTHER_RUNTIMES {
        let found = Command::new("/usr/bin/which").arg(runtime).env("PATH", &stripped).output();
        assert!(!found.unwrap().status.success(), "`{runtime}` is still on {stripped}");
    }
    stripped
}

/// A port that is free on every local address, so a server binding `::` or `0.0.0.0` can take it.
pub(crate) fn free_port() -> u16 {
    bento_test_support::ports::free_port(Addresses::Every)
}

const STARTUP_ALLOWANCE: Duration = Duration::from_secs(30);

/// Blocks until something accepts connections on `port`, or fails after a generous startup
/// allowance.
pub(crate) fn wait_for_listener(port: u16) {
    let listening = wait_for_listener_unless(port, || false);
    assert!(listening, "nothing started listening on port {port}");
}

/// Waits for a runtime instance to listen on `port`; one that ends first reports how instead
/// of a bare timeout.
pub(crate) fn wait_for_instance(runtime: &RunningRuntime, port: u16) {
    let listening = wait_for_listener_unless(port, || false);
    assert!(listening, "the instance never listened on {port}: {:?}", runtime.handle().shutdown());
}

/// Waits for a spawned host to listen on `port`; one that exits first reports its stderr.
pub(crate) fn wait_for_host(child: &mut Child, port: u16) {
    let listening = wait_for_listener_unless(port, || child.try_wait().unwrap().is_some());
    if !listening {
        let mut diagnostics = String::new();
        child.stderr.take().unwrap().read_to_string(&mut diagnostics).unwrap();
        panic!("the host ended before listening on {port}:\n{diagnostics}");
    }
}

/// Polls `port` until it accepts a connection (`true`), `ended` reports the server gone
/// (`false`), or the allowance runs out (`false`).
fn wait_for_listener_unless(port: u16, ended: impl FnMut() -> bool) -> bool {
    wait_for(STARTUP_ALLOWANCE, || TcpStream::connect(("127.0.0.1", port)).is_ok(), ended)
}

/// The port is free again on every local address, so a replacement server can take it.
pub(crate) fn assert_port_free(port: u16) {
    let listener = TcpListener::bind(("::", port))
        .unwrap_or_else(|error| panic!("port {port} is still held after shutdown: {error}"));
    drop(listener);
}

/// A disposable guest book project holding one small guest instead of the fixture.
pub(crate) fn guest_project(scratch: &Path, name: &str, source: &str) -> PathBuf {
    let project = scratch.join("project");
    fs::create_dir_all(&project).unwrap();
    fs::write(project.join(name), source).unwrap();
    project
}

/// Starts `project/<guest>` with the host's guest book configuration over the store at
/// `database`.
pub(crate) fn start_guest(
    project: &Path,
    guest: &str,
    database: &Path,
    access: Access,
    port: u16,
) -> RunningRuntime {
    let launch = Launch::prepare(project, database, port, Mode::Development).unwrap();
    let store = GuestBookStore::open(launch.database(), access).unwrap();
    start(launch.runtime_config(store), &project.join(guest)).unwrap()
}

/// Starts the fixture copy's embedded entrypoint as the host serves it and waits until it
/// listens.
pub(crate) fn start_fixture(launch: &Launch, access: Access) -> RunningRuntime {
    let store = GuestBookStore::open(launch.database(), access).unwrap();
    let entry = launch.project().join("server.embedded.ts");
    let runtime = start(launch.runtime_config(store), &entry).unwrap();
    wait_for_instance(&runtime, launch.port());
    runtime
}

/// `(name, message, createdAt)` for every row, oldest first, read straight from the store.
pub(crate) fn stored_rows(database: &Path) -> Vec<(String, String, String)> {
    let connection = Connection::open(database).unwrap();
    let mut statement =
        connection.prepare("SELECT name, message, createdAt FROM entries ORDER BY id").unwrap();
    let rows = statement.query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?))).unwrap();
    rows.map(Result::unwrap).collect()
}

/// A disposable copy of the guest book fixture with its locked dependencies installed.
///
/// The copy lives at a fixed path and is brought up to date on first use in each test process:
/// `rsync --delete` mirrors the fixture's sources (never its `node_modules` or databases) and
/// `pnpm install --frozen-lockfile` reconciles the installed graph, which is a no-op when the
/// previous run already installed it.
pub(crate) fn fixture() -> &'static Path {
    static FIXTURE: LazyLock<PathBuf> = LazyLock::new(|| {
        let copy = PathBuf::from(SCRATCH).join("fixture");
        fs::create_dir_all(&copy).unwrap();
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/remix-guest-book/");
        let mirrored = Command::new("rsync")
            .args(["-a", "--delete", "--exclude", "node_modules", "--exclude", "db/*.db"])
            .arg(source)
            .arg(&copy)
            .status()
            .unwrap();
        assert!(mirrored.success(), "rsync of the fixture failed: {mirrored}");
        let installed = Command::new("pnpm")
            .args(["install", "--frozen-lockfile"])
            .current_dir(&copy)
            .status()
            .unwrap();
        assert!(installed.success(), "pnpm install in the fixture copy failed: {installed}");
        copy
    });
    &FIXTURE
}
