//! Creating a session, and owning the native work that creation performs.
//!
//! Ownership is taken before the first native step: the shared state a
//! disposal reports on exists as soon as the operation does, so a creation
//! that is abandoned, cancelled, or outlives its caller's runtime is still
//! released and still reported.

use std::sync::Arc;

use tokio::sync::{mpsc, oneshot};

use super::driver::Actor;
use super::{Command, Pending, Session, Shared, aborted, unavailable};
use crate::engine::{Engine, EngineSession, Entry};
use crate::executor;
use crate::message::{expects_text_only, validate_additions, validate_languages};
use crate::{Availability, Cancellation, CreateOptions, Error, ErrorKind};

/// An operation that is already decided: it owns no native work, so its
/// disposal is settled on arrival.
fn settled_pending<T>(outcome: Result<T, Error>) -> Pending<T> {
    let shared = Arc::new(Shared::default());
    shared.finish();
    let (reply, result) = oneshot::channel();
    drop(reply.send(outcome));
    Pending {
        #[cfg(any(feature = "deno", test))]
        shared,
        cancellation: Cancellation::new(),
        result,
        cancel_on_drop: false,
    }
}

/// Check the model's readiness for a configuration, owning the probe so a
/// dropped caller cannot abandon native work.
pub(crate) fn pending_availability(options: CreateOptions) -> Pending<Availability> {
    let Some(engine) = crate::engine::compiled() else {
        // Without a compiled backend the answer needs no native work, so no
        // runtime is started for it.
        return settled_pending(validate_languages(&options).map(|()| Availability::Unavailable));
    };
    let runtime = match executor::runtime() {
        Ok(runtime) => runtime,
        Err(error) => return settled_pending(Err(error)),
    };

    let shared = Arc::new(Shared::default());
    let (reply, result) = oneshot::channel();
    let owner = Arc::clone(&shared);
    runtime.spawn(async move {
        drop(reply.send(crate::availability_with(engine, &options).await));
        owner.finish();
    });
    Pending {
        #[cfg(any(feature = "deno", test))]
        shared,
        cancellation: Cancellation::new(),
        result,
        cancel_on_drop: false,
    }
}

/// Create a session with the compiled model, owning its creation.
pub(crate) fn pending_create(
    options: CreateOptions,
    cancellation: Cancellation,
) -> Pending<Session> {
    let Some(engine) = crate::engine::compiled() else {
        // The same precedence as a creation that reaches the engine: an
        // absent backend is reported before anything else is inspected.
        return settled_pending(Err(unavailable()));
    };
    own_creation(engine, options, cancellation)
}

/// Create a session with a supplied engine, for tests of the behavior the
/// compiled engine drives.
#[cfg(test)]
pub(crate) fn pending_create_with(
    engine: Arc<dyn Engine>,
    options: CreateOptions,
    cancellation: Cancellation,
) -> Pending<Session> {
    own_creation(engine, options, cancellation)
}

fn own_creation(
    engine: Arc<dyn Engine>,
    options: CreateOptions,
    cancellation: Cancellation,
) -> Pending<Session> {
    let runtime = match executor::runtime() {
        Ok(runtime) => runtime,
        Err(error) => return settled_pending(Err(error)),
    };

    let shared = Arc::new(Shared::default());
    let (commands, receiver) = mpsc::unbounded_channel();
    let (reply, result) = oneshot::channel();
    runtime.spawn(create_and_own(Creation {
        engine,
        options,
        cancellation: cancellation.clone(),
        shared: Arc::clone(&shared),
        commands,
        receiver,
        reply,
    }));
    Pending {
        #[cfg(any(feature = "deno", test))]
        shared,
        cancellation,
        result,
        cancel_on_drop: true,
    }
}

struct Creation {
    engine: Arc<dyn Engine>,
    options: CreateOptions,
    cancellation: Cancellation,
    shared: Arc<Shared>,
    commands: mpsc::UnboundedSender<Command>,
    receiver: mpsc::UnboundedReceiver<Command>,
    reply: oneshot::Sender<Result<Session, Error>>,
}

/// Open the session and then own it until it is released. Failure and success
/// both end with the shared state marked settled.
async fn create_and_own(creation: Creation) {
    let Creation { engine, options, cancellation, shared, commands, receiver, reply } = creation;

    let opened = match open_for(engine, options, &cancellation).await {
        Ok(opened) => opened,
        Err(error) => {
            drop(reply.send(Err(error)));
            shared.finish();
            return;
        }
    };

    let actor = Actor {
        engine: opened.engine,
        session: opened.session,
        history: opened.history,
        committed_tokens: opened.committed_tokens,
        overhead: opened.overhead,
        model_window: opened.model_window,
        poisoned: None,
        shared: Arc::clone(&shared),
        lifetime: cancellation.clone(),
        commands: receiver,
    };
    actor.publish_accounting();

    let session = Session { commands, shared, lifetime: cancellation };
    // A caller that is gone leaves a session nobody can use: dropping it here
    // cancels its lifetime, so the owner below releases it immediately.
    drop(reply.send(Ok(session)));

    actor.run().await;
}

struct Opened {
    engine: Arc<dyn Engine>,
    session: Box<dyn EngineSession>,
    history: Vec<Entry>,
    /// The model's own count for `history`, before public accounting.
    committed_tokens: usize,
    /// The model's count for an empty transcript, which every conversation
    /// carries and no caller can use.
    overhead: usize,
    /// The model's raw context limit, before public accounting.
    model_window: usize,
}

async fn open_for(
    engine: Arc<dyn Engine>,
    options: CreateOptions,
    cancellation: &Cancellation,
) -> Result<Opened, Error> {
    if cancellation.is_cancelled() {
        return Err(aborted("session creation was cancelled"));
    }
    validate_languages(&options)?;
    validate_additions(&[], &options.initial_prompts)?;
    if !expects_text_only(&options) {
        return Err(Error::new(
            ErrorKind::NotSupported,
            "only text input and output are supported",
        ));
    }
    if engine.availability(&options).await? != Availability::Available {
        return Err(Error::new(ErrorKind::NotSupported, "the on-device model is unavailable"));
    }

    initialize(engine, options, cancellation).await.map_err(Error::for_initialization)
}

async fn initialize(
    engine: Arc<dyn Engine>,
    options: CreateOptions,
    cancellation: &Cancellation,
) -> Result<Opened, Error> {
    let model_window = engine.context_window().await?;
    let overhead = engine.count(&[]).await?;
    let history: Vec<Entry> = options.initial_prompts.into_iter().map(Entry::plain).collect();
    let committed_tokens = engine.count(&history).await?;
    if committed_tokens > model_window {
        return Err(Error::new(
            ErrorKind::QuotaExceeded,
            "the initial conversation exceeds the model's context window",
        ));
    }
    if cancellation.is_cancelled() {
        return Err(aborted("session creation was cancelled"));
    }

    let mut session = engine.open(&history).await?;
    if cancellation.is_cancelled() {
        session.release().await;
        return Err(aborted("session creation was cancelled"));
    }

    Ok(Opened { engine, session, history, committed_tokens, overhead, model_window })
}
