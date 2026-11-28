use std::fmt;

/// One kind of authority, named as Node's permission model names it. The last,
/// [`Permission::LanguageModel`], is Bento's own.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Permission {
    /// Reading files.
    FsRead,
    /// Creating, changing, and removing files.
    FsWrite,
    /// Connecting and listening on the network.
    Net,
    /// Seeing environment variables.
    Env,
    /// Starting child processes.
    ChildProcess,
    /// Starting worker threads.
    Worker,
    /// Loading native addons.
    Addons,
    /// Using WASI.
    Wasi,
    /// Using FFI.
    Ffi,
    /// Using the inspector.
    Inspector,
    /// Using OpenSSL STORE loaders.
    OpensslStore,
    /// Mounting a virtual file system.
    FsVfs,
    /// Using the local language model.
    LanguageModel,
}

impl Permission {
    /// Every permission, in the order Bento lists them.
    pub const ALL: [Permission; 13] = [
        Permission::FsRead,
        Permission::FsWrite,
        Permission::Net,
        Permission::Env,
        Permission::ChildProcess,
        Permission::Worker,
        Permission::Addons,
        Permission::Wasi,
        Permission::Ffi,
        Permission::Inspector,
        Permission::OpensslStore,
        Permission::FsVfs,
        Permission::LanguageModel,
    ];

    /// The command-line flag that allows it in a restricted launch, such as `--allow-fs-read`.
    pub fn flag(self) -> &'static str {
        match self {
            Permission::FsRead => "--allow-fs-read",
            Permission::FsWrite => "--allow-fs-write",
            Permission::Net => "--allow-net",
            Permission::Env => "--allow-env",
            Permission::ChildProcess => "--allow-child-process",
            Permission::Worker => "--allow-worker",
            Permission::Addons => "--allow-addons",
            Permission::Wasi => "--allow-wasi",
            Permission::Ffi => "--allow-ffi",
            Permission::Inspector => "--allow-inspector",
            Permission::OpensslStore => "--allow-openssl-store",
            Permission::FsVfs => "--allow-fs-vfs",
            Permission::LanguageModel => "--allow-language-model",
        }
    }

    /// The scope `process.permission.has()` knows it by. Mounting a virtual file system has
    /// none, as in Node.
    pub fn scope(self) -> Option<&'static str> {
        Some(match self {
            Permission::FsRead => "fs.read",
            Permission::FsWrite => "fs.write",
            Permission::Net => "net",
            Permission::Env => "env",
            Permission::ChildProcess => "child",
            Permission::Worker => "worker",
            Permission::Addons => "addon",
            Permission::Wasi => "wasi",
            Permission::Ffi => "ffi",
            Permission::Inspector => "inspector",
            Permission::OpensslStore => "openssl.store",
            Permission::FsVfs => return None,
            Permission::LanguageModel => "language-model",
        })
    }

    /// The permission `process.permission.has()` knows by `scope`. `fs`, which names both file
    /// system permissions, and unknown scopes have none.
    pub fn from_scope(scope: &str) -> Option<Permission> {
        Permission::ALL.into_iter().find(|permission| permission.scope() == Some(scope))
    }

    /// Whether the permission covers individual paths, which a reference can name.
    pub fn is_file_system(self) -> bool {
        matches!(self, Permission::FsRead | Permission::FsWrite)
    }
}

impl fmt::Display for Permission {
    /// The flag's name without `--allow-`, such as `fs-read`.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.flag()["--allow-".len()..])
    }
}
