use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::scope::Coverage;
use crate::{EnvPattern, PathPattern, Permission, PermissionSet, Scope, is_always_kept};

/// What one runtime instance may do. A host chooses one of the two kinds explicitly; there is
/// no default.
///
/// - [`Permissions::unrestricted`] holds every permission, as Node does without
///   `--permission`; [`Permissions::unrestricted_denying`] takes some away again.
/// - [`Permissions::restricted`] holds only what it allows, as Node does with `--permission`.
///
/// `process.permission.drop()` takes permissions away from a running instance, so the
/// permissions an instance holds can shrink. They never grow.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Permissions {
    restricted: bool,
    allowed: PermissionSet,
    denied: PermissionSet,
}

impl Permissions {
    /// Every permission.
    pub fn unrestricted() -> Permissions {
        Permissions::unrestricted_denying(PermissionSet::default())
    }

    /// Every permission except what `denied` holds. A denial takes away exactly what the
    /// matching allowance of a restricted instance would give.
    pub fn unrestricted_denying(denied: PermissionSet) -> Permissions {
        Permissions { restricted: false, allowed: PermissionSet::everything(), denied }
    }

    /// Only what `allowed` holds.
    pub fn restricted(allowed: PermissionSet) -> Permissions {
        Permissions { restricted: true, allowed, denied: PermissionSet::default() }
    }

    /// These permissions, able to read what `pattern` covers besides, as Node lets the main
    /// thread read its entry script and a worker read its working directory. Only restricted
    /// permissions gain anything: unrestricted ones read every path they do not deny already.
    pub fn reading(mut self, pattern: PathPattern) -> Permissions {
        if !self.restricted {
            return self;
        }
        match &mut self.allowed.fs_read {
            Scope::All => {}
            Scope::None => self.allowed.fs_read = Scope::only([pattern]),
            Scope::Only(patterns) => {
                patterns.insert(pattern);
            }
        }
        self
    }

    /// Whether this instance holds only what it was allowed.
    pub fn is_restricted(&self) -> bool {
        self.restricted
    }

    /// Whether anything is withheld: always for a restricted instance, and for an unrestricted
    /// one with a denial or a dropped environment. Exactly these instances define
    /// `process.permission`.
    pub fn withholds_anything(&self) -> bool {
        self.restricted || !self.denied.is_empty() || self.allowed != PermissionSet::everything()
    }

    /// Whether this instance holds all of `permission`: what `process.permission.has()` answers
    /// without a reference. For files and the environment, only an allowance of `*` holds all,
    /// and any denial takes the whole away.
    pub fn holds(&self, permission: Permission) -> bool {
        self.allowed.whole(permission) && !self.denied.any(permission)
    }

    /// Whether this instance may use `permission` at all, if only for some paths or variables.
    pub fn holds_any(&self, permission: Permission) -> bool {
        self.allowed.any(permission) && !self.denied.whole(permission)
    }

    /// Whether this instance may read or write the canonical `path`, as `permission`, which
    /// must be [`Permission::FsRead`] or [`Permission::FsWrite`], says.
    pub fn holds_path(&self, permission: Permission, path: &Path) -> bool {
        self.allowed.paths(permission).covers(path) && !self.denied.paths(permission).covers(path)
    }

    /// Whether this instance's permissions cover the variable `name`. The variables Node always
    /// keeps are kept besides; see [`Permissions::has_variable`].
    pub fn holds_variable(&self, name: &str) -> bool {
        self.allowed.env.covers(name) && !self.denied.env.covers(name)
    }

    /// What `process.permission.has('env', name)` answers: whether the instance's permissions
    /// cover the variable, or it is one Node always keeps and was not dropped. With the whole
    /// environment taken away, only the always-kept variables answer `true`, as in Node.
    pub fn has_variable(&self, name: &str) -> bool {
        match &self.denied.env {
            Scope::All => is_always_kept(name),
            denied => {
                !denied.covers(name) && (self.allowed.env.covers(name) || is_always_kept(name))
            }
        }
    }

    /// Whether a variable of the environment an instance starts with is removed before guest
    /// code runs: it is neither covered nor one Node always keeps.
    pub fn removes_variable(&self, name: &str) -> bool {
        !is_always_kept(name) && !self.holds_variable(name)
    }

    /// Whether this instance's permissions deny the variable `name` outright: a denial of an
    /// unrestricted instance, or a drop, rather than the absence of an allowance.
    pub fn denies_variable(&self, name: &str) -> bool {
        self.denied.env.covers(name)
    }

    /// The paths this instance's permissions allow for `permission`, which must be
    /// [`Permission::FsRead`] or [`Permission::FsWrite`]. Paths are held when they are allowed
    /// and not denied.
    pub fn allowed_paths(&self, permission: Permission) -> &Scope<PathPattern> {
        self.allowed.paths(permission)
    }

    /// The paths this instance's permissions deny for `permission`, which must be
    /// [`Permission::FsRead`] or [`Permission::FsWrite`].
    pub fn denied_paths(&self, permission: Permission) -> &Scope<PathPattern> {
        self.denied.paths(permission)
    }

    /// The environment variables this instance's permissions allow. Variables are held when
    /// they are allowed and not denied.
    pub fn allowed_variables(&self) -> &Scope<EnvPattern> {
        &self.allowed.env
    }

    /// The environment variables this instance's permissions deny.
    pub fn denied_variables(&self) -> &Scope<EnvPattern> {
        &self.denied.env
    }

    /// Irreversibly removes all of `permission`, as `process.permission.drop(scope)` does.
    ///
    /// Dropping the environment also forgets the variables dropped one by one, as Node does:
    /// afterwards the always-kept variables answer `has()` again, and each can be dropped anew.
    pub fn drop_whole(&mut self, permission: Permission) {
        if permission == Permission::Env {
            self.allowed.env = Scope::None;
            self.denied.env = Scope::None;
        } else {
            self.denied.set_whole(permission, true);
        }
    }

    /// Irreversibly removes the one file system allowance given by exactly `pattern`, as
    /// `process.permission.drop(scope, reference)` does for `fs.read` and `fs.write`.
    /// `permission` must be [`Permission::FsRead`] or [`Permission::FsWrite`]. A pattern that
    /// matches no allowance changes nothing, and so does any pattern when all of the permission
    /// is allowed.
    pub fn drop_pattern(&mut self, permission: Permission, pattern: &PathPattern) {
        self.allowed.paths_mut(permission).remove(pattern);
    }

    /// Irreversibly removes access to the variable `name`, as
    /// `process.permission.drop('env', name)` does, even when Node would always keep it.
    /// `name` is a name, not a pattern: Node rejects a `*` in it, and here such a name changes
    /// nothing.
    pub fn drop_variable(&mut self, name: &str) {
        let Some(pattern) = EnvPattern::parse(name).ok().filter(|_| !name.contains('*')) else {
            return;
        };
        match &mut self.denied.env {
            Scope::All => {}
            Scope::None => self.denied.env = Scope::only([pattern]),
            Scope::Only(names) => {
                names.insert(pattern);
            }
        }
    }

    pub(crate) fn allowed(&self) -> &PermissionSet {
        &self.allowed
    }

    pub(crate) fn denied(&self) -> &PermissionSet {
        &self.denied
    }
}
