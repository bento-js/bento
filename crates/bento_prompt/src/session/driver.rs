//! The task that owns one session.
//!
//! It applies operations in the order they were queued, keeps the model's
//! conversation and this crate's accounting in step, and owns cancellation,
//! native settlement and release. Nothing else touches the model session.

use std::sync::Arc;
use std::sync::atomic::Ordering;

use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::{mpsc, oneshot};

use super::{
    Command, ForkSeed, GENERATION_BUFFER, Generation, Session, Shared, aborted, destroyed,
};
use crate::engine::{Constraint, Engine, EngineGeneration, EngineSession, Entry, Request, Update};
use crate::message::validate_additions;
use crate::schema::Schema;
use crate::{Cancellation, Error, ErrorKind, Message, PromptOptions};

/// Input accepted for one generation, after validation and accounting. The
/// last entry is the prompt the model answers; everything before it is the
/// conversation that must be committed first.
struct Prepared {
    accepted: Vec<Entry>,
    /// The model's own count for `accepted`.
    accepted_tokens: usize,
}

impl Prepared {
    /// The conversation to commit, and the entry the model answers.
    fn split(&self) -> (&[Entry], &Entry) {
        let (prompt, committed) =
            self.accepted.split_last().expect("accepted input always ends with the prompt");
        (committed, prompt)
    }

    /// The entry the model answers, with the response format it carries.
    fn prompt(&self) -> &Entry {
        self.split().1
    }
}

/// What a generation delivered before it ended.
struct Produced {
    text: String,
    /// The last snapshot the model marked complete, for a constrained request.
    complete: Option<String>,
}

/// Where a generation stopped.
enum Ended {
    /// The generation reached its own terminal, with its failure if it had one.
    Terminal(Option<Error>),
    /// The session, the operation, or the consumer stopped it.
    Cancelled,
    /// The model rewrote text a consumer had already received.
    Rewritten,
}

/// Where a generation's chunks go.
enum Sink {
    Channel(mpsc::Sender<Result<String, Error>>),
    Collected,
}

pub(super) struct Actor {
    pub(super) engine: Arc<dyn Engine>,
    pub(super) session: Box<dyn EngineSession>,
    pub(super) history: Vec<Entry>,
    /// The model's own count for the committed conversation.
    pub(super) committed_tokens: usize,
    /// The model's count for an empty transcript, which every conversation
    /// carries and no caller can use.
    pub(super) overhead: usize,
    /// The model's raw context limit.
    pub(super) model_window: usize,
    pub(super) poisoned: Option<Error>,
    pub(super) shared: Arc<Shared>,
    pub(super) lifetime: Cancellation,
    pub(super) commands: mpsc::UnboundedReceiver<Command>,
}

