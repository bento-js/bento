use std::fmt::Display;

use thiserror::Error;

use crate::scope::{Coverage, Pattern};
use crate::{Permission, PermissionSet, Permissions, Scope};

/// Inherited permissions text that is not an encoding of [`Permissions`].
#[derive(Debug, Error)]
#[error("invalid inherited permissions: {0}")]
pub struct InvalidCeiling(#[from] serde_json::Error);

/// A path in the permissions is not valid UTF-8, so they have no JSON encoding.
#[derive(Debug, Error)]
#[error("cannot encode the permissions as a ceiling: {0}")]
pub struct UnencodableCeiling(#[from] serde_json::Error);

/// The first part of a requested permission a ceiling does not hold.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Excess {
    /// The permission asked for.
    pub permission: Permission,
    /// The value asked for, as a command line writes it: `*` for all of the files or
    /// variables, or one path or variable pattern. `None` for a permission that is all or
    /// nothing.
    pub requested: Option<String>,
}

impl Permissions {
    /// Encodes these permissions for a nested launch to read back as its ceiling.
    ///
    /// # Errors
    ///
    /// [`UnencodableCeiling`] when a path is not valid UTF-8; JSON cannot carry such a path.
    pub fn to_ceiling_value(&self) -> Result<String, UnencodableCeiling> {
        Ok(serde_json::to_string(self)?)
    }

    /// Reads back permissions encoded by [`Permissions::to_ceiling_value`].
    ///
    /// # Errors
    ///
    /// [`InvalidCeiling`] when the text is not such an encoding.
    pub fn from_ceiling_value(value: &str) -> Result<Permissions, InvalidCeiling> {
        Ok(serde_json::from_str(value)?)
    }

    /// The first part of `requested`, the allowances of a restricted nested launch, that these
    /// permissions, its ceiling, do not hold; `None` when every part fits.
    pub fn excess(&self, requested: &PermissionSet) -> Option<Excess> {
        Permission::ALL.into_iter().find_map(|permission| self.excess_in(requested, permission))
    }

    /// How much of `permission` these permissions hold, for a diagnostic: `all`, `none`, or
    /// the allowed patterns and any denied ones.
    pub fn describe(&self, permission: Permission) -> String {
        match permission {
            Permission::FsRead | Permission::FsWrite => {
                describe_scope(self.allowed_paths(permission), self.denied_paths(permission))
            }
            Permission::Env => describe_scope(&self.allowed().env, &self.denied().env),
            flag => (if self.holds(flag) { "all" } else { "none" }).to_owned(),
        }
    }

    fn excess_in(&self, requested: &PermissionSet, permission: Permission) -> Option<Excess> {
        let (allowed, denied) = (self.allowed(), self.denied());
        let excess = |requested| Excess { permission, requested };
        match permission {
            Permission::FsRead | Permission::FsWrite => scope_excess(
                requested.paths(permission),
                allowed.paths(permission),
                denied.paths(permission),
            )
            .map(excess),
            Permission::Env => scope_excess(&requested.env, &allowed.env, &denied.env).map(excess),
            flag => (requested.whole(flag) && !self.holds(flag)).then(|| excess(None)),
        }
    }
}

/// The first pattern of `requested` that `allowed` does not contain or that overlaps `denied`,
/// written as a command line writes it.
fn scope_excess<T: Pattern + Display>(
    requested: &Scope<T>,
    allowed: &Scope<T>,
    denied: &Scope<T>,
) -> Option<Option<String>> {
    match requested {
        Scope::None => None,
        Scope::All => (!allowed.is_all() || !denied.is_none()).then(|| Some("*".to_owned())),
        Scope::Only(patterns) => patterns
            .iter()
            .find(|pattern| !allowed.contains(pattern) || denied.overlaps(pattern))
            .map(|pattern| Some(pattern.to_string())),
    }
}

fn describe_scope<T: Ord + Display>(allowed: &Scope<T>, denied: &Scope<T>) -> String {
    match (listed(allowed), listed(denied)) {
        (None, _) => "none".to_owned(),
        (Some(_), Some(_)) if denied.is_all() => "none".to_owned(),
        (Some(allowed), None) => allowed,
        (Some(allowed), Some(denied)) => format!("{allowed} except {denied}"),
    }
}

fn listed<T: Ord + Display>(scope: &Scope<T>) -> Option<String> {
    match scope {
        Scope::All => Some("all".to_owned()),
        Scope::None => None,
        Scope::Only(patterns) if patterns.is_empty() => None,
        Scope::Only(patterns) => {
            Some(patterns.iter().map(ToString::to_string).collect::<Vec<_>>().join(", "))
        }
    }
}
