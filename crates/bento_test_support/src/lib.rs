//! Shared integration-test infrastructure: the helpers more than one crate's tests need.
//!
//! Nothing here belongs to Bento's runtime surface. Crates depend on this one only through
//! `[dev-dependencies]`, so it cannot reach a build of the libraries or the `bento` binary.
//! Everything fails a test by panicking rather than by reporting, except where a caller has to
//! tell "no answer yet" from "a wrong answer".

pub mod http;
pub mod ports;

use std::env;
use std::process::{Command, Output};
use std::sync::{Mutex, PoisonError};

/// How many isolated fixture children a suite may have running at once.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChildLimit {
    /// Children may run in parallel, because each one builds its own copy of the fixture.
    Unbounded,
    /// One child at a time, because the children share a single fixture directory.
    OneAtATime,
}

/// The environment variable naming the test a child process was started to run.
const CHILD_TEST: &str = "BENTO_FIXTURE_TEST";

/// Runs the test called `name` in a child test process that has no `NODE_PATH`, and returns
/// `true` in the parent once that child has passed.
///
/// A fixture instance started in the harness process would otherwise inherit the harness's
/// `NODE_PATH`: the fixture's native resolver reads that variable from the host process rather
/// than from the guest's environment, and would follow it outside the project. The child is
/// this same test binary filtered to `name`; it recognises itself and returns `false`, so the
/// body after the call runs exactly once, there.
///
/// # Panics
///
/// When the child fails, or runs anything other than exactly one test.
#[must_use]
pub fn isolated_fixture_test(name: &str, children: ChildLimit) -> bool {
    static ONE_CHILD: Mutex<()> = Mutex::new(());
    if env::var(CHILD_TEST).as_deref() == Ok(name) {
        assert!(env::var_os("NODE_PATH").is_none(), "fixture process inherited NODE_PATH");
        return false;
    }
    let _one_child = (children == ChildLimit::OneAtATime)
        .then(|| ONE_CHILD.lock().unwrap_or_else(PoisonError::into_inner));
    let child = Command::new(env::current_exe().unwrap())
        .args(["--exact", name, "--nocapture"])
        .env(CHILD_TEST, name)
        .env_remove("NODE_PATH")
        .output()
        .unwrap();
    let report = stdout(&child);
    assert!(child.status.success(), "{report}\n{}", stderr(&child));
    assert!(
        report.lines().any(|line| line.starts_with("test result: ok. 1 passed;")),
        "fixture child did not run exactly one test:\n{report}"
    );
    eprint!("{}", stderr(&child));
    true
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}
