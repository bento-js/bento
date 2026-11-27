use std::ffi::{OsStr, OsString};
use std::fmt;
use std::io;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::canonical_path;
use crate::scope::Pattern;

/// Paths a file system permission covers, matched as Node matches the values of
/// `--allow-fs-read` and `--allow-fs-write`, but at real locations (see
/// [`canonical_path`]).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PathPattern {
    /// Exactly this canonical path: a file, or a path that did not exist when the pattern was
    /// made.
    Exact(PathBuf),
    /// Every path whose text begins with `prefix`, as a value with `*` covers.
    Prefix {
        /// The text before the `*`. A prefix that ends in `/` also covers the directory it
        /// names, so an existing directory `/srv` is the prefix `/srv/`.
        prefix: PathBuf,
        /// The text a value had after its `*`. It covers nothing, but it is part of the value,
        /// so `process.permission.drop()` tells `dir/*` and `dir/*x` apart, as Node does.
        ignored: OsString,
    },
}

impl PathPattern {
    /// Every path whose text begins with `prefix`, with no text after the `*`.
    pub fn prefix(prefix: impl Into<PathBuf>) -> PathPattern {
        PathPattern::Prefix { prefix: prefix.into(), ignored: OsString::new() }
    }

    /// The pattern a command-line value names, resolved against `base` as `bento` resolves
    /// `--allow-fs-read=VALUE` against the directory it runs in.
    ///
    /// A value containing `*` covers every path that begins with the text before the first
    /// `*`; the text after it is kept but covers nothing, and the directory part of the text is
    /// resolved to its real location. Any other value is resolved to its real location, and
    /// covers everything beneath it when it is an existing directory, or else exactly itself.
    /// The value `*` alone is all of the permission ([`crate::Scope::All`]), not a pattern; here
    /// it would mean the paths beneath `base`.
    ///
    /// # Errors
    ///
    /// Whatever [`canonical_path`] reports for the resolved path.
    pub fn resolve(value: impl AsRef<OsStr>, base: &Path) -> io::Result<PathPattern> {
        let value = value.as_ref().as_bytes();
        match value.iter().position(|byte| *byte == b'*') {
            Some(wildcard) => wildcard_prefix(&value[..wildcard], &value[wildcard + 1..], base),
            None => {
                let path = canonical_path(&base.join(OsStr::from_bytes(value)))?;
                Ok(if path.is_dir() { directory(path) } else { PathPattern::Exact(path) })
            }
        }
    }

    /// Whether this pattern covers `path`, a canonical path.
    pub fn covers(&self, path: &Path) -> bool {
        let path = path.as_os_str().as_bytes();
        match self {
            PathPattern::Exact(exact) => path == exact.as_os_str().as_bytes(),
            PathPattern::Prefix { prefix, .. } => prefix_covers(prefix_bytes(prefix), path),
        }
    }
}

fn prefix_bytes(prefix: &Path) -> &[u8] {
    prefix.as_os_str().as_bytes()
}

/// Whether the prefix covers `path`: its text begins with the prefix, or it is the directory a
/// prefix ending in `/` names.
fn prefix_covers(prefix: &[u8], path: &[u8]) -> bool {
    path.starts_with(prefix) || prefix.strip_suffix(b"/").is_some_and(|named| path == named)
}

/// The prefix pattern for the text before a `*`: its directory part at its real location,
/// then the partial name after the directory's last `/`, kept as written, with `ignored`, the
/// text after the `*`.
fn wildcard_prefix(text: &[u8], ignored: &[u8], base: &Path) -> io::Result<PathPattern> {
    let split = text.iter().rposition(|byte| *byte == b'/').map_or(0, |slash| slash + 1);
    let (directory_part, partial_name) = text.split_at(split);
    let directory = canonical_path(&base.join(OsStr::from_bytes(directory_part)))?;
    let mut prefix = beneath(directory).into_vec();
    prefix.extend_from_slice(partial_name);
    Ok(PathPattern::Prefix {
        prefix: PathBuf::from(OsString::from_vec(prefix)),
        ignored: OsStr::from_bytes(ignored).to_owned(),
    })
}

/// The prefix pattern covering an existing directory and everything beneath it.
fn directory(path: PathBuf) -> PathPattern {
    PathPattern::prefix(beneath(path))
}

/// `directory` followed by exactly one `/`.
fn beneath(directory: PathBuf) -> OsString {
    let mut text = directory.into_os_string();
    if !text.as_bytes().ends_with(b"/") {
        text.push("/");
    }
    text
}

impl Pattern for PathPattern {
    type Target = Path;

    fn covers(&self, target: &Path) -> bool {
        PathPattern::covers(self, target)
    }

    fn within(&self, other: &PathPattern) -> bool {
        match (self, other) {
            (PathPattern::Exact(path), _) => other.covers(path),
            (PathPattern::Prefix { .. }, PathPattern::Exact(_)) => false,
            // Whatever `prefix` covers begins with it, so with `outer` too. The directory a
            // `prefix` ending in `/` names is at least as long as `outer` unless the two are
            // equal, so `outer` covers it too.
            (PathPattern::Prefix { prefix, .. }, PathPattern::Prefix { prefix: outer, .. }) => {
                prefix_bytes(prefix).starts_with(prefix_bytes(outer))
            }
        }
    }

    fn overlaps(&self, other: &PathPattern) -> bool {
        match (self, other) {
            (PathPattern::Exact(path), pattern) | (pattern, PathPattern::Exact(path)) => {
                pattern.covers(path)
            }
            (
                PathPattern::Prefix { prefix: one, .. },
                PathPattern::Prefix { prefix: other, .. },
            ) => {
                let (one, other) = (prefix_bytes(one), prefix_bytes(other));
                one.starts_with(other) || other.starts_with(one)
            }
        }
    }
}

impl fmt::Display for PathPattern {
    /// The pattern as a command-line value would name it: the path, or the prefix followed by
    /// `*` and any text written after it.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PathPattern::Exact(path) => write!(formatter, "{}", path.display()),
            PathPattern::Prefix { prefix, ignored } => {
                write!(formatter, "{}*{}", prefix.display(), ignored.display())
            }
        }
    }
}
