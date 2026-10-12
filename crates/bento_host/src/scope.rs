use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::NetTarget;

/// How much of one kind of authority a runtime instance holds.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "scope", content = "items")]
pub enum Scope<T: Ord> {
    #[default]
    None,
    All,
    /// Only these items are authorized; an empty set grants nothing, like [`Scope::None`].
    Only(BTreeSet<T>),
}

/// A granted item, and whether it authorizes a requested operation target.
pub trait Covers {
    /// The borrowed form operations are checked against (`Path` for paths, `str` for names).
    type Target: ?Sized;

    fn covers(&self, requested: &Self::Target) -> bool;
    fn as_target(&self) -> &Self::Target;
    /// How the item reads in diagnostics.
    fn describe(&self) -> String;
}

impl Covers for PathBuf {
    type Target = Path;

    /// Component-wise containment: `/a` covers `/a/b` but not `/ab`.
    fn covers(&self, requested: &Path) -> bool {
        requested.starts_with(self)
    }

    fn as_target(&self) -> &Path {
        self
    }

    fn describe(&self) -> String {
        self.display().to_string()
    }
}

impl Covers for String {
    type Target = str;

    fn covers(&self, requested: &str) -> bool {
        self == requested
    }

    fn as_target(&self) -> &str {
        self
    }

    fn describe(&self) -> String {
        self.clone()
    }
}

impl Covers for NetTarget {
    type Target = NetTarget;

    /// A host without a port covers every port of that host. Hosts are compared in their
    /// canonical spelling.
    fn covers(&self, requested: &NetTarget) -> bool {
        self.host() == requested.host()
            && (self.port().is_none() || self.port() == requested.port())
    }

    fn as_target(&self) -> &NetTarget {
        self
    }

    fn describe(&self) -> String {
        self.to_string()
    }
}

impl<T: Ord> Scope<T> {
    /// A scope of exactly these items. An empty list grants nothing.
    pub fn only(items: impl IntoIterator<Item = T>) -> Self {
        let items: BTreeSet<T> = items.into_iter().collect();
        if items.is_empty() { Scope::None } else { Scope::Only(items) }
    }
}

impl<T: Ord + Covers> Scope<T> {
    pub fn allows(&self, requested: &T::Target) -> bool {
        match self {
            Scope::None => false,
            Scope::All => true,
            Scope::Only(granted) => granted.iter().any(|item| item.covers(requested)),
        }
    }

    /// The part of `requested` this scope does not authorize, if any.
    pub(crate) fn excess<'a>(&self, requested: &'a Scope<T>) -> Option<Excess<'a, T>> {
        match (requested, self) {
            (Scope::None, _) | (_, Scope::All) => None,
            (Scope::All, _) => Some(Excess::Everything),
            (Scope::Only(items), _) => {
                items.iter().find(|item| !self.allows(item.as_target())).map(Excess::Item)
            }
        }
    }
}

pub(crate) enum Excess<'a, T> {
    Everything,
    Item(&'a T),
}
