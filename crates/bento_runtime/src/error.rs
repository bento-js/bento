use std::path::Path;

use deno_core::error::{CoreError, CoreErrorKind, JsError};
use deno_error::JsErrorClass;
use thiserror::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorKind {
    Startup,
    Resolution,
    Transform,
    Permission,
    Uncaught,
}

#[derive(Debug, Error)]
#[error("{message}")]
pub struct RuntimeError {
    pub kind: ErrorKind,
    pub message: String,
}

pub type StartError = RuntimeError;

#[derive(Debug)]
pub enum RunOutcome {
    Completed,
    Exited(i32),
    Failed(RuntimeError),
}

impl RuntimeError {
    pub(crate) fn startup(message: impl Into<String>) -> Self {
        Self { kind: ErrorKind::Startup, message: message.into() }
    }

    pub(crate) fn guest(error: CoreError, entry: &Path) -> Self {
        let message = match error.as_kind() {
            CoreErrorKind::Js(error) => deno_runtime::fmt_errors::format_js_error(error, None),
            _ => format!("{error:#}"),
        };
        let kind = classify(&error);
        Self { kind, message: format!("{}: {message}", entry.display()) }
    }
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
