//! One conversation: ordered state, generation, forks, and disposal.
//!
//! Every native step a session performs — counting, opening, generating,
//! restoring and releasing — runs on this crate's own runtime, so work that an
//! isolate started still finishes and is still reported when that isolate's
//! runtime is gone. Ownership is taken before the first native step, which is
//! why a creation or clone that never produced a session is still disposable.
//!
//! This module holds the handles a caller sees and the owned futures they
//! await; [`create`] owns creation and cloning, and [`driver`] owns the task
//! that runs one session.

mod create;
mod driver;

use std::future::IntoFuture;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::task::{Context, Poll};

use tokio::sync::{mpsc, oneshot};

#[cfg(test)]
use crate::engine::Engine;
use crate::latch::Latch;
use crate::{Cancellation, CreateOptions, Error, ErrorKind, Message, PromptOptions};

#[cfg(test)]
pub(crate) use create::pending_create_with;
pub(crate) use create::{pending_availability, pending_create};

/// Updates held between the model and a consumer that stopped reading.
pub(crate) const GENERATION_BUFFER: usize = 4;

pub(super) fn aborted(reason: &str) -> Error {
    Error::new(ErrorKind::Aborted, reason)
}

pub(super) fn destroyed() -> Error {
    aborted("the session was destroyed")
}

pub(super) fn unavailable() -> Error {
    Error::new(ErrorKind::NotSupported, "this build has no on-device model backend")
}

/// One conversation with the on-device model.
///
/// State-changing operations apply in the order their methods were called,
/// independently of the order their futures are awaited. Dropping a session
/// cancels its work; [`Session::dispose`] additionally waits for the model to
/// settle.
#[derive(Debug)]
pub struct Session {
    pub(super) commands: mpsc::UnboundedSender<Command>,
    pub(super) shared: Arc<Shared>,
    pub(super) lifetime: Cancellation,
}

#[derive(Debug, Default)]
pub(super) struct Shared {
    pub(super) usage: AtomicUsize,
    pub(super) window: AtomicUsize,
    settled: Latch,
}

impl Shared {
    /// Mark every owned native step complete. Only the owner calls this, and
    /// only once its resources are released.
    pub(super) fn finish(&self) {
        self.settled.raise();
    }
}

/// An operation whose native work is owned from the moment it is created.
///
/// Dropping it cancels the operation; the work it already started still runs
/// to completion. The owning task holds the shared state either way; this
/// handle keeps a reference only where a caller can ask for its disposal.
pub(crate) struct Pending<T> {
    #[cfg(any(feature = "deno", test))]
    pub(super) shared: Arc<Shared>,
    pub(super) cancellation: Cancellation,
    pub(super) result: oneshot::Receiver<Result<T, Error>>,
    pub(super) cancel_on_drop: bool,
}

#[cfg(any(feature = "deno", test))]
impl<T> Pending<T> {
    /// Ownership of every native step this operation performs, available
    /// before the first one starts.
    pub(crate) fn disposal(&self) -> Disposal {
        Disposal::of(Arc::clone(&self.shared))
    }
}

impl<T> Future for Pending<T> {
    type Output = Result<T, Error>;

    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        let pending = self.get_mut();
        match Pin::new(&mut pending.result).poll(context) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(outcome) => {
                pending.cancel_on_drop = false;
                Poll::Ready(outcome.unwrap_or_else(|_| Err(destroyed())))
            }
        }
    }
}

impl<T> Drop for Pending<T> {
    fn drop(&mut self) {
        if self.cancel_on_drop {
            self.cancellation.cancel();
        }
    }
}

impl<T> std::fmt::Debug for Pending<T> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("Pending").finish_non_exhaustive()
    }
}

/// Waits for owned native work to settle.
///
/// Resolves wherever it is awaited, including on another runtime and after the
/// session or operation it came from is gone.
#[derive(Clone)]
pub struct Disposal {
    shared: Arc<Shared>,
}

impl Disposal {
    pub(super) fn of(shared: Arc<Shared>) -> Self {
        Self { shared }
    }

    /// Whether the work this handle owns has already settled.
    #[must_use]
    pub fn is_settled(&self) -> bool {
        self.shared.settled.is_raised()
    }
}

impl std::fmt::Debug for Disposal {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Disposal")
            .field("settled", &self.is_settled())
            .finish_non_exhaustive()
    }
}

impl IntoFuture for Disposal {
    type Output = ();
    type IntoFuture = Pin<Box<dyn Future<Output = ()> + Send>>;

    fn into_future(self) -> Self::IntoFuture {
        Box::pin(async move { self.shared.settled.raised().await })
    }
}

impl Session {
    /// Create a session with the compiled on-device model.
    ///
    /// The supplied cancellation controls the session's lifetime: cancelling it
    /// destroys the session.
    ///
    /// # Errors
    ///
    /// Reports [`ErrorKind::NotSupported`] when no model is compiled in or the
    /// model is unavailable, [`ErrorKind::InvalidInput`] for invalid options,
    /// [`ErrorKind::QuotaExceeded`] when the initial conversation does not fit,
    /// [`ErrorKind::Aborted`] when cancelled, and [`ErrorKind::Operation`] when
    /// the model session cannot be created.
    pub async fn create(options: CreateOptions, cancellation: Cancellation) -> Result<Self, Error> {
        pending_create(options, cancellation).await
    }

