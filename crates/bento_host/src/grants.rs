use std::fmt;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::NetTarget;
use crate::scope::{Covers, Excess, Scope};

/// The authority one runtime instance holds. Anything not granted is denied; guest
/// subprocesses and remote module imports are never grantable.
///
/// Path items are absolute and canonical (see [`crate::canonical_path`]); builders of
/// grants canonicalize before constructing them so symlinks cannot widen a scope.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Grants {
    /// Files the guest may read; a granted directory covers everything beneath it.
    pub read: Scope<PathBuf>,
    /// Files the guest may create or modify; a granted directory covers everything beneath it.
    pub write: Scope<PathBuf>,
    /// Endpoints the guest may connect to or listen on; an item without a port covers every
    /// port of that host.
    pub net: Scope<NetTarget>,
    /// Environment variable names the guest may read or set; names are matched exactly.
    pub env: Scope<String>,
    /// Paths from which native Node-API addons may be loaded. An enabled addon runs
    /// native code in the host process, outside every other scope's containment.
    pub addons: Scope<PathBuf>,
    /// Whether the guest may use the host's local language model; the capability is the whole
    /// grant and names no model.
    #[serde(default)]
    pub language_model: bool,
}

/// Grant options a host requested for one launch. `None` for a kind means the
/// launch did not mention it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GrantRequest {
    /// Requested [`Grants::read`] scope.
    pub read: Option<Scope<PathBuf>>,
    /// Requested [`Grants::write`] scope.
    pub write: Option<Scope<PathBuf>>,
    /// Requested [`Grants::net`] scope.
    pub net: Option<Scope<NetTarget>>,
    /// Requested [`Grants::env`] scope.
    pub env: Option<Scope<String>>,
    /// Requested [`Grants::addons`] scope.
    pub addons: Option<Scope<PathBuf>>,
    /// Requested [`Grants::language_model`] capability.
    pub language_model: Option<bool>,
}

/// One kind of authority, named as grant diagnostics and CLI flags spell it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GrantKind {
    /// Reading files.
    Read,
    /// Creating or modifying files.
    Write,
    /// Outbound connections and listeners.
    Net,
    /// Reading and setting environment variables.
    Env,
    /// Loading native Node-API addons.
    Addons,
    /// Using the local language model.
    LanguageModel,
}

/// A requested grant exceeded the policy inherited from the invoking host.
#[derive(Debug, Error, PartialEq, Eq)]
#[error("requested {kind} grant `{requested}` exceeds the inherited policy")]
pub struct WiderGrant {
    /// The kind of authority that was over-requested.
    pub kind: GrantKind,
    /// The offending item as it reads in diagnostics, or `<all>` when the request asked for
    /// unrestricted authority.
    pub requested: String,
}

/// Inherited grant policy text that is not a policy encoding.
#[derive(Debug, Error)]
#[error("invalid inherited grant policy: {0}")]
pub struct InvalidCeiling(#[from] serde_json::Error);

/// A granted path is not valid UTF-8, so the policy has no JSON encoding.
#[derive(Debug, Error)]
#[error("cannot encode the grant policy as a ceiling: {0}")]
pub struct UnencodableCeiling(#[from] serde_json::Error);

impl Grants {
    /// Denies every kind of authority.
    pub fn none() -> Self {
        Grants::default()
    }

    /// Encodes this policy for a nested launch to read back as its ceiling.
    ///
    /// # Errors
    ///
    /// Fails when a granted path is not valid UTF-8; JSON cannot carry such a path.
    pub fn to_ceiling_value(&self) -> Result<String, UnencodableCeiling> {
        Ok(serde_json::to_string(self)?)
    }

    /// Reads back a policy encoded by [`Grants::to_ceiling_value`].
    ///
    /// # Errors
    ///
    /// [`InvalidCeiling`] when the text is not such an encoding.
    pub fn from_ceiling_value(value: &str) -> Result<Self, InvalidCeiling> {
        Ok(serde_json::from_str(value)?)
    }
}

impl GrantRequest {
    /// The policy for this launch. Without a ceiling, unmentioned kinds are denied.
    /// Under a ceiling, unmentioned kinds inherit it and mentioned kinds must fit
    /// within it; a wider request fails instead of being silently clamped.
    ///
    /// # Errors
    ///
    /// [`WiderGrant`] naming the first kind and item that the ceiling does not cover.
    pub fn resolve(self, ceiling: Option<&Grants>) -> Result<Grants, WiderGrant> {
        let Some(ceiling) = ceiling else {
            return Ok(Grants {
                read: self.read.unwrap_or_default(),
                write: self.write.unwrap_or_default(),
                net: self.net.unwrap_or_default(),
                env: self.env.unwrap_or_default(),
                addons: self.addons.unwrap_or_default(),
                language_model: self.language_model.unwrap_or_default(),
            });
        };
        Ok(Grants {
            read: narrow(GrantKind::Read, self.read, &ceiling.read)?,
            write: narrow(GrantKind::Write, self.write, &ceiling.write)?,
            net: narrow(GrantKind::Net, self.net, &ceiling.net)?,
            env: narrow(GrantKind::Env, self.env, &ceiling.env)?,
            addons: narrow(GrantKind::Addons, self.addons, &ceiling.addons)?,
            language_model: capability(
                GrantKind::LanguageModel,
                self.language_model,
                ceiling.language_model,
            )?,
        })
    }
}

fn narrow<T>(
    kind: GrantKind,
    requested: Option<Scope<T>>,
    ceiling: &Scope<T>,
) -> Result<Scope<T>, WiderGrant>
where
    T: Ord + Clone + Covers,
{
    let Some(requested) = requested else {
        return Ok(ceiling.clone());
    };
    let excess = ceiling.excess(&requested).map(|excess| match excess {
        Excess::Everything => "<all>".to_string(),
        Excess::Item(item) => item.describe(),
    });
    match excess {
        Some(requested) => Err(WiderGrant { kind, requested }),
        None => Ok(requested),
    }
}

/// Like [`narrow`] for a capability a launch either holds or does not: requesting it where the
/// ceiling denies it is the boolean form of asking for `<all>`.
fn capability(kind: GrantKind, requested: Option<bool>, ceiling: bool) -> Result<bool, WiderGrant> {
    match requested {
        None => Ok(ceiling),
        Some(true) if !ceiling => Err(WiderGrant { kind, requested: "<all>".to_owned() }),
        Some(requested) => Ok(requested),
    }
}

impl fmt::Display for GrantKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            GrantKind::Read => "read",
            GrantKind::Write => "write",
            GrantKind::Net => "net",
            GrantKind::Env => "env",
            GrantKind::Addons => "addons",
            GrantKind::LanguageModel => "language-model",
        })
    }
}
