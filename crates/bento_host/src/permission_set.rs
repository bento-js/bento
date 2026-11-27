use serde::{Deserialize, Serialize};

use crate::{EnvPattern, PathPattern, Permission, Scope};

/// An amount of every permission: what a restricted instance is allowed, or what an
/// unrestricted one is denied. Each field is as specific as the Node flag for it: paths and
/// wildcard patterns for files, names and prefixes for the environment, and all or nothing for
/// everything else. The default holds nothing.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PermissionSet {
    /// Reading files.
    pub fs_read: Scope<PathPattern>,
    /// Creating, changing, and removing files.
    pub fs_write: Scope<PathPattern>,
    /// Seeing environment variables.
    pub env: Scope<EnvPattern>,
    /// Connecting and listening on the network.
    pub net: bool,
    /// Starting child processes, which run with the user's full authority.
    pub child_process: bool,
    /// Starting worker threads.
    pub worker: bool,
    /// Loading native addons, which run native code in the host process outside every
    /// permission.
    pub addons: bool,
    /// Using WASI.
    pub wasi: bool,
    /// Using FFI.
    pub ffi: bool,
    /// Using the inspector.
    pub inspector: bool,
    /// Using OpenSSL STORE loaders.
    pub openssl_store: bool,
    /// Mounting a virtual file system.
    pub fs_vfs: bool,
    /// Using the local language model.
    pub language_model: bool,
}

impl PermissionSet {
    /// All of every permission.
    pub fn everything() -> PermissionSet {
        PermissionSet {
            fs_read: Scope::All,
            fs_write: Scope::All,
            env: Scope::All,
            net: true,
            child_process: true,
            worker: true,
            addons: true,
            wasi: true,
            ffi: true,
            inspector: true,
            openssl_store: true,
            fs_vfs: true,
            language_model: true,
        }
    }

    /// Whether this set holds none of any permission.
    pub fn is_empty(&self) -> bool {
        Permission::ALL.into_iter().all(|permission| !self.any(permission))
    }

    /// Whether this set holds all of `permission`.
    pub fn whole(&self, permission: Permission) -> bool {
        match permission {
            Permission::FsRead => self.fs_read.is_all(),
            Permission::FsWrite => self.fs_write.is_all(),
            Permission::Env => self.env.is_all(),
            flag => *self.flag(flag),
        }
    }

    /// Whether this set holds any of `permission`.
    pub fn any(&self, permission: Permission) -> bool {
        match permission {
            Permission::FsRead => !self.fs_read.is_none(),
            Permission::FsWrite => !self.fs_write.is_none(),
            Permission::Env => !self.env.is_none(),
            flag => *self.flag(flag),
        }
    }

    /// The file system scope for `permission`, which must be [`Permission::FsRead`] or
    /// [`Permission::FsWrite`].
    pub(crate) fn paths(&self, permission: Permission) -> &Scope<PathPattern> {
        match permission {
            Permission::FsWrite => &self.fs_write,
            _ => &self.fs_read,
        }
    }

    pub(crate) fn paths_mut(&mut self, permission: Permission) -> &mut Scope<PathPattern> {
        match permission {
            Permission::FsWrite => &mut self.fs_write,
            _ => &mut self.fs_read,
        }
    }

    /// Makes this set hold all of `permission`, or none of it.
    pub(crate) fn set_whole(&mut self, permission: Permission, whole: bool) {
        fn scope<T: Ord>(whole: bool) -> Scope<T> {
            if whole { Scope::All } else { Scope::None }
        }
        match permission {
            Permission::FsRead => self.fs_read = scope(whole),
            Permission::FsWrite => self.fs_write = scope(whole),
            Permission::Env => self.env = scope(whole),
            flag => *self.flag_mut(flag) = whole,
        }
    }

    fn flag(&self, permission: Permission) -> &bool {
        match permission {
            Permission::Net => &self.net,
            Permission::ChildProcess => &self.child_process,
            Permission::Worker => &self.worker,
            Permission::Addons => &self.addons,
            Permission::Wasi => &self.wasi,
            Permission::Ffi => &self.ffi,
            Permission::Inspector => &self.inspector,
            Permission::OpensslStore => &self.openssl_store,
            Permission::FsVfs => &self.fs_vfs,
            Permission::LanguageModel => &self.language_model,
            Permission::FsRead | Permission::FsWrite | Permission::Env => {
                unreachable!("{permission:?} has a scope, not a flag")
            }
        }
    }

    fn flag_mut(&mut self, permission: Permission) -> &mut bool {
        match permission {
            Permission::Net => &mut self.net,
            Permission::ChildProcess => &mut self.child_process,
            Permission::Worker => &mut self.worker,
            Permission::Addons => &mut self.addons,
            Permission::Wasi => &mut self.wasi,
            Permission::Ffi => &mut self.ffi,
            Permission::Inspector => &mut self.inspector,
            Permission::OpensslStore => &mut self.openssl_store,
            Permission::FsVfs => &mut self.fs_vfs,
            Permission::LanguageModel => &mut self.language_model,
            Permission::FsRead | Permission::FsWrite | Permission::Env => {
                unreachable!("{permission:?} has a scope, not a flag")
            }
        }
    }
}
