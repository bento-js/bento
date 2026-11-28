use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

/// How much of a permission that names individual paths or variables a set of permissions
/// holds.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "scope", content = "items")]
pub enum Scope<T: Ord> {
    /// Nothing; the default.
    #[default]
    None,
    /// Everything, including paths and variables that appear later; Node's `*`.
    All,
    /// What these patterns cover. An empty set covers nothing, like [`Scope::None`].
    Only(BTreeSet<T>),
}

/// A pattern a [`Scope`] holds, and the targets it covers.
pub(crate) trait Pattern: Ord {
    /// What an operation names: a path or a variable name.
    type Target: ?Sized;

    fn covers(&self, target: &Self::Target) -> bool;
    /// Whether every target this pattern covers, `other` covers too.
    fn within(&self, other: &Self) -> bool;
    /// Whether some target is covered by both.
    fn overlaps(&self, other: &Self) -> bool;
}

impl<T: Ord> Scope<T> {
    /// A scope of exactly these patterns. An empty list covers nothing.
    pub fn only(items: impl IntoIterator<Item = T>) -> Self {
        let items: BTreeSet<T> = items.into_iter().collect();
        if items.is_empty() { Scope::None } else { Scope::Only(items) }
    }

    /// Whether this is all of the permission.
    pub fn is_all(&self) -> bool {
        matches!(self, Scope::All)
    }

    /// Whether this covers nothing.
    pub fn is_none(&self) -> bool {
        match self {
            Scope::None => true,
            Scope::All => false,
            Scope::Only(items) => items.is_empty(),
        }
    }

    /// Removes exactly `pattern`, if this scope holds it. All of a permission has no patterns
    /// to remove and stays whole.
    pub(crate) fn remove(&mut self, pattern: &T) {
        if let Scope::Only(items) = self {
            items.remove(pattern);
        }
    }
}

/// What a scope of patterns covers.
pub(crate) trait Coverage<T: Pattern> {
    fn covers(&self, target: &T::Target) -> bool;
    /// Whether every target `pattern` covers, this scope covers too.
    fn contains(&self, pattern: &T) -> bool;
    /// Whether this scope covers some target `pattern` covers.
    fn overlaps(&self, pattern: &T) -> bool;
}

impl<T: Pattern> Coverage<T> for Scope<T> {
    fn covers(&self, target: &T::Target) -> bool {
        match self {
            Scope::None => false,
            Scope::All => true,
            Scope::Only(items) => items.iter().any(|item| item.covers(target)),
        }
    }

    fn contains(&self, pattern: &T) -> bool {
        match self {
            Scope::None => false,
            Scope::All => true,
            Scope::Only(items) => items.iter().any(|item| pattern.within(item)),
        }
    }

    fn overlaps(&self, pattern: &T) -> bool {
        match self {
            Scope::None => false,
            Scope::All => true,
            Scope::Only(items) => items.iter().any(|item| pattern.overlaps(item)),
        }
    }
}
