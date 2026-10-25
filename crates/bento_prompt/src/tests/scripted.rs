//! Deterministic engine double used by the crate's behavioral tests.
//!
//! It is compiled only under `cfg(test)` and drives the same internal engine
//! contract as the native backend, so the tests exercise real session
//! transitions instead of a substitute session implementation.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::Value;
use tokio::sync::Notify;

use crate::engine::{
    BoxFuture, Engine, EngineGeneration, EngineSession, Entry, Request, Settlement, Update,
};
use crate::{Availability, CreateOptions, Error, ErrorKind, Message};

/// Removes the installed engine when dropped.
#[cfg(feature = "deno")]
pub(crate) use crate::engine::EngineGuard as Guard;

/// Serialization overhead the double charges for an empty transcript, mirroring
/// the measured native overhead of one token.
pub(crate) const EMPTY_OVERHEAD: usize = 1;

const DEFAULT_WINDOW: usize = 256;

/// Token cost of a transcript: the empty-transcript overhead plus one entry
/// token and one token per whitespace-separated word of every message.
pub(crate) fn transcript_tokens(transcript: &[Message]) -> usize {
    EMPTY_OVERHEAD
        + transcript
            .iter()
            .map(|message| 1 + message.content.split_whitespace().count())
            .sum::<usize>()
}

/// Token cost the double charges for a constraint that is included in the prompt.
pub(crate) fn schema_tokens(source: &Value) -> usize {
    source
        .to_string()
        .split(|character: char| !character.is_alphanumeric())
        .filter(|part| !part.is_empty())
        .count()
}

/// One observable engine event, in the order the engine observed it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Event {
    AvailabilityChecked,
    Opening,
    Opened(Vec<Message>),
    Counting,
    Counted(usize),
    Generated(usize),
    Cancelled(usize),
    Settled(usize),
    Restored(Vec<Message>),
    Released,
}

/// Keeps one kind of native step in flight until it is released, the way a
/// started native call does not stop because a future was dropped.
#[derive(Debug)]
pub(crate) struct Hold {
    gate: Arc<Gate>,
}

impl Hold {
    pub(crate) fn release(&self) {
        self.gate.release();
    }
}

#[derive(Debug, Default)]
struct Gate {
    released: AtomicBool,
    signal: Notify,
}

impl Gate {
    fn release(&self) {
        self.released.store(true, Ordering::SeqCst);
        self.signal.notify_waiters();
    }

    async fn wait(&self) {
        loop {
            let notified = self.signal.notified();
            if self.released.load(Ordering::SeqCst) {
                return;
            }
            notified.await;
        }
    }
}

/// A gate on native token counting that first lets an agreed number of counts
/// run, so a test can hold a later count without disturbing earlier ones.
#[derive(Debug)]
struct CountHold {
    passes: AtomicUsize,
    gate: Arc<Gate>,
}

impl CountHold {
    /// Whether this count runs straight through, spending one allowance.
    fn claim_pass(&self) -> bool {
        self.passes
            .try_update(Ordering::SeqCst, Ordering::SeqCst, |left| left.checked_sub(1))
            .is_ok()
    }
}

/// One generation request the session asked the engine to run.
#[derive(Debug, Clone)]
pub(crate) struct RecordedRequest {
    pub(crate) committed: Vec<Message>,
    pub(crate) prompt: Message,
    pub(crate) schema: Option<Value>,
    pub(crate) include_schema_in_prompt: bool,
}

/// One scripted generation.
#[derive(Debug, Clone)]
pub(crate) struct Script {
    steps: Vec<Step>,
    terminal: Terminal,
    settle_delay_ms: Option<u64>,
}

#[derive(Debug, Clone)]
enum Step {
    Text(String),
    Structured { content_json: String, is_complete: bool },
}

#[derive(Debug, Clone)]
enum Terminal {
    Done,
    Fail(ErrorKind),
    Hang,
}

impl Script {
    /// Cumulative text snapshots followed by successful completion.
    pub(crate) fn text<I, S>(snapshots: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self { steps: text_steps(snapshots), terminal: Terminal::Done, settle_delay_ms: None }
    }

