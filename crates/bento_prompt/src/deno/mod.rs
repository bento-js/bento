//! Deno extension exposing this crate's sessions as the web Prompt API.
//!
//! The extension installs `globalThis.LanguageModel` in whichever isolate it is registered
//! in, and owns that isolate's sessions through its resource table: identifiers are not
//! shared between isolates, so a worker cannot reach another isolate's conversation.
//!
//! Host authority is immutable for the life of the instance and is checked here, before any
//! model readiness is inspected or any session is created. The JavaScript side holds the web
//! semantics — argument validation, abort reasons, events, and streams — while this side
//! holds ordering, cancellation, and native settlement.

use std::cell::RefCell;
use std::future::Future;
use std::rc::Rc;

use deno_core::{AsyncRefCell, Extension, OpState, RcRef, ResourceId, op2};
use deno_error::JsErrorBox;
use serde::{Deserialize, Serialize};

use crate::schema::Schema;
use crate::session::{Pending, pending_availability, pending_create};
use crate::{
    Availability, Cancellation, CreateOptions, Error, ErrorKind, Expected, Message, PromptOptions,
    Role, Session,
};

mod state;
#[cfg(test)]
mod tests;

pub use state::{Access, Cleanup};

use state::{CancellationEntry, GenerationEntry, SessionEntry, Ticket};

/// Builds the extension that installs the Prompt API into one isolate.
pub fn extension(access: Access, cleanup: Cleanup) -> Extension {
    bento_prompt_deno::init(access, cleanup)
}

/// Why something was refused, in the vocabulary the web surface reports. The message is the
/// failure's own; the draft's error name already says which category it belongs to.
#[derive(Serialize)]
struct Failure {
    error: &'static str,
    message: String,
}

impl Failure {
    fn of(error: &Error) -> Self {
        Self { error: web_error(error.kind()), message: error.message().to_owned() }
    }
}

/// What an operation came to, in the vocabulary the web surface reports.
#[derive(Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
enum Outcome<T> {
    Done { value: T },
    Failed(Failure),
}

impl<T> Outcome<T> {
    fn done(value: T) -> Self {
        Self::Done { value }
    }

    fn failed(error: &Error) -> Self {
        Self::Failed(Failure::of(error))
    }

    fn denied() -> Self {
        Self::Failed(Failure {
            error: "NotAllowedError",
            message: "The host has not granted language-model access".into(),
        })
    }

    fn destroyed() -> Self {
        Self::Failed(Failure {
            error: "AbortError",
            message: "The session has been destroyed".into(),
        })
    }

    fn invalid(message: String) -> Self {
        Self::Failed(Failure { error: "TypeError", message })
    }
}

impl<T> From<Result<T, Error>> for Outcome<T> {
    fn from(result: Result<T, Error>) -> Self {
        match result {
            Ok(value) => Self::done(value),
            Err(error) => Self::failed(&error),
        }
    }
}

// The draft names one error per failure category; these are those names, not Bento's.
fn web_error(kind: ErrorKind) -> &'static str {
    match kind {
        ErrorKind::NotSupported => "NotSupportedError",
        ErrorKind::InvalidInput => "TypeError",
        ErrorKind::QuotaExceeded => "QuotaExceededError",
        ErrorKind::NotReadable => "NotReadableError",
        ErrorKind::Aborted => "AbortError",
        ErrorKind::Operation => "OperationError",
        ErrorKind::Unknown => "UnknownError",
    }
}

#[derive(Deserialize)]
struct MessageInput {
    role: String,
    content: String,
}

impl MessageInput {
    fn into_message(self) -> Result<Message, String> {
        let role = match self.role.as_str() {
            "system" => Role::System,
            "user" => Role::User,
            "assistant" => Role::Assistant,
            other => return Err(format!("unknown message role `{other}`")),
        };
        Ok(Message { role, content: self.content })
    }
}

fn messages(inputs: Vec<MessageInput>) -> Result<Vec<Message>, String> {
    inputs.into_iter().map(MessageInput::into_message).collect()
}

#[derive(Deserialize)]
struct ExpectedInput {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    languages: Vec<String>,
}

