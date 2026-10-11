use std::io;
use std::thread;

use signal_hook::consts::{SIGINT, SIGTERM};
use signal_hook::iterator::Signals;

use bento_task::{Cancellation, Signal};

/// Turns SIGINT and SIGTERM sent to `bento` into a request to cancel the run, which then
/// stops the running command's whole process group. This replaces their default action,
/// which would end `bento` and leave the task's processes running.
pub fn cancel_on_signal(cancellation: &Cancellation) -> io::Result<()> {
    let mut signals = Signals::new([SIGINT, SIGTERM])?;
    let cancellation = cancellation.clone();
    thread::spawn(move || {
        for number in signals.forever() {
            let signal = if number == SIGINT { Signal::Interrupt } else { Signal::Terminate };
            cancellation.cancel(signal);
        }
    });
    Ok(())
}
