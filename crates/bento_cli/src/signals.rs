use std::io;
use std::process::ExitCode;
use std::thread;

use signal_hook::consts::{SIGINT, SIGTERM};
use signal_hook::iterator::Signals;

use bento_runtime::Signal;

/// Replaces the default action of SIGINT and SIGTERM sent to `bento`, which would end it at
/// once and leave its work running, with `on_signal`, called on a dedicated thread for each
/// signal received.
pub fn listen(mut on_signal: impl FnMut(Signal) + Send + 'static) -> io::Result<()> {
    let mut signals = Signals::new([SIGINT, SIGTERM])?;
    thread::spawn(move || {
        for number in signals.forever() {
            on_signal(if number == SIGINT { Signal::Interrupt } else { Signal::Terminate });
        }
    });
    Ok(())
}

/// The status of a run that `signal` stopped, as shells report a process it ended: 128 plus
/// the signal's number.
pub fn status(signal: Signal) -> u8 {
    match signal {
        Signal::Interrupt => 130,
        Signal::Terminate => 143,
    }
}

pub fn exit_code(signal: Signal) -> ExitCode {
    ExitCode::from(status(signal))
}