impl Actor {
    /// Drive this session until it is destroyed and released.
    ///
    /// The returned future declares `Send` rather than leaving it to
    /// inference: cloning spawns another session's driver, so inferring it
    /// would depend on itself.
    #[expect(
        clippy::manual_async_fn,
        reason = "the explicit Send return breaks recursive inference through spawned clone drivers"
    )]
    pub(super) fn run(self) -> impl Future<Output = ()> + Send + 'static {
        async move { self.drive().await }
    }

    async fn drive(mut self) {
        loop {
            let command = tokio::select! {
                biased;
                () = self.lifetime.cancelled() => None,
                command = self.commands.recv() => command,
            };
            let Some(command) = command else {
                break;
            };
            self.handle(command).await;
        }

        self.commands.close();
        while let Ok(command) = self.commands.try_recv() {
            command.abort();
        }
        self.session.release().await;
        self.shared.finish();
    }

    async fn handle(&mut self, command: Command) {
        match command {
            Command::Append { messages, cancellation, reply } => {
                let result = self.append(messages, &cancellation).await;
                drop(reply.send(result));
            }
            Command::Measure { messages, options, cancellation, reply } => {
                let result = self.measure(messages, &options, &cancellation).await;
                drop(reply.send(result));
            }
            Command::Prompt { messages, options, cancellation, reply } => {
                let result = self.prompt(messages, options, cancellation).await;
                drop(reply.send(result));
            }
            Command::Stream { messages, options, cancellation, reply } => {
                self.stream(messages, options, cancellation, reply).await;
            }
            Command::Fork { cancellation, seed, reply } => {
                self.fork(&cancellation, seed, reply).await;
            }
        }
    }

    fn guard(&self, cancellation: &Cancellation) -> Result<(), Error> {
        if let Some(error) = &self.poisoned {
            return Err(error.clone());
        }
        if self.lifetime.is_cancelled() {
            return Err(destroyed());
        }
        if cancellation.is_cancelled() {
            return Err(aborted("the operation was cancelled"));
        }
        Ok(())
    }

    /// Publish what a caller sees: the model's own counts less the empty
    /// transcript every conversation carries and nobody can use.
    pub(super) fn publish_accounting(&self) {
        self.shared
            .usage
            .store(self.committed_tokens.saturating_sub(self.overhead), Ordering::SeqCst);
        self.shared.window.store(self.model_window.saturating_sub(self.overhead), Ordering::SeqCst);
    }

    fn commit(&mut self, committed_tokens: usize) {
        self.committed_tokens = committed_tokens;
        self.publish_accounting();
    }

    async fn append(
        &mut self,
        messages: Vec<Message>,
        cancellation: &Cancellation,
    ) -> Result<(), Error> {
        self.guard(cancellation)?;
        validate_additions(&Entry::messages(&self.history), &messages)?;

        let mut candidate = self.history.clone();
        candidate.extend(messages.into_iter().map(Entry::plain));
        let total = self.engine.count(&candidate).await?;
        // Cancellation during counting must not commit anything.
        self.guard(cancellation)?;
        if total > self.model_window {
            return Err(Error::new(
                ErrorKind::QuotaExceeded,
                "the messages do not fit in the remaining context window",
            ));
        }

        self.history = candidate;
        self.commit(total);
        Ok(())
    }

    async fn measure(
        &mut self,
        messages: Vec<Message>,
        options: &PromptOptions,
        cancellation: &Cancellation,
    ) -> Result<usize, Error> {
        self.guard(cancellation)?;
        validate_additions(&Entry::messages(&self.history), &messages)?;
        let constraint = constraint_of(options)?;
        if options.omit_response_constraint_input && constraint.is_none() {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "suppressing the constraint input requires a response constraint",
            ));
        }

        let candidate = self.candidate(messages, constraint);
        let total = self.engine.count(&candidate).await?;
        self.guard(cancellation)?;
        Ok(total.saturating_sub(self.committed_tokens))
    }

    /// The conversation as it would stand after `messages`, with `constraint`
    /// attached to the prompt entry that carries it.
    fn candidate(&self, messages: Vec<Message>, constraint: Option<Constraint>) -> Vec<Entry> {
        let mut candidate = self.history.clone();
        let supplied = messages.len();
        candidate.extend(messages.into_iter().map(Entry::plain));
        match candidate.last_mut() {
            Some(Entry::Prompt { constraint: slot, .. }) if supplied > 0 => *slot = constraint,
            _ => candidate.push(Entry::Prompt { text: String::new(), constraint }),
        }
        candidate
    }

    async fn prepare(
        &mut self,
        messages: Vec<Message>,
        options: &PromptOptions,
        cancellation: &Cancellation,
    ) -> Result<Prepared, Error> {
        self.guard(cancellation)?;
        validate_additions(&Entry::messages(&self.history), &messages)?;
        let constraint = constraint_of(options)?;

        let accepted = self.candidate(messages, constraint);
        let accepted_tokens = self.engine.count(&accepted).await?;
        // Cancellation during preflight must not commit input or generate.
        self.guard(cancellation)?;
        if accepted_tokens > self.model_window {
            return Err(Error::new(
                ErrorKind::QuotaExceeded,
                "the request does not fit in the model's context window",
            ));
        }

        Ok(Prepared { accepted, accepted_tokens })
    }

    async fn start(
        &mut self,
        prepared: &Prepared,
        cancellation: &Cancellation,
    ) -> Result<Box<dyn EngineGeneration>, Error> {
        self.guard(cancellation)?;
        let (committed, prompt) = prepared.split();
        let generation = self.session.generate(Request { committed, prompt }).await?;
        self.history.clone_from(&prepared.accepted);
        self.commit(prepared.accepted_tokens);
        Ok(generation)
    }

    async fn prompt(
        &mut self,
        messages: Vec<Message>,
        options: PromptOptions,
        cancellation: Cancellation,
    ) -> Result<String, Error> {
        let prepared = self.prepare(messages, &options, &cancellation).await?;
        let generation = self.start(&prepared, &cancellation).await?;
        self.pump(prepared, generation, cancellation, Cancellation::new(), Sink::Collected).await
    }

    async fn stream(
        &mut self,
        messages: Vec<Message>,
        options: PromptOptions,
        cancellation: Cancellation,
        reply: oneshot::Sender<Result<Generation, Error>>,
    ) {
        let prepared = match self.prepare(messages, &options, &cancellation).await {
            Ok(prepared) => prepared,
            Err(error) => {
                drop(reply.send(Err(error)));
                return;
            }
        };
        let generation = match self.start(&prepared, &cancellation).await {
            Ok(generation) => generation,
            Err(error) => {
                drop(reply.send(Err(error)));
                return;
            }
        };

        let (sender, receiver) = mpsc::channel(GENERATION_BUFFER);
        let consumer = Cancellation::new();
        let handle = Generation {
            chunks: receiver,
            operation: cancellation.clone(),
            lifetime: self.lifetime.clone(),
            consumer: consumer.clone(),
            finished: false,
        };
        if reply.send(Ok(handle)).is_err() {
            consumer.cancel();
        }

        drop(self.pump(prepared, generation, cancellation, consumer, Sink::Channel(sender)).await);
    }

    async fn pump(
        &mut self,
        prepared: Prepared,
        mut generation: Box<dyn EngineGeneration>,
        cancellation: Cancellation,
        consumer: Cancellation,
        sink: Sink,
    ) -> Result<String, Error> {
        let (produced, ended) =
            collect(&mut generation, &self.lifetime, &cancellation, &consumer, &sink).await;

        // A generation abandoned before its terminal is still the model's to
        // finish: it is stopped, drained, and awaited before anything else
        // touches the model session.
        if !matches!(ended, Ended::Terminal(_)) {
            generation.cancel();
            while generation.next().await.is_some() {}
        }
        generation.settlement().settled().await;
        drop(generation);

        let failure = match ended {
            Ended::Terminal(failure) => failure,
            Ended::Cancelled => Some(aborted("the generation was cancelled")),
            Ended::Rewritten => Some(Error::new(
                ErrorKind::Unknown,
                "the model rewrote text that was already delivered",
            )),
        };

        let result = match failure {
            Some(error) => Err(error),
            None => self.finish(&prepared, produced, &cancellation, &consumer).await,
        };

        match &result {
            Ok(answer) => {
                if let Sink::Channel(sender) = &sink
                    && prepared.prompt().constraint().is_some()
                {
                    deliver(sender, Ok(answer.clone()), &consumer, &self.lifetime).await;
                }
            }
            Err(error) => {
                self.restore(&prepared).await;
                if let Sink::Channel(sender) = &sink {
                    deliver(sender, Err(error.clone()), &consumer, &self.lifetime).await;
                }
            }
        }

        result
    }

    /// Commit a completed response, validating it against any constraint and
    /// against the remaining context window, unless the turn was abandoned
    /// while the model was still counting it.
    async fn finish(
        &mut self,
        prepared: &Prepared,
        produced: Produced,
        cancellation: &Cancellation,
        consumer: &Cancellation,
    ) -> Result<String, Error> {
        let constraint = prepared.prompt().constraint();
        let answer = match constraint {
            None => produced.text,
            Some(constraint) => {
                let json = produced.complete.ok_or_else(|| {
                    Error::new(ErrorKind::Unknown, "the model produced no complete JSON result")
                })?;
                constraint.schema.validate_text(&json)?;
                json
            }
        };

        let mut history = prepared.accepted.clone();
        history.push(Entry::Response {
            text: answer.clone(),
            structure: constraint.map(|constraint| constraint.schema.name()),
        });
        let total = self.engine.count(&history).await?;
        // The count is native work that runs to completion, so an abort that
        // arrives while it is in flight is answered after it, not by dropping
        // it: the turn is then abandoned rather than committing a response its
        // caller or reader will never receive.
        if self.lifetime.is_cancelled() || cancellation.is_cancelled() || consumer.is_cancelled() {
            return Err(aborted("the generation was cancelled"));
        }
        if total > self.model_window {
            return Err(Error::new(
                ErrorKind::QuotaExceeded,
                "the generated response exceeds the model's context window",
            ));
        }

        // A constrained request leaves the model's own transcript holding a
        // response format and structured content. Reproducing that exactly is
        // a rebuild, so the session is only claimed unchanged for plain turns.
        if constraint.is_none() {
            self.session.applied(&history);
        }
        self.history = history;
        self.commit(total);
        Ok(answer)
    }

    /// Put the model session back on the committed conversation after a failed
    /// or cancelled generation.
    async fn restore(&mut self, prepared: &Prepared) {
        if self.lifetime.is_cancelled() {
            return;
        }
        if let Err(error) = self.session.restore(&prepared.accepted).await {
            self.poisoned = Some(Error::new(
                ErrorKind::Unknown,
                format!("the conversation could not be restored: {error}"),
            ));
        }
    }

    async fn fork(
        &mut self,
        cancellation: &Cancellation,
        seed: ForkSeed,
        reply: oneshot::Sender<Result<Session, Error>>,
    ) {
        let history = self.history.clone();
        let opened = match self.open_clone(cancellation, &history).await {
            Ok(opened) => opened,
            Err(error) => {
                seed.shared.finish();
                drop(reply.send(Err(error.for_initialization())));
                return;
            }
        };

        let ForkSeed { shared, commands, receiver, lifetime } = seed;
        let clone_actor = Actor {
            engine: Arc::clone(&self.engine),
            session: opened,
            history,
            committed_tokens: self.committed_tokens,
            overhead: self.overhead,
            model_window: self.model_window,
            poisoned: None,
            shared: Arc::clone(&shared),
            lifetime: lifetime.clone(),
            commands: receiver,
        };
        clone_actor.publish_accounting();

        let clone = Session { commands, shared, lifetime };
        // A driver already runs on this crate's runtime, so the clone's driver
        // joins it without consulting the runtime again.
        tokio::spawn(clone_actor.run());

        // A caller that is gone leaves a clone nobody can use; dropping it here
        // cancels its lifetime, and its owner releases it.
        drop(reply.send(Ok(clone)));
    }

    async fn open_clone(
        &mut self,
        cancellation: &Cancellation,
        history: &[Entry],
    ) -> Result<Box<dyn EngineSession>, Error> {
        self.guard(cancellation)?;
        let mut session = self.engine.open(history).await?;
        if cancellation.is_cancelled() || self.lifetime.is_cancelled() {
            session.release().await;
            return Err(aborted("cloning was cancelled"));
        }
        Ok(session)
    }
}