    #[cfg(test)]
    pub(crate) fn create_with(
        engine: Arc<dyn Engine>,
        options: CreateOptions,
        cancellation: Cancellation,
    ) -> Pending<Self> {
        pending_create_with(engine, options, cancellation)
    }

    /// Commit messages without generating a response.
    ///
    /// # Errors
    ///
    /// Reports [`ErrorKind::InvalidInput`] for messages the conversation cannot
    /// accept, [`ErrorKind::QuotaExceeded`] when they do not fit, and
    /// [`ErrorKind::Aborted`] when cancelled or destroyed.
    pub fn append(
        &self,
        messages: Vec<Message>,
        cancellation: Cancellation,
    ) -> impl Future<Output = Result<(), Error>> + Send + 'static + use<> {
        self.enqueue(|reply| Command::Append { messages, cancellation, reply })
    }

    /// Prefill messages and generate one response.
    ///
    /// # Errors
    ///
    /// Reports [`ErrorKind::InvalidInput`] or [`ErrorKind::NotSupported`] for
    /// rejected input and constraints, [`ErrorKind::QuotaExceeded`] when the
    /// request does not fit, [`ErrorKind::NotReadable`] when the model refuses,
    /// [`ErrorKind::Aborted`] when cancelled, and [`ErrorKind::Unknown`] for
    /// other generation failures.
    pub fn prompt(
        &self,
        messages: Vec<Message>,
        options: PromptOptions,
        cancellation: Cancellation,
    ) -> impl Future<Output = Result<String, Error>> + Send + 'static + use<> {
        self.enqueue(|reply| Command::Prompt { messages, options, cancellation, reply })
    }

    /// Prefill messages and stream one response.
    ///
    /// Text generation yields incremental deltas; a constrained generation
    /// yields one validated JSON result.
    ///
    /// # Errors
    ///
    /// Reports the same failures as [`Session::prompt`]; failures observed
    /// after generation started are reported by [`Generation::next`].
    pub fn stream(
        &self,
        messages: Vec<Message>,
        options: PromptOptions,
        cancellation: Cancellation,
    ) -> impl Future<Output = Result<Generation, Error>> + Send + 'static + use<> {
        self.enqueue(|reply| Command::Stream { messages, options, cancellation, reply })
    }

    /// Measure what the supplied input and constraint would add, without
    /// changing the conversation.
    ///
    /// # Errors
    ///
    /// Reports [`ErrorKind::InvalidInput`] or [`ErrorKind::NotSupported`] for
    /// rejected input and constraints, and [`ErrorKind::Aborted`] when
    /// cancelled or destroyed.
    pub fn measure_context_usage(
        &self,
        messages: Vec<Message>,
        options: PromptOptions,
        cancellation: Cancellation,
    ) -> impl Future<Output = Result<usize, Error>> + Send + 'static + use<> {
        self.enqueue(|reply| Command::Measure { messages, options, cancellation, reply })
    }

    /// Create an independent session that starts with this conversation.
    ///
    /// # Errors
    ///
    /// Reports [`ErrorKind::Aborted`] when cancelled or destroyed and
    /// [`ErrorKind::Operation`] when the model session cannot be created.
    pub fn fork(
        &self,
        cancellation: Cancellation,
    ) -> impl Future<Output = Result<Self, Error>> + Send + 'static + use<> {
        self.pending_fork(cancellation)
    }

    /// Clone this conversation, owning the clone's native work from the moment
    /// the operation is queued.
    pub(crate) fn pending_fork(&self, cancellation: Cancellation) -> Pending<Self> {
        let shared = Arc::new(Shared::default());
        let (commands, receiver) = mpsc::unbounded_channel();
        let (reply, result) = oneshot::channel();
        let seed = ForkSeed {
            shared: Arc::clone(&shared),
            commands,
            receiver,
            lifetime: Cancellation::new(),
        };

        if let Err(returned) =
            self.commands.send(Command::Fork { cancellation: cancellation.clone(), seed, reply })
        {
            returned.0.abort();
        }

        Pending {
            #[cfg(any(feature = "deno", test))]
            shared,
            cancellation,
            result,
            cancel_on_drop: true,
        }
    }

    /// Tokens the committed conversation occupies.
    #[must_use]
    pub fn context_usage(&self) -> usize {
        self.shared.usage.load(Ordering::SeqCst)
    }

    /// Tokens this session can use in total.
    #[must_use]
    pub fn context_window(&self) -> usize {
        self.shared.window.load(Ordering::SeqCst)
    }

    /// Make the session unusable and cancel its work. Idempotent.
    pub fn destroy(&self) {
        self.lifetime.cancel();
    }

    /// Destroy the session and wait for the model's work to settle.
    pub fn dispose(&self) -> Disposal {
        self.destroy();
        Disposal::of(Arc::clone(&self.shared))
    }

    fn enqueue<T, F>(
        &self,
        build: F,
    ) -> impl Future<Output = Result<T, Error>> + Send + 'static + use<T, F>
    where
        T: Send + 'static,
        F: FnOnce(oneshot::Sender<Result<T, Error>>) -> Command,
    {
        let (reply, response) = oneshot::channel();
        let queued = self.commands.send(build(reply)).is_ok();
        async move {
            if !queued {
                return Err(destroyed());
            }
            response.await.unwrap_or_else(|_| Err(destroyed()))
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.lifetime.cancel();
    }
}

