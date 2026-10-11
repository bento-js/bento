use std::fmt;
use std::path::PathBuf;

/// A `tsconfig.json` option that Bento handles differently from `tsc`.
///
/// Warnings name options that are ignored even though they would change `tsc`'s output;
/// errors name settings under which some files cannot execute at all.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub severity: Severity,
    /// The `tsconfig.json` that was loaded.
    pub path: PathBuf,
    /// The `compilerOptions` key, such as `paths`.
    pub option: String,
    pub message: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    Warning,
    Error,
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}: {}", self.path.display(), self.severity, self.message)
    }
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Severity::Warning => "warning",
            Severity::Error => "error",
        })
    }
}