    /// Cumulative text snapshots followed by a failure.
    pub(crate) fn text_then_fail<I, S>(snapshots: I, kind: ErrorKind) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self { steps: text_steps(snapshots), terminal: Terminal::Fail(kind), settle_delay_ms: None }
    }

    /// Cumulative text snapshots followed by a generation that never settles
    /// on its own.
    pub(crate) fn text_then_hang<I, S>(snapshots: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self { steps: text_steps(snapshots), terminal: Terminal::Hang, settle_delay_ms: None }
    }

    /// Cumulative structured snapshots; the last one is the complete result.
    pub(crate) fn structured<I, S>(snapshots: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let mut steps: Vec<Step> = snapshots
            .into_iter()
            .map(|snapshot| Step::Structured { content_json: snapshot.into(), is_complete: false })
            .collect();
        if let Some(Step::Structured { is_complete, .. }) = steps.last_mut() {
            *is_complete = true;
        }
        Self { steps, terminal: Terminal::Done, settle_delay_ms: None }
    }

    /// A generation that fails before emitting anything.
    pub(crate) fn failing(kind: ErrorKind) -> Self {
        Self { steps: Vec::new(), terminal: Terminal::Fail(kind), settle_delay_ms: None }
    }

    /// A generation that emits nothing and never settles until cancelled.
    pub(crate) fn hang() -> Self {
        Self { steps: Vec::new(), terminal: Terminal::Hang, settle_delay_ms: None }
    }

    /// Delay native settlement after the terminal is reached, so disposal is
    /// observably later than cancellation.
    pub(crate) fn settle_after_cancel_ms(mut self, milliseconds: u64) -> Self {
        self.settle_delay_ms = Some(milliseconds);
        self
    }
}

fn text_steps<I, S>(snapshots: I) -> Vec<Step>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    snapshots.into_iter().map(|snapshot| Step::Text(snapshot.into())).collect()
}

#[derive(Debug)]
struct State {
    available: Mutex<bool>,
    languages: Mutex<Vec<String>>,
    window: Mutex<usize>,
    availability_failure: Mutex<Option<ErrorKind>>,
    open_failure: Mutex<Option<ErrorKind>>,
    open_gate: Mutex<Option<Arc<Gate>>>,
    count_hold: Mutex<Option<Arc<CountHold>>>,
    settle_gate: Mutex<Option<Arc<Gate>>>,
    scripts: Mutex<VecDeque<Script>>,
    log: Mutex<Vec<Event>>,
    requests: Mutex<Vec<RecordedRequest>>,
    consumed: Mutex<usize>,
    progress: Notify,
}

impl State {
    fn record(&self, event: Event) {
        self.log.lock().expect("engine log").push(event);
        self.progress.notify_waiters();
    }
}

/// Handle used by tests to script and observe the engine.
#[derive(Debug, Clone)]
pub(crate) struct ScriptedEngine {
    state: Arc<State>,
}

impl ScriptedEngine {
    pub(crate) fn new() -> Self {
        Self {
            state: Arc::new(State {
                available: Mutex::new(true),
                languages: Mutex::new(vec!["en".to_owned(), "en-US".to_owned()]),
                window: Mutex::new(DEFAULT_WINDOW),
                availability_failure: Mutex::new(None),
                open_failure: Mutex::new(None),
                open_gate: Mutex::new(None),
                count_hold: Mutex::new(None),
                settle_gate: Mutex::new(None),
                scripts: Mutex::new(VecDeque::new()),
                log: Mutex::new(Vec::new()),
                requests: Mutex::new(Vec::new()),
                consumed: Mutex::new(0),
                progress: Notify::new(),
            }),
        }
    }

    /// The engine this double installs into a session.
    pub(crate) fn engine(&self) -> Arc<dyn Engine> {
        Arc::new(Self { state: Arc::clone(&self.state) })
    }

    /// Install this engine as the thread-local override that `availability`
    /// and [`crate::Session::create`] consult, so adapter tests exercise the
    /// production entry points. The override is removed when the guard drops.
    #[cfg(feature = "deno")]
    pub(crate) fn install(&self) -> Guard {
        crate::engine::install(self.engine())
    }

