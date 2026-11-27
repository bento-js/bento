use std::fmt;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::scope::Pattern;

/// Environment variables a permission covers: one name, or every name beginning with a
/// prefix, written as the prefix followed by `*` (`APP_*`). These are Node's `--allow-env`
/// values, apart from `*` alone, which is all of the permission ([`crate::Scope::All`]).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct EnvPattern(String);

/// Text that is neither a variable name nor a name prefix followed by `*`.
#[derive(Debug, Error, PartialEq, Eq)]
#[error("`{0}` is not a variable name or a variable name prefix followed by `*`")]
pub struct InvalidEnvPattern(String);

impl EnvPattern {
    /// The pattern `text` writes: a non-empty name without `=`, optionally ending in one `*`.
    ///
    /// # Errors
    ///
    /// [`InvalidEnvPattern`] for an empty text, `*` alone, a `=`, or a `*` before the end.
    pub fn parse(text: &str) -> Result<EnvPattern, InvalidEnvPattern> {
        let name = text.strip_suffix('*').unwrap_or(text);
        if name.is_empty() || name.contains(['=', '*', '\0']) {
            return Err(InvalidEnvPattern(text.to_owned()));
        }
        Ok(EnvPattern(text.to_owned()))
    }

    /// Whether this pattern covers the variable `name`.
    pub fn matches(&self, name: &str) -> bool {
        match self.prefix() {
            Some(prefix) => name.starts_with(prefix),
            None => name == self.0,
        }
    }

    /// The prefix, when this pattern ends in `*`.
    fn prefix(&self) -> Option<&str> {
        self.0.strip_suffix('*')
    }
}

impl Pattern for EnvPattern {
    type Target = str;

    fn covers(&self, name: &str) -> bool {
        self.matches(name)
    }

    fn within(&self, other: &EnvPattern) -> bool {
        match (self.prefix(), other.prefix()) {
            (None, _) => other.matches(&self.0),
            (Some(_), None) => false,
            (Some(prefix), Some(outer)) => prefix.starts_with(outer),
        }
    }

    fn overlaps(&self, other: &EnvPattern) -> bool {
        match (self.prefix(), other.prefix()) {
            (None, _) => other.matches(&self.0),
            (Some(_), None) => self.matches(&other.0),
            (Some(one), Some(other)) => one.starts_with(other) || other.starts_with(one),
        }
    }
}

impl fmt::Display for EnvPattern {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl TryFrom<String> for EnvPattern {
    type Error = InvalidEnvPattern;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        EnvPattern::parse(&text)
    }
}

impl From<EnvPattern> for String {
    fn from(pattern: EnvPattern) -> Self {
        pattern.0
    }
}

/// Variables a restricted instance keeps whatever its permissions allow: the ones Node and
/// its dependencies read themselves. This is the list in Node v27.0.0-alpha.0.2.0's
/// `src/permission/env_permission.cc`; `LC_*` stands for every name beginning `LC_`.
const ALWAYS_KEPT: &[&str] = &[
    // Read by Node.js itself after startup.
    "NODE_BENCH_CONTEXT",
    "NODE_BENCH_FILE_RUN_ID",
    "NODE_BENCH_RUN_ID",
    "NODE_CHANNEL_FD",
    "NODE_CHANNEL_SERIALIZATION_MODE",
    "NODE_CLUSTER_SCHED_POLICY",
    "NODE_COMPILE_CACHE",
    "NODE_COMPILE_CACHE_PORTABLE",
    "NODE_COMPILE_CACHE_READONLY",
    "NODE_DEBUG",
    "NODE_DEBUG_NATIVE",
    "NODE_DISABLE_COLORS",
    "NODE_DISABLE_COMPILE_CACHE",
    "NODE_EXTRA_CA_CERTS",
    "NODE_ICU_DATA",
    "NODE_INSPECT_RESUME_ON_START",
    "NODE_NO_WARNINGS",
    "NODE_OPTIONS",
    "NODE_PATH",
    "NODE_PENDING_DEPRECATION",
    "NODE_PENDING_PIPE_INSTANCES",
    "NODE_PRESERVE_SYMLINKS",
    "NODE_REDIRECT_WARNINGS",
    "NODE_REPL_EXTERNAL_MODULE",
    "NODE_REPL_HISTORY",
    "NODE_TEST_CONTEXT",
    "NODE_TEST_WORKER_ID",
    "NODE_TLS_REJECT_UNAUTHORIZED",
    "NODE_UNIQUE_ID",
    "NODE_USE_ENV_PROXY",
    "NODE_USE_SYSTEM_CA",
    "NODE_V8_COVERAGE",
    "WATCH_REPORT_DEPENDENCIES",
    // Terminal and CI detection.
    "APPVEYOR",
    "BUILDKITE",
    "CI",
    "CI_NAME",
    "CIRCLECI",
    "COLORTERM",
    "DRONE",
    "FORCE_COLOR",
    "GITEA_ACTIONS",
    "GITHUB_ACTIONS",
    "GITLAB_CI",
    "NO_COLOR",
    "TEAMCITY_VERSION",
    "TERM",
    "TERM_PROGRAM",
    "TERM_PROGRAM_VERSION",
    "TMUX",
    "TRAVIS",
    // libuv.
    "HOME",
    "PATH",
    "TEMP",
    "TMP",
    "TMPDIR",
    "USERPROFILE",
    "UV_THREADPOOL_SIZE",
    "UV_USE_IO_URING",
    // Windows process environments.
    "COMSPEC",
    "HOMEDRIVE",
    "HOMEPATH",
    "LOGONSERVER",
    "SYSTEMDRIVE",
    "SYSTEMROOT",
    "USERDOMAIN",
    "USERNAME",
    "WINDIR",
    // c-ares and the system resolver.
    "HOSTALIASES",
    "LOCALDOMAIN",
    "RES_OPTIONS",
    // ICU, the time zone, and the locale.
    "ICU_DATA",
    "LANG",
    "LANGUAGE",
    "LC_*",
    "TZ",
    // OpenSSL.
    "OPENSSL_CONF",
    "OPENSSL_CONF_INCLUDE",
    "OPENSSL_ENGINES",
    "OPENSSL_MODULES",
    "SSL_CERT_DIR",
    "SSL_CERT_FILE",
    // The dynamic loader.
    "DYLD_FALLBACK_LIBRARY_PATH",
    "DYLD_LIBRARY_PATH",
    "LD_LIBRARY_PATH",
    "LIBPATH",
];

/// Whether `name` is on Node's list of variables a restricted instance always keeps, such as
/// `PATH`, `HOME`, `NODE_OPTIONS`, and the `LC_*` locale variables.
pub fn is_always_kept(name: &str) -> bool {
    ALWAYS_KEPT.iter().any(|kept| match kept.strip_suffix('*') {
        Some(prefix) => name.starts_with(prefix),
        None => name == *kept,
    })
}
