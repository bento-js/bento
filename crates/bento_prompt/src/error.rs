//! Typed failures reported by this crate.

use std::fmt;

/// Category of a [`Error`].
///
/// Categories come from validation and from the model's typed failures; they
/// are never inferred from a localized message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ErrorKind {
    /// The requested capability, language, schema feature, or backend is not
    /// supported by this build or by the on-device model.
    NotSupported,
    /// The supplied messages or options are invalid.
    InvalidInput,
    /// The conversation would exceed the model's context window.
    QuotaExceeded,
    /// The model refused the request or filtered its output.
    NotReadable,
    /// The operation or its session was cancelled.
    Aborted,
    /// Creating or cloning a session failed for infrastructure reasons.
    Operation,
    /// A generation failed without a more specific category.
    Unknown,
}

impl ErrorKind {
    const fn as_str(self) -> &'static str {
        match self {
            Self::NotSupported => "not supported",
            Self::InvalidInput => "invalid input",
            Self::QuotaExceeded => "quota exceeded",
            Self::NotReadable => "not readable",
            Self::Aborted => "aborted",
            Self::Operation => "operation",
            Self::Unknown => "unknown",
        }
    }
}

impl fmt::Display for ErrorKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// A failure of a prompt-session operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    kind: ErrorKind,
    message: String,
}

impl Error {
    pub(crate) fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self { kind, message: message.into() }
    }

    pub(crate) fn for_initialization(mut self) -> Self {
        if self.kind == ErrorKind::Unknown {
            self.kind = ErrorKind::Operation;
        }
        self
    }

    /// The category of this failure.
    #[must_use]
    pub const fn kind(&self) -> ErrorKind {
        self.kind
    }

    /// A diagnostic description of this failure.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.kind, self.message)
    }
}

impl std::error::Error for Error {}