    /// Keep every native open in flight until the returned hold is released,
    /// including after the operation that started it was cancelled.
    pub(crate) fn hold_open(&self) -> Hold {
        let gate = Arc::new(Gate::default());
        *self.state.open_gate.lock().expect("open gate") = Some(Arc::clone(&gate));
        Hold { gate }
    }

    /// Keep every generation's native settlement in flight until the returned
    /// hold is released, so disposal is observably unfinished after a cancel.
    pub(crate) fn hold_settlement(&self) -> Hold {
        let gate = Arc::new(Gate::default());
        *self.state.settle_gate.lock().expect("settle gate") = Some(Arc::clone(&gate));
        Hold { gate }
    }

    /// Keep every native token count in flight until the returned hold is
    /// released.
    pub(crate) fn hold_counts(&self) -> Hold {
        self.hold_counts_after(0)
    }

    /// Let the next `passes` native token counts run, then keep every later
    /// one in flight until the returned hold is released.
    ///
    /// One generation counts twice: once for its preflight, and once for the
    /// transcript the generated response would leave behind. Allowing the
    /// preflight through holds the session inside that second native count.
    pub(crate) fn hold_counts_after(&self, passes: usize) -> Hold {
        let gate = Arc::new(Gate::default());
        let hold =
            Arc::new(CountHold { passes: AtomicUsize::new(passes), gate: Arc::clone(&gate) });
        *self.state.count_hold.lock().expect("count hold") = Some(hold);
        Hold { gate }
    }

    pub(crate) fn set_available(&self, available: bool) {
        *self.state.available.lock().expect("availability") = available;
    }

    pub(crate) fn set_languages<I, S>(&self, languages: I)
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        *self.state.languages.lock().expect("languages") =
            languages.into_iter().map(Into::into).collect();
    }

    pub(crate) fn set_context_window(&self, tokens: usize) {
        *self.state.window.lock().expect("window") = tokens;
    }

    /// Fail the next readiness probe, and only that one, so a caller that swallows or
    /// retries a failed probe is visible as a later probe that succeeds.
    pub(crate) fn fail_next_availability(&self, kind: ErrorKind) {
        *self.state.availability_failure.lock().expect("availability failure") = Some(kind);
    }

    pub(crate) fn fail_open(&self, kind: ErrorKind) {
        *self.state.open_failure.lock().expect("open failure") = Some(kind);
    }

    /// Queue one generation script; generations consume them in order.
    pub(crate) fn script(&self, script: Script) {
        self.state.scripts.lock().expect("scripts").push_back(script);
    }

    pub(crate) fn log(&self) -> Vec<Event> {
        self.state.log.lock().expect("engine log").clone()
    }

    pub(crate) fn requests(&self) -> Vec<RecordedRequest> {
        self.state.requests.lock().expect("requests").clone()
    }

    /// Snapshots the engine handed out, used to prove transport bounds.
    pub(crate) fn consumed_steps(&self) -> usize {
        *self.state.consumed.lock().expect("consumed")
    }

    /// Wait until the recorded log satisfies `predicate`.
    pub(crate) async fn wait_for<F>(&self, predicate: F)
    where
        F: Fn(&[Event]) -> bool,
    {
        loop {
            let notified = self.state.progress.notified();
            if predicate(&self.log()) {
                return;
            }
            notified.await;
        }
    }
}

