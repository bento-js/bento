//! Apple Foundation Models backend for macOS on Apple silicon.
//!
//! Every synchronous native call — model lookup, availability, context size,
//! locale matching, schema compilation, session construction and starting a
//! generation — runs on a blocking thread, so an embedding event loop stays
//! responsive. Generation always uses the cancellable native stream, and a
//! cancelled request settles before its session is reused or released.

use std::sync::{Arc, LazyLock};

use foundation_models::stream::{
    Generation as NativeGeneration, GenerationHandle, GenerationUpdate,
};
use foundation_models::{
    Availability as NativeAvailability, ConfiguredSystemLanguageModel, FMError, GeneratedContent,
    GenerationOptions, GenerationSchema, Instructions, LanguageModelSession, Prompt,
    ResponseFormat, Segment, SystemLanguageModel, Transcript, TranscriptEntry,
    TranscriptInstructions, TranscriptPrompt, TranscriptResponse,
};
use serde_json::Value;
use tokio::sync::Mutex;

use crate::engine::native_schema::dynamic_schema;
use crate::engine::{
    BoxFuture, Engine, EngineGeneration, EngineSession, Entry, Request, Settlement, Update,
};
use crate::schema::Schema;
use crate::{Availability, CreateOptions, Error, ErrorKind};

/// Process-wide native state: the system model and the most recently compiled
/// constraint, so measuring and generating with one constraint compiles it
/// once. Initialization touches the framework, so it is only forced from a
/// blocking thread.
static STATE: LazyLock<State> = LazyLock::new(|| State {
    model: SystemLanguageModel::default_model(),
    compiled_schema: Mutex::new(None),
});

struct State {
    model: Option<ConfiguredSystemLanguageModel>,
    compiled_schema: Mutex<Option<(Value, Arc<GenerationSchema>)>>,
}

/// The compiled backend.
pub(crate) fn engine() -> Arc<dyn Engine> {
    Arc::new(NativeEngine)
}

struct NativeEngine;

fn model() -> Result<&'static ConfiguredSystemLanguageModel, Error> {
    STATE.model.as_ref().ok_or_else(|| {
        Error::new(ErrorKind::NotSupported, "the system language model is unavailable")
    })
}

/// Resolve the system model on a blocking thread, forcing framework
/// initialization off the caller's thread.
async fn model_off_thread() -> Result<&'static ConfiguredSystemLanguageModel, Error> {
    tokio::task::spawn_blocking(model).await.map_err(joined)?
}

/// The native schema for `schema`, reusing the cached one when it matches.
pub(super) async fn native_schema(schema: &Schema) -> Result<Arc<GenerationSchema>, Error> {
    let mut cache = STATE.compiled_schema.lock().await;
    if let Some((source, compiled)) = cache.as_ref()
        && source == schema.source()
    {
        return Ok(Arc::clone(compiled));
    }

    let root = dynamic_schema(schema.root(), &schema.name());
    let compiled =
        Arc::new(blocking(move || GenerationSchema::from_dynamic(root, Vec::new())).await?);
    *cache = Some((schema.source().clone(), Arc::clone(&compiled)));
    Ok(compiled)
}

impl Engine for NativeEngine {
    fn availability(&self, options: &CreateOptions) -> BoxFuture<'_, Result<Availability, Error>> {
        // Owned here because the probe crosses onto a blocking thread.
        let languages: Vec<String> =
            crate::message::requested_languages(options).map(ToOwned::to_owned).collect();
        Box::pin(async move {
            tokio::task::spawn_blocking(move || {
                let model = model()?;
                if !matches!(model.availability(), NativeAvailability::Available) {
                    return Ok(Availability::Unavailable);
                }
                for language in &languages {
                    if !model.supports_locale(language) {
                        return Ok(Availability::Unavailable);
                    }
                }
                Ok(Availability::Available)
            })
            .await
            .map_err(joined)?
        })
    }

    fn context_window(&self) -> BoxFuture<'_, Result<usize, Error>> {
        Box::pin(async move {
            tokio::task::spawn_blocking(|| Ok(model()?.context_size())).await.map_err(joined)?
        })
    }

    fn count<'a>(&'a self, transcript: &'a [Entry]) -> BoxFuture<'a, Result<usize, Error>> {
        Box::pin(async move {
            let model = model_off_thread().await?;
            let native = native_transcript(transcript).await?;
            model.token_count_for_transcript(&native).await.map_err(map_error)
        })
    }

    fn open<'a>(
        &'a self,
        history: &'a [Entry],
    ) -> BoxFuture<'a, Result<Box<dyn EngineSession>, Error>> {
        Box::pin(async move {
            let session = open_session(history).await?;
            let opened: Box<dyn EngineSession> =
                Box::new(NativeSessionHandle { session: Some(session), basis: history.to_vec() });
            Ok(opened)
        })
    }
}

