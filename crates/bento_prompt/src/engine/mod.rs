//! The internal boundary between session semantics and the compiled model
//! implementation. It is private: this crate exposes a concrete session, not a
//! backend registry.

#[cfg(native_backend)]
pub(crate) mod native;
#[cfg(native_backend)]
mod native_schema;

use std::pin::Pin;
use std::sync::Arc;

use crate::schema::Schema;
use crate::{Availability, CreateOptions, Error, Message, Role};

pub(crate) type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// A constraint attached to one request, exactly as the model received it.
#[derive(Debug, Clone)]
pub(crate) struct Constraint {
    pub(crate) schema: Arc<Schema>,
    /// Whether the schema's instructional input was part of the request.
    pub(crate) in_prompt: bool,
}

impl PartialEq for Constraint {
    fn eq(&self, other: &Self) -> bool {
        self.in_prompt == other.in_prompt && self.schema.source() == other.schema.source()
    }
}

/// One entry of a conversation, in the shape the model actually holds it:
/// instructions, a prompt with the response format it was answered under, or a
/// response that is either text or structured content. Keeping this typed is
/// what lets accounting and reconstruction describe the same request.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Entry {
    Instructions { text: String },
    Prompt { text: String, constraint: Option<Constraint> },
    Response { text: String, structure: Option<String> },
}

impl Entry {
    /// The entry a plain message represents.
    pub(crate) fn plain(message: Message) -> Self {
        match message.role {
            Role::System => Self::Instructions { text: message.content },
            Role::User => Self::Prompt { text: message.content, constraint: None },
            Role::Assistant => Self::Response { text: message.content, structure: None },
        }
    }

    pub(crate) fn role(&self) -> Role {
        match self {
            Self::Instructions { .. } => Role::System,
            Self::Prompt { .. } => Role::User,
            Self::Response { .. } => Role::Assistant,
        }
    }

    pub(crate) fn text(&self) -> &str {
        match self {
            Self::Instructions { text }
            | Self::Prompt { text, .. }
            | Self::Response { text, .. } => text,
        }
    }

    pub(crate) fn constraint(&self) -> Option<&Constraint> {
        match self {
            Self::Prompt { constraint, .. } => constraint.as_ref(),
            _ => None,
        }
    }

    pub(crate) fn message(&self) -> Message {
        Message { role: self.role(), content: self.text().to_owned() }
    }

    pub(crate) fn messages(entries: &[Self]) -> Vec<Message> {
        entries.iter().map(Self::message).collect()
    }
}

/// One model implementation: availability, accounting, and session creation.
pub(crate) trait Engine: Send + Sync + 'static {
    fn availability(&self, options: &CreateOptions) -> BoxFuture<'_, Result<Availability, Error>>;

    /// The model's context limit, in the model's own tokens.
    fn context_window(&self) -> BoxFuture<'_, Result<usize, Error>>;

    /// Tokens the model charges for `transcript`, including the response
    /// format every prompt entry carries.
    fn count<'a>(&'a self, transcript: &'a [Entry]) -> BoxFuture<'a, Result<usize, Error>>;

    /// Create a model session whose committed conversation is `history`.
    fn open<'a>(
        &'a self,
        history: &'a [Entry],
    ) -> BoxFuture<'a, Result<Box<dyn EngineSession>, Error>>;
}

/// One model session, owned by exactly one session driver.
pub(crate) trait EngineSession: Send + 'static {
    /// Start one generation. The implementation makes the model session's
    /// committed conversation equal `request.committed` first.
    fn generate<'a>(
        &'a mut self,
        request: Request<'a>,
    ) -> BoxFuture<'a, Result<Box<dyn EngineGeneration>, Error>>;

    /// Restore `transcript` as the committed conversation after a generation
    /// failed or was cancelled.
    fn restore<'a>(&'a mut self, transcript: &'a [Entry]) -> BoxFuture<'a, Result<(), Error>>;

    /// Record that the model session now holds `transcript`, so it is not
    /// rebuilt needlessly. The caller only claims this when the model's own
    /// state is known to match the reconstruction.
    fn applied(&mut self, transcript: &[Entry]);

    /// Release model resources. The caller settles generations first.
    fn release(&mut self) -> BoxFuture<'_, ()>;
}

/// One running generation.
pub(crate) trait EngineGeneration: Send {
    /// The next update, or `None` once the generation reached its terminal.
    fn next(&mut self) -> BoxFuture<'_, Option<Result<Update, Error>>>;

    /// Ask the model to stop. Idempotent, and valid at any point.
    fn cancel(&self);

    /// A settlement handle that outlives this generation and the task driving
    /// it, so a host can wait for the model even after the runtime that
    /// started the work is gone.
    fn settlement(&self) -> Arc<dyn Settlement>;
}

/// Completion of the model's own work, independent of any async runtime.
pub(crate) trait Settlement: Send + Sync + 'static {
    /// Resolve once the model's work and its callbacks have settled.
    fn settled(&self) -> BoxFuture<'_, ()>;
}

/// One update from a running generation.
///
/// Only a compiled engine produces these; a build without one still carries
/// the protocol it would speak.
#[cfg_attr(
    not(any(native_backend, test)),
    expect(dead_code, reason = "no engine is compiled in to produce these updates")
)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Update {
    /// Text appended to what was already delivered.
    Append(String),
    /// A snapshot that rewrites text already delivered.
    Replace(String),
    /// A constrained-generation snapshot.
    Snapshot { content_json: String, is_complete: bool },
}

/// One generation request.
///
/// Only a compiled engine reads these; a build without one still describes
/// the request it would make.
#[cfg_attr(
    not(any(native_backend, test)),
    expect(dead_code, reason = "no engine is compiled in to read this request")
)]
pub(crate) struct Request<'a> {
    /// The conversation that must be committed before generating.
    pub(crate) committed: &'a [Entry],
    /// The prompt entry the model generates a response to, with its response
    /// format.
    pub(crate) prompt: &'a Entry,
}

/// The engine this build compiled, if any.
pub(crate) fn compiled() -> Option<Arc<dyn Engine>> {
    #[cfg(all(test, feature = "deno"))]
    if let Some(engine) = installed() {
        return Some(engine);
    }

    #[cfg(native_backend)]
    {
        Some(native::engine())
    }

    #[cfg(not(native_backend))]
    {
        None
    }
}

#[cfg(all(test, feature = "deno"))]
thread_local! {
    static INSTALLED: std::cell::RefCell<Option<Arc<dyn Engine>>> =
        const { std::cell::RefCell::new(None) };
}

/// The engine installed on this thread by a test, if any.
#[cfg(all(test, feature = "deno"))]
fn installed() -> Option<Arc<dyn Engine>> {
    INSTALLED.with(|slot| slot.borrow().clone())
}

/// Install `engine` for this thread until the returned guard drops.
#[cfg(all(test, feature = "deno"))]
pub(crate) fn install(engine: Arc<dyn Engine>) -> EngineGuard {
    let previous = INSTALLED.with(|slot| slot.borrow_mut().replace(engine));
    EngineGuard { previous }
}

/// Restores the previously installed engine when dropped.
#[cfg(all(test, feature = "deno"))]
pub(crate) struct EngineGuard {
    previous: Option<Arc<dyn Engine>>,
}

#[cfg(all(test, feature = "deno"))]
impl Drop for EngineGuard {
    fn drop(&mut self) {
        let previous = self.previous.take();
        INSTALLED.with(|slot| *slot.borrow_mut() = previous);
    }
}