impl Engine for ScriptedEngine {
    fn availability(&self, options: &CreateOptions) -> BoxFuture<'_, Result<Availability, Error>> {
        let available = *self.state.available.lock().expect("availability");
        let supported = self.state.languages.lock().expect("languages").clone();
        let requested: Vec<String> = options
            .expected_inputs
            .iter()
            .chain(options.expected_outputs.iter())
            .flat_map(|expected| expected.languages.clone())
            .collect();
        let failure = self.state.availability_failure.lock().expect("availability failure").take();
        self.state.record(Event::AvailabilityChecked);
        Box::pin(async move {
            if let Some(kind) = failure {
                return Err(Error::new(kind, "scripted availability failure"));
            }
            if !available {
                return Ok(Availability::Unavailable);
            }
            if requested.iter().any(|language| !supported.contains(language)) {
                return Ok(Availability::Unavailable);
            }
            Ok(Availability::Available)
        })
    }

    fn context_window(&self) -> BoxFuture<'_, Result<usize, Error>> {
        let window = *self.state.window.lock().expect("window");
        Box::pin(async move { Ok(window) })
    }

    fn count<'a>(&'a self, transcript: &'a [Entry]) -> BoxFuture<'a, Result<usize, Error>> {
        let state = Arc::clone(&self.state);
        Box::pin(async move {
            state.record(Event::Counting);
            let hold = state.count_hold.lock().expect("count hold").clone();
            if let Some(hold) = hold
                && !hold.claim_pass()
            {
                hold.gate.wait().await;
            }
            let mut total = transcript_tokens(&Entry::messages(transcript));
            for entry in transcript {
                if let Some(constraint) = entry.constraint()
                    && constraint.in_prompt
                {
                    total += schema_tokens(constraint.schema.source());
                }
            }
            state.record(Event::Counted(total));
            Ok(total)
        })
    }

    fn open<'a>(
        &'a self,
        history: &'a [Entry],
    ) -> BoxFuture<'a, Result<Box<dyn EngineSession>, Error>> {
        let state = Arc::clone(&self.state);
        Box::pin(async move {
            state.record(Event::Opening);
            let gate = state.open_gate.lock().expect("open gate").clone();
            if let Some(gate) = gate {
                gate.wait().await;
            }
            if let Some(kind) = *state.open_failure.lock().expect("open failure") {
                return Err(Error::new(kind, "scripted open failure"));
            }
            state.record(Event::Opened(Entry::messages(history)));
            let session: Box<dyn EngineSession> =
                Box::new(ScriptedSession { state, next_generation: 0 });
            Ok(session)
        })
    }
}

struct ScriptedSession {
    state: Arc<State>,
    next_generation: usize,
}

impl EngineSession for ScriptedSession {
    fn generate<'a>(
        &'a mut self,
        request: Request<'a>,
    ) -> BoxFuture<'a, Result<Box<dyn EngineGeneration>, Error>> {
        let state = Arc::clone(&self.state);
        let id = self.next_generation;
        self.next_generation += 1;

        Box::pin(async move {
            let constraint = request.prompt.constraint().cloned();
            state.requests.lock().expect("requests").push(RecordedRequest {
                committed: Entry::messages(request.committed),
                prompt: request.prompt.message(),
                schema: constraint.as_ref().map(|constraint| constraint.schema.source().clone()),
                include_schema_in_prompt: constraint.is_some_and(|constraint| constraint.in_prompt),
            });
            let script = state
                .scripts
                .lock()
                .expect("scripts")
                .pop_front()
                .unwrap_or_else(|| Script::text(["scripted answer"]));
            state.record(Event::Generated(id));
            let generation: Box<dyn EngineGeneration> = Box::new(ScriptedGeneration {
                id,
                state,
                steps: script.steps.into_iter().collect(),
                terminal: script.terminal,
                settle_delay_ms: script.settle_delay_ms,
                received: String::new(),
                cancelled: Arc::new(AtomicBool::new(false)),
                cancel_signal: Arc::new(Notify::new()),
                settled: Arc::new(AtomicBool::new(false)),
                settle_signal: Arc::new(Notify::new()),
                finished: false,
            });
            Ok(generation)
        })
    }

    fn restore<'a>(&'a mut self, transcript: &'a [Entry]) -> BoxFuture<'a, Result<(), Error>> {
        let state = Arc::clone(&self.state);
        let restored = Entry::messages(transcript);
        Box::pin(async move {
            state.record(Event::Restored(restored));
            Ok(())
        })
    }

    fn applied(&mut self, _transcript: &[Entry]) {}

    fn release(&mut self) -> BoxFuture<'_, ()> {
        let state = Arc::clone(&self.state);
        Box::pin(async move {
            state.record(Event::Released);
        })
    }
}

