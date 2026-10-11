use std::path::{Path, PathBuf};

use deno_core::error::{CoreError, CoreErrorKind, JsError};
use deno_error::JsErrorClass;
use thiserror::Error;

use crate::sources::Sources;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorKind {
    Startup,
    Resolution,
    Transform,
    Permission,
    Uncaught,
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
#[error("{message}")]
pub struct RuntimeError {
    pub kind: ErrorKind,
    pub message: String,
    location: Option<SourceLocation>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceLocation {
    pub path: PathBuf,
    pub line: u32,
    pub column: u32,
}

pub type StartError = RuntimeError;

#[derive(Debug)]
pub enum RunOutcome {
    Completed,
    Exited(i32),
    Failed(RuntimeError),
    Shutdown(crate::ShutdownReport),
}

impl RuntimeError {
    pub fn location(&self) -> Option<&SourceLocation> {
        self.location.as_ref()
    }

    pub(crate) fn transform(message: impl Into<String>, location: Option<SourceLocation>) -> Self {
        Self { kind: ErrorKind::Transform, message: message.into(), location }
    }

    pub(crate) fn startup(message: impl Into<String>) -> Self {
        Self { kind: ErrorKind::Startup, message: message.into(), location: None }
    }

    pub(crate) fn guest(error: CoreError, entry: &Path, sources: &Sources) -> Self {
        let message = match error.as_kind() {
            CoreErrorKind::Js(error) => deno_runtime::fmt_errors::format_js_error(error, None),
            _ => format!("{error:#}"),
        };
        let kind = classify(&error);
        let location = match error.as_kind() {
            CoreErrorKind::Js(error) => error
                .message
                .as_deref()
                .and_then(|message| sources.error_location(message))
                .or_else(|| stack_location(error, sources, kind == ErrorKind::Permission)),
            _ => sources.error_location(&error.to_string()),
        };
        Self { kind, message: format!("{}: {message}", entry.display()), location }
    }
}

fn stack_location(error: &JsError, sources: &Sources, permission: bool) -> Option<SourceLocation> {
    if permission {
        let cause_location =
            error.cause.as_deref().and_then(|cause| stack_location(cause, sources, true)).or_else(
                || {
                    error.aggregated.as_ref()?.iter().find_map(|cause| {
                        has_permission_cause(cause)
                            .then(|| stack_location(cause, sources, true))
                            .flatten()
                    })
                },
            );
        if cause_location.is_some() {
            return cause_location;
        }
    }
    error.frames.iter().find_map(|frame| {
        let name = frame.file_name.as_deref()?;
        let path = deno_core::ModuleSpecifier::parse(name)
            .ok()
            .and_then(|url| url.to_file_path().ok())
            .or_else(|| Path::new(name).is_absolute().then(|| PathBuf::from(name)))?;
        let local = if permission {
            sources.is_local_source(&path)
        } else {
            sources.is_project_source(&path)
        };
        if !local {
            return None;
        }
        Some(SourceLocation {
            path,
            line: u32::try_from(frame.line_number?).ok()?.max(1),
            column: u32::try_from(frame.column_number?).ok()?.max(1),
        })
    })
}

fn classify(error: &CoreError) -> ErrorKind {
    let message = error.to_string();
    if message.contains("BENTO_RESOLUTION") {
        return ErrorKind::Resolution;
    }
    if message.contains("BENTO_TRANSFORM") {
        return ErrorKind::Transform;
    }
    if matches!(error.as_kind(), CoreErrorKind::Parse(_)) || message.contains("BENTO_PARSE") {
        return ErrorKind::Startup;
    }
    if matches!(error.get_class().as_ref(), "NotCapable" | "PermissionDenied") {
        return ErrorKind::Permission;
    }
    if let CoreErrorKind::Js(error) = error.as_kind() {
        if has_permission_cause(error) {
            return ErrorKind::Permission;
        }
        if error.name.as_deref() == Some("SyntaxError") {
            return ErrorKind::Startup;
        }
    }
    ErrorKind::Uncaught
}

fn has_permission_cause(error: &JsError) -> bool {
    matches!(error.name.as_deref(), Some("NotCapable" | "PermissionDenied"))
        || error.cause.as_deref().is_some_and(has_permission_cause)
        || error.aggregated.as_ref().is_some_and(|errors| errors.iter().any(has_permission_cause))
}