/// A clone's state, created before the clone's native work starts so its
/// disposal exists from the moment the operation is queued.
pub(super) struct ForkSeed {
    pub(super) shared: Arc<Shared>,
    pub(super) commands: mpsc::UnboundedSender<Command>,
    pub(super) receiver: mpsc::UnboundedReceiver<Command>,
    pub(super) lifetime: Cancellation,
}

/// A running generation.
///
/// Dropping it cancels the generation; the session waits for the model to
/// settle before its next operation.
#[derive(Debug)]
pub struct Generation {
    pub(super) chunks: mpsc::Receiver<Result<String, Error>>,
    /// The operation's own cancellation: an abort the caller asked for.
    pub(super) operation: Cancellation,
    /// The session's lifetime: destruction ends this generation too.
    pub(super) lifetime: Cancellation,
    /// This handle's cancellation, from [`Generation::cancel`] or drop.
    pub(super) consumer: Cancellation,
    pub(super) finished: bool,
}

/// What a wait on a running generation produced.
enum Step {
    Chunk(Option<Result<String, Error>>),
    Aborted(Error),
    Closed,
}

impl Generation {
    /// The next chunk, or `None` once the generation ended.
    ///
    /// Cancelling the operation, or destroying the session, ends the
    /// generation at once: chunks the session had already buffered are not
    /// delivered after the abort the caller asked for. Cancelling the
    /// consumer instead closes the generation without inventing an error.
    pub async fn next(&mut self) -> Option<Result<String, Error>> {
        if self.finished {
            return None;
        }

        let step = if self.operation.is_cancelled() {
            Step::Aborted(aborted("the generation was cancelled"))
        } else if self.lifetime.is_cancelled() {
            Step::Aborted(destroyed())
        } else if self.consumer.is_cancelled() {
            Step::Closed
        } else {
            tokio::select! {
                biased;
                () = self.operation.cancelled() => {
                    Step::Aborted(aborted("the generation was cancelled"))
                }
                () = self.lifetime.cancelled() => Step::Aborted(destroyed()),
                () = self.consumer.cancelled() => Step::Closed,
                chunk = self.chunks.recv() => Step::Chunk(chunk),
            }
        };

        match step {
            Step::Chunk(Some(Ok(chunk))) => Some(Ok(chunk)),
            Step::Chunk(Some(Err(error))) => {
                self.finished = true;
                Some(Err(error))
            }
            Step::Chunk(None) | Step::Closed => {
                self.finished = true;
                None
            }
            Step::Aborted(error) => {
                self.finished = true;
                Some(Err(error))
            }
        }
    }

    /// Stop the generation. The reader observes a closed generation, not an
    /// error.
    pub fn cancel(&self) {
        self.consumer.cancel();
    }
}

impl Drop for Generation {
    fn drop(&mut self) {
        self.consumer.cancel();
    }
}

pub(super) enum Command {
    Append {
        messages: Vec<Message>,
        cancellation: Cancellation,
        reply: oneshot::Sender<Result<(), Error>>,
    },
    Measure {
        messages: Vec<Message>,
        options: PromptOptions,
        cancellation: Cancellation,
        reply: oneshot::Sender<Result<usize, Error>>,
    },
    Prompt {
        messages: Vec<Message>,
        options: PromptOptions,
        cancellation: Cancellation,
        reply: oneshot::Sender<Result<String, Error>>,
    },
    Stream {
        messages: Vec<Message>,
        options: PromptOptions,
        cancellation: Cancellation,
        reply: oneshot::Sender<Result<Generation, Error>>,
    },
    Fork {
        cancellation: Cancellation,
        seed: ForkSeed,
        reply: oneshot::Sender<Result<Session, Error>>,
    },
}

impl std::fmt::Debug for Command {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("Command")
    }
}

impl Command {
    /// Answer a command that will never run, releasing anything it owned.
    pub(super) fn abort(self) {
        match self {
            Self::Append { reply, .. } => drop(reply.send(Err(destroyed()))),
            Self::Measure { reply, .. } => drop(reply.send(Err(destroyed()))),
            Self::Prompt { reply, .. } => drop(reply.send(Err(destroyed()))),
            Self::Stream { reply, .. } => drop(reply.send(Err(destroyed()))),
            Self::Fork { seed, reply, .. } => {
                seed.shared.finish();
                drop(reply.send(Err(destroyed())));
            }
        }
    }
}
