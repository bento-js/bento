//! Behavioral tests for the session semantics of proposal.0002.

pub(crate) mod scripted;

mod accounting;
mod constraints;
mod lifecycle;
mod streaming;

use scripted::ScriptedEngine;

use crate::{Cancellation, CreateOptions, Message, Role, Session};

pub(crate) fn system(content: &str) -> Message {
    Message { role: Role::System, content: content.to_owned() }
}

pub(crate) fn user(content: &str) -> Message {
    Message { role: Role::User, content: content.to_owned() }
}

pub(crate) fn assistant(content: &str) -> Message {
    Message { role: Role::Assistant, content: content.to_owned() }
}

/// A session with no initial prompts, backed by a scripted engine.
pub(crate) async fn scripted_session() -> (ScriptedEngine, Session) {
    let engine = ScriptedEngine::new();
    let session =
        Session::create_with(engine.engine(), CreateOptions::default(), Cancellation::new())
            .await
            .expect("scripted session");
    (engine, session)
}

/// Position of the first matching event, for ordering assertions.
pub(crate) fn position<F>(events: &[scripted::Event], predicate: F) -> usize
where
    F: Fn(&scripted::Event) -> bool,
{
    events
        .iter()
        .position(predicate)
        .unwrap_or_else(|| panic!("expected event missing from {events:?}"))
}