/// Deliver a generation's updates until it ends, reporting how it ended and
/// what it produced along the way.
async fn collect(
    generation: &mut Box<dyn EngineGeneration>,
    lifetime: &Cancellation,
    cancellation: &Cancellation,
    consumer: &Cancellation,
    sink: &Sink,
) -> (Produced, Ended) {
    let mut produced = Produced { text: String::new(), complete: None };

    let ended = loop {
        let update = tokio::select! {
            biased;
            () = lifetime.cancelled() => break Ended::Cancelled,
            () = cancellation.cancelled() => break Ended::Cancelled,
            () = consumer.cancelled() => break Ended::Cancelled,
            update = generation.next() => update,
        };

        match update {
            None => break Ended::Terminal(None),
            Some(Err(error)) => break Ended::Terminal(Some(error)),
            Some(Ok(Update::Append(delta))) => {
                produced.text.push_str(&delta);
                if let Sink::Channel(sender) = sink {
                    let sent = tokio::select! {
                        biased;
                        () = lifetime.cancelled() => Err(()),
                        () = cancellation.cancelled() => Err(()),
                        () = consumer.cancelled() => Err(()),
                        result = sender.send(Ok(delta)) => result.map_err(drop),
                    };
                    if sent.is_err() {
                        break Ended::Cancelled;
                    }
                }
            }
            Some(Ok(Update::Replace(_))) => break Ended::Rewritten,
            Some(Ok(Update::Snapshot { content_json, is_complete })) => {
                if is_complete {
                    produced.complete = Some(content_json);
                }
            }
        }
    };

    (produced, ended)
}

/// The constraint a request carries, with the input choice it was made with.
fn constraint_of(options: &PromptOptions) -> Result<Option<Constraint>, Error> {
    let Some(source) = options.response_constraint.as_ref() else {
        return Ok(None);
    };
    let schema = Arc::new(Schema::translate(source)?);
    Ok(Some(Constraint { schema, in_prompt: !options.omit_response_constraint_input }))
}

/// Deliver a terminal chunk without blocking a consumer that stopped reading:
/// a full transport means the consumer is no longer draining.
async fn deliver(
    sender: &mpsc::Sender<Result<String, Error>>,
    item: Result<String, Error>,
    consumer: &Cancellation,
    lifetime: &Cancellation,
) {
    match sender.try_send(item) {
        Ok(()) | Err(TrySendError::Closed(_)) => {}
        Err(TrySendError::Full(item)) => {
            tokio::select! {
                biased;
                () = consumer.cancelled() => {}
                () = lifetime.cancelled() => {}
                result = sender.send(item) => drop(result),
            }
        }
    }
}