struct ScriptedGeneration {
    id: usize,
    state: Arc<State>,
    steps: VecDeque<Step>,
    terminal: Terminal,
    settle_delay_ms: Option<u64>,
    received: String,
    cancelled: Arc<AtomicBool>,
    cancel_signal: Arc<Notify>,
    settled: Arc<AtomicBool>,
    settle_signal: Arc<Notify>,
    finished: bool,
}

impl ScriptedGeneration {
    async fn settle(&mut self) {
        if !self.finished {
            self.finished = true;
            if let Some(milliseconds) = self.settle_delay_ms {
                tokio::time::sleep(Duration::from_millis(milliseconds)).await;
            }
            let gate = self.state.settle_gate.lock().expect("settle gate").clone();
            if let Some(gate) = gate {
                gate.wait().await;
            }
            self.settled.store(true, Ordering::SeqCst);
            self.state.record(Event::Settled(self.id));
            self.settle_signal.notify_waiters();
        }
    }
}

impl EngineGeneration for ScriptedGeneration {
    fn next(&mut self) -> BoxFuture<'_, Option<Result<Update, Error>>> {
        Box::pin(async move {
            if self.finished {
                return None;
            }
            if self.cancelled.load(Ordering::SeqCst) {
                self.settle().await;
                return Some(Err(Error::new(ErrorKind::Aborted, "scripted cancellation")));
            }
            if let Some(step) = self.steps.pop_front() {
                *self.state.consumed.lock().expect("consumed") += 1;
                let update = match step {
                    Step::Text(snapshot) => {
                        let update = match snapshot.strip_prefix(self.received.as_str()) {
                            Some(delta) => Update::Append(delta.to_owned()),
                            None => Update::Replace(snapshot.clone()),
                        };
                        self.received = snapshot;
                        update
                    }
                    Step::Structured { content_json, is_complete } => {
                        Update::Snapshot { content_json, is_complete }
                    }
                };
                return Some(Ok(update));
            }
            match self.terminal.clone() {
                Terminal::Done => {
                    self.settle().await;
                    None
                }
                Terminal::Fail(kind) => {
                    self.settle().await;
                    Some(Err(Error::new(kind, "scripted generation failure")))
                }
                Terminal::Hang => {
                    let signal = Arc::clone(&self.cancel_signal);
                    while !self.cancelled.load(Ordering::SeqCst) {
                        let notified = signal.notified();
                        if self.cancelled.load(Ordering::SeqCst) {
                            break;
                        }
                        notified.await;
                    }
                    self.settle().await;
                    Some(Err(Error::new(ErrorKind::Aborted, "scripted cancellation")))
                }
            }
        })
    }

    fn cancel(&self) {
        if !self.cancelled.swap(true, Ordering::SeqCst) {
            self.state.record(Event::Cancelled(self.id));
            self.cancel_signal.notify_waiters();
        }
    }

    fn settlement(&self) -> Arc<dyn Settlement> {
        Arc::new(ScriptedSettlement {
            settled: Arc::clone(&self.settled),
            signal: Arc::clone(&self.settle_signal),
        })
    }
}

impl Drop for ScriptedGeneration {
    /// The model's own work settles shortly after the stream is released, even
    /// when nothing drives it any more.
    fn drop(&mut self) {
        if !self.finished {
            self.finished = true;
            self.settled.store(true, Ordering::SeqCst);
            self.state.record(Event::Settled(self.id));
            self.settle_signal.notify_waiters();
        }
    }
}

struct ScriptedSettlement {
    settled: Arc<AtomicBool>,
    signal: Arc<Notify>,
}

impl Settlement for ScriptedSettlement {
    fn settled(&self) -> BoxFuture<'_, ()> {
        Box::pin(async move {
            while !self.settled.load(Ordering::SeqCst) {
                let notified = self.signal.notified();
                if self.settled.load(Ordering::SeqCst) {
                    break;
                }
                notified.await;
            }
        })
    }
}
