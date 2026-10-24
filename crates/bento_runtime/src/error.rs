use std::path::{Path, PathBuf};

use deno_core::error::{CoreError, CoreErrorKind, JsError};
use deno_error::JsErrorClass;
use thiserror::Error;

use crate::sources::Sources;

/// Which stage of a run an error belongs to, for hosts that report the stages differently.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorKind {
    /// The instance could not be set up (paths, host modules, worker setup), or a CommonJS
    /// module could not be compiled, which may happen after guest code has run.
    Startup,
    /// An import named a module that could not be found.
    Resolution,
    /// A module's TypeScript or JSX could not be transformed, or the project's
    /// `tsconfig.json` could not be loaded.
    Transform,
    /// The guest attempted an operation outside its grants.
    Permission,
    /// The guest threw and nothing handled it.
    Uncaught,
}

/// A failure that ended, or prevented, one run.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
#[error("{message}")]
pub struct RuntimeError {
    /// The stage this failure belongs to.
    pub kind: ErrorKind,
    /// Ready to show: terminal escapes are stripped, and a guest failure is prefixed with
    /// the entry path and carries the formatted stack.
    pub message: String,
    location: Option<SourceLocation>,
}

/// Where in the project's own sources a failure happened.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceLocation {
    /// Absolute path of the file the failure is attributed to: a project source, except for
    /// a permission failure, which may be placed in a dependency.
    pub path: PathBuf,
    /// One-based line.
    pub line: u32,
    /// One-based column.
    pub column: u32,
}

/// Failure to start an instance, which is always a [`RuntimeError`].
pub type StartError = RuntimeError;

/// How one run ended.
#[derive(Debug)]
pub enum RunOutcome {
    /// The guest's event loop drained with nothing left to do.
    Completed,
    /// The guest exited with this code.
    Exited(i32),
    /// The instance failed before or during execution.
    Failed(RuntimeError),
    /// A host-requested shutdown ended the run; the report says how far it got.
    Shutdown(crate::ShutdownReport),
}

impl RuntimeError {
    /// Where this failure happened, when it can be placed in the project's own sources.
    pub fn location(&self) -> Option<&SourceLocation> {
        self.location.as_ref()
    }

    pub(crate) fn transform(message: impl Into<String>, location: Option<SourceLocation>) -> Self {
        Self { kind: ErrorKind::Transform, message: plain_message(message.into()), location }
    }

    pub(crate) fn startup(message: impl Into<String>) -> Self {
        Self { kind: ErrorKind::Startup, message: plain_message(message.into()), location: None }
    }

    pub(crate) fn unsupported(message: impl Into<String>) -> Self {
        Self { kind: ErrorKind::Uncaught, message: plain_message(message.into()), location: None }
    }

    pub(crate) fn guest(error: CoreError, entry: &Path, sources: &Sources) -> Self {
        let failure = match error.as_kind() {
            CoreErrorKind::Js(error) => error.message.as_deref().and_then(|message| {
                sources.error_failure(error.name.as_deref().unwrap_or("Error"), message)
            }),
            _ => sources.error_failure(&error.get_class(), &error.get_message()),
        };
        let kind = failure.as_ref().map_or_else(|| classify(&error), |failure| failure.kind);
        let location = failure.and_then(|failure| failure.location).or_else(|| {
            if let CoreErrorKind::Js(error) = error.as_kind() {
                stack_location(error, sources, kind == ErrorKind::Permission)
            } else {
                None
            }
        });
        let message = match error.as_kind() {
            CoreErrorKind::Js(error) => deno_runtime::fmt_errors::format_js_error(error, None),
            _ => format!("{error:#}"),
        };
        let mut message = plain_message(format!("{}: {message}", entry.display()));
        if kind == ErrorKind::Permission && message.contains("--allow-ffi") {
            message = message.replace("Requires ffi access", "Requires addons access").replace(
                "run again with the --allow-ffi flag",
                "the host must grant addons access (--allow-addons for the Bento CLI)",
            );
        }
        Self { kind, message, location }
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
    if matches!(error.as_kind(), CoreErrorKind::Parse(_)) {
        return ErrorKind::Startup;
    }
    if matches!(error.get_class().as_ref(), "NotCapable" | "PermissionDenied") {
        return ErrorKind::Permission;
    }
    if let CoreErrorKind::Js(error) = error.as_kind()
        && has_permission_cause(error)
    {
        return ErrorKind::Permission;
    }
    if let CoreErrorKind::Js(error) = error.as_kind()
        && error.name.as_deref() == Some("SyntaxError")
        && let [compiler, wrapper, ..] = error.frames.as_slice()
        && compiler.file_name.as_deref() == Some("ext:core/01_core.js")
        && matches!(compiler.function_name.as_deref(), Some("compileFunction" | "evalContext"))
        && wrapper.file_name.as_deref() == Some("node:module")
        && wrapper.function_name.as_deref() == Some("wrapSafe")
    {
        return ErrorKind::Startup;
    }
    ErrorKind::Uncaught
}

fn has_permission_cause(error: &JsError) -> bool {
    matches!(error.name.as_deref(), Some("NotCapable" | "PermissionDenied"))
        || error.cause.as_deref().is_some_and(has_permission_cause)
        || error.aggregated.as_ref().is_some_and(|errors| errors.iter().any(has_permission_cause))
}

fn plain_message(message: String) -> String {
    let Some(first_escape) = message.find('\u{1b}') else {
        return message;
    };
    let mut plain = String::with_capacity(message.len());
    plain.push_str(&message[..first_escape]);
    let mut remaining = &message[first_escape..];
    while let Some(offset) = remaining.find('\u{1b}') {
        plain.push_str(&remaining[..offset]);
        remaining = &remaining[offset + 1..];
        remaining = &remaining[escape_length(remaining)..];
    }
    plain.push_str(remaining);
    plain
}

fn escape_length(sequence: &str) -> usize {
    let bytes = sequence.as_bytes();
    match bytes.first() {
        Some(b'[') => {
            let end = bytes.iter().skip(1).position(|byte| !(0x20..=0x3f).contains(byte));
            end.filter(|end| (0x40..=0x7e).contains(&bytes[end + 1])).map_or(1, |end| end + 2)
        }
        Some(b']' | b'P' | b'X' | b'^' | b'_') => {
            for (index, byte) in bytes.iter().enumerate().skip(1) {
                if matches!(*byte, b'\r' | b'\n') {
                    return 1;
                }
                if *byte == b'\x07' && bytes[0] == b']' {
                    return index + 1;
                }
                if *byte == b'\x1b' && bytes.get(index + 1) == Some(&b'\\') {
                    return index + 2;
                }
                if *byte == b'\x1b' {
                    return 1;
                }
            }
            1
        }
        Some(0x20..=0x2f) => {
            let end = bytes.iter().position(|byte| !(0x20..=0x2f).contains(byte));
            end.filter(|end| (0x30..=0x7e).contains(&bytes[*end])).map_or(0, |end| end + 1)
        }
        Some(0x30..=0x7e) => 1,
        _ => 0,
    }
}