struct NativeSessionHandle {
    session: Option<Arc<LanguageModelSession>>,
    /// Conversation the native session currently holds, or empty once a
    /// generation mutated it.
    basis: Vec<Entry>,
}

impl NativeSessionHandle {
    async fn rebuild(&mut self, history: &[Entry]) -> Result<(), Error> {
        self.session = Some(open_session(history).await?);
        self.basis = history.to_vec();
        Ok(())
    }

    fn session(&self) -> Result<Arc<LanguageModelSession>, Error> {
        self.session
            .as_ref()
            .map(Arc::clone)
            .ok_or_else(|| Error::new(ErrorKind::Aborted, "the model session was released"))
    }
}

impl EngineSession for NativeSessionHandle {
    fn generate<'a>(
        &'a mut self,
        request: Request<'a>,
    ) -> BoxFuture<'a, Result<Box<dyn EngineGeneration>, Error>> {
        Box::pin(async move {
            if self.basis != request.committed {
                self.rebuild(request.committed).await?;
            }

            let session = self.session()?;
            let retained = Arc::clone(&session);
            let content = request.prompt.text().to_owned();
            let constrained = match request.prompt.constraint() {
                Some(constraint) => {
                    Some((native_schema(&constraint.schema).await?, constraint.in_prompt))
                }
                None => None,
            };

            let generation = tokio::task::spawn_blocking(move || match constrained {
                None => session.stream_text(Prompt::text(content), GenerationOptions::new()),
                Some((schema, in_prompt)) => session.stream_generated_async(
                    Prompt::text(content),
                    &schema,
                    in_prompt,
                    GenerationOptions::new(),
                ),
            })
            .await
            .map_err(joined)?
            .map_err(map_error)?;

            // The model appends the request and its response to its own
            // transcript, so the basis only becomes valid again once the
            // caller states what the session holds.
            self.basis.clear();

            let handle = generation.handle();
            let started: Box<dyn EngineGeneration> =
                Box::new(NativeStream { generation, handle, session: retained });
            Ok(started)
        })
    }

    fn restore<'a>(&'a mut self, transcript: &'a [Entry]) -> BoxFuture<'a, Result<(), Error>> {
        Box::pin(async move { self.rebuild(transcript).await })
    }

    fn applied(&mut self, transcript: &[Entry]) {
        self.basis = transcript.to_vec();
    }

    fn release(&mut self) -> BoxFuture<'_, ()> {
        let session = self.session.take();
        self.basis.clear();
        Box::pin(async move {
            if let Some(session) = session {
                drop(tokio::task::spawn_blocking(move || drop(session)).await);
            }
        })
    }
}

struct NativeStream {
    generation: NativeGeneration,
    handle: GenerationHandle,
    /// Retained so the model session outlives a cancelled request's rollback,
    /// even when the task driving this generation is dropped.
    session: Arc<LanguageModelSession>,
}

impl EngineGeneration for NativeStream {
    fn next(&mut self) -> BoxFuture<'_, Option<Result<Update, Error>>> {
        Box::pin(async move {
            let update = self.generation.next().await?;
            Some(match update {
                Ok(GenerationUpdate::Chunk(delta)) => Ok(Update::Append(delta)),
                Ok(GenerationUpdate::Replace(text)) => Ok(Update::Replace(text)),
                Ok(GenerationUpdate::Snapshot(snapshot)) => Ok(Update::Snapshot {
                    content_json: snapshot.content_json,
                    is_complete: snapshot.is_complete,
                }),
                Ok(_) => Err(Error::new(
                    ErrorKind::Unknown,
                    "the model reported an update this build does not understand",
                )),
                Err(error) => Err(map_error(error)),
            })
        })
    }

    fn cancel(&self) {
        self.handle.cancel();
    }

    fn settlement(&self) -> Arc<dyn Settlement> {
        Arc::new(NativeSettlement {
            handle: self.handle.clone(),
            _session: Arc::clone(&self.session),
        })
    }
}

