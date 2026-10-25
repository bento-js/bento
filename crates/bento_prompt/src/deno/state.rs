//! What one isolate's extension owns: the instance's authority, the native cleanup it still
//! owes, and the resources behind the identifiers JavaScript holds.

use std::borrow::Cow;
use std::cell::Cell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use deno_core::{AsyncRefCell, Resource};

use crate::{Cancellation, Disposal, Generation, Session};

/// Whether this instance's guest code may use the operating system's language model.
///
/// Hosts decide once, before the guest runs; nothing in the guest can widen it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Access(pub(super) bool);

impl Access {
    /// Access as the host granted it.
    pub fn new(granted: bool) -> Self {
        Self(granted)
    }

    /// Access to the model.
    pub fn granted() -> Self {
        Self(true)
    }

    /// No access to the model.
    pub fn denied() -> Self {
        Self(false)
    }
}

/// Native cleanup owed by the sessions of one runtime instance.
///
/// Destroying a session cancels its work immediately, but releasing native state and
/// awaiting in-flight callbacks finishes afterwards. A host that must report whether
/// disposal completed shares one of these between its isolates and awaits
/// [`Cleanup::settle`] before saying so.
#[derive(Clone, Default)]
pub struct Cleanup(Arc<Mutex<Vec<Disposal>>>);

impl Cleanup {
    /// Cleanup owing nothing yet.
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether every piece of native work this instance owns has settled. Work is owned
    /// from before it starts, so an operation still inside the model counts as owed.
    ///
    /// # Panics
    ///
    /// Never: this private lock protects only local registration and pruning;
    /// no guest or native code runs while it is held.
    pub fn is_idle(&self) -> bool {
        self.0.lock().expect("cleanup list").iter().all(Disposal::is_settled)
    }

    /// Waits for every owned piece of native work, including work started while waiting.
    /// Returns immediately when nothing is owed.
    ///
    /// # Panics
    ///
    /// Never, for the reason given on [`Cleanup::is_idle`].
    pub async fn settle(&self) {
        loop {
            let pending = {
                let mut owed = self.0.lock().expect("cleanup list");
                owed.retain(|disposal| !disposal.is_settled());
                owed.first().cloned()
            };
            let Some(pending) = pending else {
                return;
            };
            pending.await;
        }
    }

    // Settled handles are dropped as new work arrives, so a guest that keeps asking about
    // availability does not accumulate them.
    pub(super) fn track(&self, disposal: Disposal) {
        let mut owed = self.0.lock().expect("cleanup list");
        owed.retain(|previous| !previous.is_settled());
        owed.push(disposal);
    }
}

// One conversation, owned by the isolate that created it.
pub(super) struct SessionEntry {
    pub(super) session: Session,
    cleanup: Cleanup,
    released: Cell<bool>,
}

impl SessionEntry {
    pub(super) fn new(session: Session, cleanup: Cleanup) -> Self {
        Self { session, cleanup, released: Cell::new(false) }
    }

    // Cancels the session's work and records the native settlement the instance still owes.
    pub(super) fn release(&self) {
        if !self.released.replace(true) {
            self.cleanup.track(self.session.dispose());
        }
    }
}

impl Resource for SessionEntry {
    fn name(&self) -> Cow<'_, str> {
        "languageModelSession".into()
    }

    fn close(self: Rc<Self>) {
        self.release();
    }
}

impl Drop for SessionEntry {
    fn drop(&mut self) {
        self.release();
    }
}

// One running generation. Dropping it stops the native work behind it.
pub(super) struct GenerationEntry {
    pub(super) generation: AsyncRefCell<Generation>,
}

impl Resource for GenerationEntry {
    fn name(&self) -> Cow<'_, str> {
        "languageModelGeneration".into()
    }
}

// A cancellation handle JavaScript keeps for as long as the thing it can cancel exists.
pub(super) struct CancellationEntry(pub(super) Cancellation);

impl Resource for CancellationEntry {
    fn name(&self) -> Cow<'_, str> {
        "languageModelCancellation".into()
    }

    // Instance shutdown closes every resource; cancelling here stops owned model work even
    // when guest code never did.
    fn close(self: Rc<Self>) {
        self.0.cancel();
    }
}
