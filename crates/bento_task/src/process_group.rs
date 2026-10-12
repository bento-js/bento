use std::io;
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, ExitStatus};
use std::thread;
use std::time::{Duration, Instant};

use crate::cancellation::{Cancellation, Signal};

const POLL_INTERVAL: Duration = Duration::from_millis(10);
/// How long killed processes may take to disappear before the group counts as stuck.
const KILL_DEADLINE: Duration = Duration::from_secs(5);

pub(crate) enum Outcome {
    Exited(ExitStatus),
    /// `status` is how the command ended after hearing `signal`: `None` when it had ended
    /// before the signal arrived, or when anything in its group had to be killed.
    Cancelled {
        signal: Signal,
        status: Option<ExitStatus>,
    },
}

/// A command running as the leader of its own process group, so that everything it
/// starts, however deeply nested, can be stopped together.
pub(crate) struct ProcessGroup {
    leader: Child,
    id: libc::pid_t,
}

impl ProcessGroup {
    pub(crate) fn spawn(mut command: Command) -> io::Result<ProcessGroup> {
        let leader = command.process_group(0).spawn()?;
        let id = libc::pid_t::try_from(leader.id()).expect("process ids fit in pid_t");
        Ok(ProcessGroup { leader, id })
    }

    /// Waits for the leader to exit, then stops whatever it left running in the group. On
    /// cancellation, including one requested while those leftovers stop, forwards the signal
    /// to the whole group. Either way, kills whatever remains after `grace_period` and
    /// returns only once the group is empty.
    pub(crate) fn wait(
        mut self,
        cancellation: &Cancellation,
        grace_period: Duration,
    ) -> io::Result<Outcome> {
        loop {
            if let Some(status) = self.leader.try_wait()? {
                return self.stop_leftovers(status, cancellation, grace_period);
            }
            if let Some(signal) = cancellation.wait(POLL_INTERVAL) {
                let status = self.stop(signal, grace_period)?;
                return Ok(Outcome::Cancelled { signal, status });
            }
        }
    }

    /// Processes a finished leader started in the background would otherwise outlive the
    /// command, beyond the reach of any later cancellation.
    fn stop_leftovers(
        &mut self,
        status: ExitStatus,
        cancellation: &Cancellation,
        grace_period: Duration,
    ) -> io::Result<Outcome> {
        if self.is_empty()? {
            return Ok(Outcome::Exited(status));
        }
        self.send(libc::SIGTERM);
        let deadline = Instant::now() + grace_period;
        let outcome = match self.wait_until_empty_or_cancelled(cancellation, deadline)? {
            Some(signal) => {
                self.send(signal.number());
                Outcome::Cancelled { signal, status: None }
            }
            None => Outcome::Exited(status),
        };
        self.kill_after(deadline)?;
        Ok(outcome)
    }

    /// Forwards `signal` and returns the leader's status if the whole group then ends by itself.
    fn stop(&mut self, signal: Signal, grace_period: Duration) -> io::Result<Option<ExitStatus>> {
        self.send(signal.number());
        if self.kill_after(Instant::now() + grace_period)? {
            return self.leader.try_wait();
        }
        Ok(None)
    }

    /// Kills whatever is still in the group at `deadline`. Returns whether the group emptied
    /// by itself.
    fn kill_after(&mut self, deadline: Instant) -> io::Result<bool> {
        if self.wait_until_empty(deadline)? {
            return Ok(true);
        }
        self.send(libc::SIGKILL);
        if self.wait_until_empty(Instant::now() + KILL_DEADLINE)? {
            return Ok(false);
        }
        Err(io::Error::other(format!("process group {} survived SIGKILL", self.id)))
    }

    /// Reaps the leader and waits for every other member of the group to exit.
    fn wait_until_empty(&mut self, deadline: Instant) -> io::Result<bool> {
        loop {
            if self.has_emptied()? {
                return Ok(true);
            }
            if Instant::now() >= deadline {
                return Ok(false);
            }
            thread::sleep(POLL_INTERVAL);
        }
    }

    /// Like `wait_until_empty`, but returns a cancellation requested before the group empties
    /// or `deadline` passes.
    fn wait_until_empty_or_cancelled(
        &mut self,
        cancellation: &Cancellation,
        deadline: Instant,
    ) -> io::Result<Option<Signal>> {
        loop {
            if self.has_emptied()? || Instant::now() >= deadline {
                return Ok(None);
            }
            if let Some(signal) = cancellation.wait(POLL_INTERVAL) {
                return Ok(Some(signal));
            }
        }
    }

    fn has_emptied(&mut self) -> io::Result<bool> {
        Ok(self.leader.try_wait()?.is_some() && self.is_empty()?)
    }

    fn send(&self, signal: libc::c_int) {
        // SAFETY: `killpg` has no memory-safety preconditions. The group cannot have been
        // reused yet: it has a member until `is_empty` observes otherwise, and nothing is
        // sent after that.
        unsafe { libc::killpg(self.id, signal) };
    }

    fn is_empty(&self) -> io::Result<bool> {
        // SAFETY: signal 0 only checks whether any process in the group still exists.
        if unsafe { libc::killpg(self.id, 0) } == 0 {
            return Ok(false);
        }
        let error = io::Error::last_os_error();
        match error.raw_os_error() {
            Some(libc::ESRCH) => Ok(true),
            Some(libc::EPERM) => Ok(false),
            _ => Err(error),
        }
    }
}