/// Native settlement is raised by the model's own task, so it resolves on any
/// runtime, including after the one that started the work has shut down. The
/// retained session keeps the model object alive until that rollback finished.
struct NativeSettlement {
    handle: GenerationHandle,
    _session: Arc<LanguageModelSession>,
}

impl Settlement for NativeSettlement {
    fn settled(&self) -> BoxFuture<'_, ()> {
        Box::pin(async move { self.handle.settled().await })
    }
}

async fn open_session(history: &[Entry]) -> Result<Arc<LanguageModelSession>, Error> {
    let transcript = native_transcript(history).await?;
    let session = blocking(move || LanguageModelSession::from_transcript(transcript)).await?;
    Ok(Arc::new(session))
}

/// Run synchronous native work off the caller's thread.
async fn blocking<T, F>(work: F) -> Result<T, Error>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, FMError> + Send + 'static,
{
    match tokio::task::spawn_blocking(work).await {
        Ok(result) => result.map_err(map_error),
        Err(error) => Err(joined(error)),
    }
}

fn joined(error: tokio::task::JoinError) -> Error {
    Error::new(ErrorKind::Unknown, format!("native work could not be completed: {error}"))
}

/// Build the transcript this conversation represents, including the response
/// format a constrained prompt carried and the structured content a
/// constrained response produced, so counting and reconstruction describe the
/// same request the model answered.
async fn native_transcript(entries: &[Entry]) -> Result<Transcript, Error> {
    let mut transcript: Vec<TranscriptEntry> = Vec::with_capacity(entries.len());
    for entry in entries {
        let built = match entry {
            Entry::Instructions { text } => {
                let mut instructions = Instructions::new();
                instructions.push_text(text.clone());
                TranscriptEntry::Instructions(TranscriptInstructions::new(instructions))
            }
            Entry::Prompt { text, constraint } => {
                let mut prompt = TranscriptPrompt::new(Prompt::text(text.clone()));
                if let Some(constraint) = constraint.as_ref()
                    && constraint.in_prompt
                {
                    let compiled = native_schema(&constraint.schema).await?;
                    prompt.response_format = Some(ResponseFormat::json_schema((*compiled).clone()));
                }
                TranscriptEntry::Prompt(prompt)
            }
            Entry::Response { text, structure } => {
                let segment = match structure {
                    Some(name) => {
                        let content = GeneratedContent::from_json_str(text).map_err(map_error)?;
                        Segment::structure(name.clone(), content)
                    }
                    None => Segment::text(text.clone()),
                };
                TranscriptEntry::Response(TranscriptResponse::new(vec![segment]))
            }
        };
        transcript.push(built);
    }
    Ok(Transcript::from(transcript))
}

/// Translate the model's typed failures. Categories come from variants, never
/// from message text.
fn map_error(error: FMError) -> Error {
    let kind = match &error {
        FMError::ContextWindowExceeded(_) => ErrorKind::QuotaExceeded,
        // Throttling is not context exhaustion: it must not be reported as a
        // quota failure, which would fabricate a context-overflow event.
        FMError::RateLimited(_) => ErrorKind::Unknown,
        FMError::GuardrailViolation(_) | FMError::Refusal(_) => ErrorKind::NotReadable,
        FMError::UnsupportedLanguage(_)
        | FMError::UnsupportedGuide(_)
        | FMError::UnsupportedCapability(_)
        | FMError::UnsupportedTranscriptContent(_)
        | FMError::ModelUnavailable { .. }
        | FMError::AssetsUnavailable(_) => ErrorKind::NotSupported,
        FMError::Cancelled => ErrorKind::Aborted,
        _ => ErrorKind::Unknown,
    };

    let mut message = error.message().to_owned();
    if kind == ErrorKind::QuotaExceeded
        && let Some(details) = error.language_model_error_details()
        && let (Some(limit), Some(used)) = (details.context_size(), details.token_count())
    {
        message = format!("{message} ({used} of {limit} tokens)");
    }
    Error::new(kind, message)
}

#[cfg(test)]
mod tests {
    use super::map_error;
    use crate::ErrorKind;
    use foundation_models::FMError;

    #[test]
    fn decoding_failure_is_not_an_initialization_error() {
        let error = map_error(FMError::DecodingFailure("invalid generated JSON".into()));

        assert_eq!(error.kind(), ErrorKind::Unknown);
    }
}
