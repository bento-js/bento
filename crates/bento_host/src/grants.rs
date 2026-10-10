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
    pub read: Scope<PathBuf>,
    pub write: Scope<PathBuf>,
    pub net: Scope<NetTarget>,
    pub env: Scope<String>,
    /// Paths from which native Node-API addons may be loaded. An enabled addon runs
    /// native code in the host process, outside every other scope's containment.
    pub addons: Scope<PathBuf>,
}

/// Grant options a host requested for one launch. `None` for a kind means the
/// launch did not mention it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GrantRequest {
    pub read: Option<Scope<PathBuf>>,
    pub write: Option<Scope<PathBuf>>,
    pub net: Option<Scope<NetTarget>>,
    pub env: Option<Scope<String>>,
    pub addons: Option<Scope<PathBuf>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GrantKind {
    Read,
    Write,
    Net,
    Env,
    Addons,
}

/// A requested grant exceeded the policy inherited from the invoking host.
#[derive(Debug, Error, PartialEq, Eq)]
#[error("requested {kind} grant `{requested}` exceeds the inherited policy")]
pub struct WiderGrant {
    pub kind: GrantKind,
    pub requested: String,
}

#[derive(Debug, Error)]
#[error("invalid inherited grant policy: {0}")]
pub struct InvalidCeiling(#[from] serde_json::Error);

impl Grants {
    pub fn none() -> Self {
        Grants::default()
    }

    /// Encodes this policy for a nested launch to read back as its ceiling.
    pub fn to_ceiling_value(&self) -> String {
        serde_json::to_string(self).expect("grants always serialize")
    }

    pub fn from_ceiling_value(value: &str) -> Result<Self, InvalidCeiling> {
        Ok(serde_json::from_str(value)?)
    }
}

impl GrantRequest {
    /// The policy for this launch. Without a ceiling, unmentioned kinds are denied.
    /// Under a ceiling, unmentioned kinds inherit it and mentioned kinds must fit
    /// within it; a wider request fails instead of being silently clamped.
    pub fn resolve(self, ceiling: Option<&Grants>) -> Result<Grants, WiderGrant> {
        let Some(ceiling) = ceiling else {
            return Ok(Grants {
                read: self.read.unwrap_or_default(),
                write: self.write.unwrap_or_default(),
                net: self.net.unwrap_or_default(),
                env: self.env.unwrap_or_default(),
                addons: self.addons.unwrap_or_default(),
            });
        };
        Ok(Grants {
            read: narrow(GrantKind::Read, self.read, &ceiling.read)?,
            write: narrow(GrantKind::Write, self.write, &ceiling.write)?,
            net: narrow(GrantKind::Net, self.net, &ceiling.net)?,
            env: narrow(GrantKind::Env, self.env, &ceiling.env)?,
            addons: narrow(GrantKind::Addons, self.addons, &ceiling.addons)?,
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

impl fmt::Display for GrantKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            GrantKind::Read => "read",
            GrantKind::Write => "write",
            GrantKind::Net => "net",
            GrantKind::Env => "env",
            GrantKind::Addons => "addons",
        })
    }
}