impl ExpectedInput {
    fn into_expected(self) -> Expected {
        Expected { kind: self.kind, languages: self.languages }
    }
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateInput {
    #[serde(default)]
    initial_prompts: Vec<MessageInput>,
    #[serde(default)]
    expected_inputs: Vec<ExpectedInput>,
    #[serde(default)]
    expected_outputs: Vec<ExpectedInput>,
}

impl CreateInput {
    fn into_options(self) -> Result<CreateOptions, String> {
        Ok(CreateOptions {
            initial_prompts: messages(self.initial_prompts)?,
            expected_inputs: self
                .expected_inputs
                .into_iter()
                .map(ExpectedInput::into_expected)
                .collect(),
            expected_outputs: self
                .expected_outputs
                .into_iter()
                .map(ExpectedInput::into_expected)
                .collect(),
        })
    }
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PromptInput {
    #[serde(default)]
    response_constraint: Option<serde_json::Value>,
    #[serde(default)]
    omit_response_constraint_input: bool,
}

impl PromptInput {
    fn into_options(self) -> PromptOptions {
        PromptOptions {
            response_constraint: self.response_constraint,
            omit_response_constraint_input: self.omit_response_constraint_input,
        }
    }
}

fn access(state: &Rc<RefCell<OpState>>) -> bool {
    state.borrow().borrow::<Access>().0
}

fn cleanup(state: &Rc<RefCell<OpState>>) -> Cleanup {
    state.borrow().borrow::<Cleanup>().clone()
}

fn session(state: &Rc<RefCell<OpState>>, rid: ResourceId) -> Option<Rc<SessionEntry>> {
    state.borrow().resource_table.get::<SessionEntry>(rid).ok()
}

fn cancellation(state: &Rc<RefCell<OpState>>, rid: ResourceId) -> Option<Cancellation> {
    state.borrow().resource_table.get::<CancellationEntry>(rid).ok().map(|entry| entry.0.clone())
}

// The pieces one session operation needs, looked up before the operation is started so that
// the start itself stays synchronous and JavaScript's call order becomes the session's order.
struct Call {
    session: Rc<SessionEntry>,
    messages: Vec<Message>,
    options: PromptOptions,
    cancellation: Cancellation,
    ticket: Ticket,
}

fn prepare<T>(
    state: &Rc<RefCell<OpState>>,
    rid: ResourceId,
    cancel: ResourceId,
    inputs: Vec<MessageInput>,
    options: PromptInput,
) -> Result<Call, Outcome<T>> {
    let Some(session) = session(state, rid) else {
        return Err(Outcome::destroyed());
    };
    let Some(cancellation) = cancellation(state, cancel) else {
        return Err(Outcome::destroyed());
    };
    let messages = messages(inputs).map_err(Outcome::invalid)?;
    let ticket = session.order.ticket();
    Ok(Call { session, messages, options: options.into_options(), cancellation, ticket })
}

/// Hand one operation's outcome to JavaScript in its session's order: an operation that only
/// ran because an earlier one finished must not settle before that earlier one does.
async fn settle<T>(ticket: Ticket, outcome: Outcome<T>) -> Outcome<T> {
    ticket.turn().await;
    outcome
}

/// The instance's authority, so creation can refuse before running a monitor callback, in
/// the order the shared algorithm specifies. Every path that reaches the model checks it
/// again on this side.
#[op2(fast)]
fn op_bento_prompt_allowed(state: &mut OpState) -> bool {
    state.borrow::<Access>().0
}

/// Decides a response constraint by the translation a generation would use, so the subset
/// has one definition and one answer for a schema it cannot accept. It converts the value as
/// `PromptInput` does, so the call-time answer and the queued generation see the same schema.
#[op2]
#[serde]
fn op_bento_prompt_constraint(#[serde] constraint: serde_json::Value) -> Option<Failure> {
    Schema::translate(&constraint).err().as_ref().map(Failure::of)
}

/// Native work is owned from the moment it is reserved, before the first native step runs,
/// so an isolate that disappears mid-operation still leaves its instance accountable for
/// finishing it.
fn own<T>(state: &Rc<RefCell<OpState>>, pending: Pending<T>) -> Pending<T> {
    cleanup(state).track(pending.disposal());
    pending
}

/// Reserves an availability probe, or reports what to answer without one. A denied
/// instance is honestly unavailable and never reaches the model to find out.
fn start_availability(
    state: &Rc<RefCell<OpState>>,
    options: CreateInput,
) -> Result<Pending<Availability>, Outcome<String>> {
    if !access(state) {
        return Err(Outcome::done("unavailable".to_owned()));
    }
    let options = options.into_options().map_err(Outcome::invalid)?;
    Ok(own(state, pending_availability(options)))
}

#[op2]
#[serde]
fn op_bento_prompt_availability(
    state: Rc<RefCell<OpState>>,
    #[serde] options: CreateInput,
) -> Result<impl Future<Output = Outcome<String>>, JsErrorBox> {
    let started = start_availability(&state, options);
    Ok(async move {
        let pending = match started {
            Ok(pending) => pending,
            Err(outcome) => return outcome,
        };
        match pending.await {
            Ok(Availability::Available) => Outcome::done("available".to_owned()),
            Ok(Availability::Unavailable) => Outcome::done("unavailable".to_owned()),
            Err(error) => Outcome::failed(&error),
        }
    })
}

/// Reserves a creation, refusing in Rust before the model is touched when the host has not
/// granted access.
fn start_create(
    state: &Rc<RefCell<OpState>>,
    options: CreateInput,
    cancel: ResourceId,
) -> Result<Pending<Session>, Outcome<ResourceId>> {
    if !access(state) {
        return Err(Outcome::denied());
    }
    let Some(cancellation) = cancellation(state, cancel) else {
        return Err(Outcome::destroyed());
    };
    let options = options.into_options().map_err(Outcome::invalid)?;
    Ok(own(state, pending_create(options, cancellation)))
}

#[op2]
#[serde]
fn op_bento_prompt_create(
    state: Rc<RefCell<OpState>>,
    #[serde] options: CreateInput,
    #[smi] cancel: ResourceId,
) -> Result<impl Future<Output = Outcome<ResourceId>>, JsErrorBox> {
    let cleanup = cleanup(&state);
    let started = start_create(&state, options, cancel);
    Ok(async move {
        let pending = match started {
            Ok(pending) => pending,
            Err(outcome) => return outcome,
        };
        match pending.await {
            Ok(session) => {
                let entry = SessionEntry::new(session, cleanup);
                Outcome::done(state.borrow_mut().resource_table.add(entry))
            }
            Err(error) => Outcome::failed(&error),
        }
    })
}

#[op2]
#[serde]
fn op_bento_prompt_append(
    state: Rc<RefCell<OpState>>,
    #[smi] rid: ResourceId,
    #[serde] inputs: Vec<MessageInput>,
    #[smi] cancel: ResourceId,
) -> Result<impl Future<Output = Outcome<()>>, JsErrorBox> {
    let started = prepare(&state, rid, cancel, inputs, PromptInput::default())
        .map(|call| (call.ticket, call.session.session.append(call.messages, call.cancellation)));
    Ok(async move {
        match started {
            Ok((ticket, pending)) => settle(ticket, pending.await.into()).await,
            Err(outcome) => outcome,
        }
    })
}

#[op2]
#[serde]
fn op_bento_prompt_prompt(
    state: Rc<RefCell<OpState>>,
    #[smi] rid: ResourceId,
    #[serde] inputs: Vec<MessageInput>,
    #[serde] options: PromptInput,
    #[smi] cancel: ResourceId,
) -> Result<impl Future<Output = Outcome<String>>, JsErrorBox> {
    let started = prepare(&state, rid, cancel, inputs, options).map(|call| {
        (call.ticket, call.session.session.prompt(call.messages, call.options, call.cancellation))
    });
    Ok(async move {
        match started {
            Ok((ticket, pending)) => settle(ticket, pending.await.into()).await,
            Err(outcome) => outcome,
        }
    })
}

#[op2]
#[serde]
fn op_bento_prompt_measure(
    state: Rc<RefCell<OpState>>,
    #[smi] rid: ResourceId,
    #[serde] inputs: Vec<MessageInput>,
    #[serde] options: PromptInput,
    #[smi] cancel: ResourceId,
) -> Result<impl Future<Output = Outcome<f64>>, JsErrorBox> {
    let started = prepare(&state, rid, cancel, inputs, options).map(|call| {
        (
            call.ticket,
            call.session.session.measure_context_usage(
                call.messages,
                call.options,
                call.cancellation,
            ),
        )
    });
    Ok(async move {
        match started {
            Ok((ticket, pending)) => {
                settle(ticket, pending.await.map(|tokens| tokens as f64).into()).await
            }
            Err(outcome) => outcome,
        }
    })
}

#[op2]
#[serde]
fn op_bento_prompt_fork(
    state: Rc<RefCell<OpState>>,
    #[smi] rid: ResourceId,
    #[smi] cancel: ResourceId,
) -> Result<impl Future<Output = Outcome<ResourceId>>, JsErrorBox> {
    let cleanup = cleanup(&state);
    // The cloning operation's cancellation stops the clone being made; the clone that
    // results owns its own lifetime and outlives both the source and this handle.
    let started = match (session(&state, rid), cancellation(&state, cancel)) {
        (Some(entry), Some(cancellation)) => {
            Ok((entry.order.ticket(), own(&state, entry.session.pending_fork(cancellation))))
        }
        _ => Err(Outcome::destroyed()),
    };
    Ok(async move {
        let (ticket, pending) = match started {
            Ok(started) => started,
            Err(outcome) => return outcome,
        };
        let outcome = match pending.await {
            Ok(session) => {
                let entry = SessionEntry::new(session, cleanup);
                Outcome::done(state.borrow_mut().resource_table.add(entry))
            }
            Err(error) => Outcome::failed(&error),
        };
        settle(ticket, outcome).await
    })
}

#[op2]
#[serde]
fn op_bento_prompt_stream(
    state: Rc<RefCell<OpState>>,
    #[smi] rid: ResourceId,
    #[serde] inputs: Vec<MessageInput>,
    #[serde] options: PromptInput,
    #[smi] cancel: ResourceId,
) -> Result<impl Future<Output = Outcome<ResourceId>>, JsErrorBox> {
    let started = prepare(&state, rid, cancel, inputs, options).map(|call| {
        (call.ticket, call.session.session.stream(call.messages, call.options, call.cancellation))
    });
    Ok(async move {
        let (ticket, pending) = match started {
            Ok(started) => started,
            Err(outcome) => return outcome,
        };
        let outcome = match pending.await {
            Ok(generation) => {
                let entry = GenerationEntry { generation: AsyncRefCell::new(generation) };
                Outcome::done(state.borrow_mut().resource_table.add(entry))
            }
            Err(error) => Outcome::failed(&error),
        };
        settle(ticket, outcome).await
    })
}

/// One chunk per call, so a slow or stopped consumer bounds what the transport holds.
#[op2]
#[serde]
fn op_bento_prompt_stream_next(
    state: Rc<RefCell<OpState>>,
    #[smi] rid: ResourceId,
) -> Result<impl Future<Output = Outcome<Option<String>>>, JsErrorBox> {
    let entry = state.borrow().resource_table.get::<GenerationEntry>(rid).ok();
    Ok(async move {
        let Some(entry) = entry else {
            return Outcome::destroyed();
        };
        let mut generation = RcRef::map(entry, |entry| &entry.generation).borrow_mut().await;
        match generation.next().await {
            Some(Ok(chunk)) => Outcome::done(Some(chunk)),
            Some(Err(error)) => Outcome::failed(&error),
            None => Outcome::done(None),
        }
    })
}

#[op2(fast)]
fn op_bento_prompt_usage(state: &mut OpState, #[smi] rid: ResourceId) -> f64 {
    state
        .resource_table
        .get::<SessionEntry>(rid)
        .map_or(f64::NAN, |entry| entry.session.context_usage() as f64)
}

#[op2(fast)]
fn op_bento_prompt_window(state: &mut OpState, #[smi] rid: ResourceId) -> f64 {
    state
        .resource_table
        .get::<SessionEntry>(rid)
        .map_or(f64::NAN, |entry| entry.session.context_window() as f64)
}

/// Immediate, idempotent destruction. Native release settles through [`Cleanup`].
#[op2(fast)]
fn op_bento_prompt_destroy(state: &mut OpState, #[smi] rid: ResourceId) {
    if let Ok(entry) = state.resource_table.take::<SessionEntry>(rid) {
        entry.session.destroy();
        entry.release();
    }
}

#[op2(fast)]
#[smi]
fn op_bento_prompt_cancellation(state: &mut OpState) -> ResourceId {
    state.resource_table.add(CancellationEntry(Cancellation::new()))
}

#[op2(fast)]
fn op_bento_prompt_cancel(state: &mut OpState, #[smi] rid: ResourceId) {
    if let Ok(entry) = state.resource_table.get::<CancellationEntry>(rid) {
        entry.0.cancel();
    }
}

/// Releases a handle whose operation finished, without cancelling anything.
#[op2(fast)]
fn op_bento_prompt_release(state: &mut OpState, #[smi] rid: ResourceId) {
    let _ = state.resource_table.take::<CancellationEntry>(rid);
}

/// Stops a generation and releases it; the session awaits its native settlement before the
/// next operation runs.
#[op2(fast)]
fn op_bento_prompt_stream_close(state: &mut OpState, #[smi] rid: ResourceId) {
    let _ = state.resource_table.take::<GenerationEntry>(rid);
}

deno_core::extension!(bento_prompt_deno,
    ops = [
        op_bento_prompt_allowed,
        op_bento_prompt_constraint,
        op_bento_prompt_availability,
        op_bento_prompt_create,
        op_bento_prompt_append,
        op_bento_prompt_prompt,
        op_bento_prompt_measure,
        op_bento_prompt_fork,
        op_bento_prompt_stream,
        op_bento_prompt_stream_next,
        op_bento_prompt_stream_close,
        op_bento_prompt_usage,
        op_bento_prompt_window,
        op_bento_prompt_destroy,
        op_bento_prompt_cancellation,
        op_bento_prompt_cancel,
        op_bento_prompt_release,
    ],
    esm_entry_point = "ext:bento_prompt_deno/prompt.mjs",
    esm = [dir "src/deno", "input.mjs", "constraint.mjs", "prompt.mjs"],
    options = { access: Access, cleanup: Cleanup },
    state = |state, options| {
        state.put(options.access);
        state.put(options.cleanup);
    },
);
