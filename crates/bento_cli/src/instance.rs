use std::path::Path;
use std::sync::mpsc::Sender;
use std::thread;

use bento_runtime::{RuntimeConfig, RuntimeError, RuntimeHandle};

use crate::events::Event;

/// A runtime instance executing the script.
pub(crate) struct Instance {
    handle: RuntimeHandle,
}

impl Instance {
    /// Starts executing `entry` under `config`. Once the instance has ended, `Event::Ended`
    /// reports how.
    pub(crate) fn start(
        config: RuntimeConfig,
        entry: &Path,
        events: &Sender<Event>,
    ) -> Result<Instance, RuntimeError> {
        let running = bento_runtime::start(config, entry)?;
        let handle = running.handle();
        let events = events.clone();
        thread::spawn(move || {
            // Sending fails only once `bento` has stopped listening because it is exiting.
            let _ = events.send(Event::Ended(running.wait()));
        });
        Ok(Instance { handle })
    }

    pub(crate) fn handle(&self) -> &RuntimeHandle {
        &self.handle
    }

    /// Requests the runtime's bounded shutdown without waiting for it; `Event::Ended` follows.
    pub(crate) fn shut_down(&self) {
        let handle = self.handle.clone();
        thread::spawn(move || handle.shutdown());
    }
}
