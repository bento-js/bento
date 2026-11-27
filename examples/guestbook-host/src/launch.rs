//! What the host settles about a guest book instance before it opens the store or runs anything.

use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::path::{Path, PathBuf};

use clap::ValueEnum;
use thiserror::Error;

use bento_host::{EnvPattern, PathPattern, PermissionSet, Permissions, Scope, canonical_path};
use bento_runtime::{RuntimeConfig, package_lookup_paths};

use crate::module::guest_book_module;
use crate::store::GuestBookStore;

// `DATABASE_URL` is allowed because the application's environment schema declares it; the host
// never sets it, so the database location stays on the Rust side.
const ALLOWED_ENV: [&str; 3] = ["PORT", "NODE_ENV", "DATABASE_URL"];

/// The `NODE_ENV` the guest sees.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum Mode {
    /// Log requests and serve unminified assets
    Development,
    /// Serve minified, fingerprinted assets
    Production,
}

impl Mode {
    pub fn node_env(self) -> &'static str {
        match self {
            Mode::Development => "development",
            Mode::Production => "production",
        }
    }
}

#[derive(Debug, Error)]
pub enum LaunchError {
    #[error("cannot use the project at {}: {source}", path.display())]
    Project {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("cannot locate the database at {}: {source}", path.display())]
    Database {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error(
        "the database at {} is under {}, which the guest may read; keep the store outside the project and every node_modules directory beside or above it",
        database.display(),
        readable.display()
    )]
    DatabaseReadable { database: PathBuf, readable: PathBuf },
}

/// A located guest book project, the directories its guest may read, the store's location
/// outside them, and what the guest is told: the port and the mode.
#[derive(Clone, Debug)]
pub struct Launch {
    project: PathBuf,
    readable: BTreeSet<PathBuf>,
    database: PathBuf,
    port: u16,
    mode: Mode,
}

impl Launch {
    /// Resolves `project` to the canonical directory permissions are checked against and
    /// `database` to the path the store will open, refusing a database that would fall under a
    /// directory the guest may read.
    ///
    /// # Errors
    ///
    /// [`LaunchError::Project`] when the project directory or a package lookup path above it
    /// cannot be resolved, [`LaunchError::Database`] when `database` cannot be resolved, and
    /// [`LaunchError::DatabaseReadable`] when it resolves under a path the guest may read.
    pub fn prepare(
        project: &Path,
        database: &Path,
        port: u16,
        mode: Mode,
    ) -> Result<Launch, LaunchError> {
        let project_error = |source| LaunchError::Project { path: project.to_owned(), source };
        let project = std::fs::canonicalize(project).map_err(project_error)?;
        let readable = std::iter::once(project.clone())
            .chain(package_lookup_paths(&project).map_err(project_error)?)
            .collect();
        let database = database_outside(database, &readable)?;
        Ok(Launch { project, readable, database, port, mode })
    }

    pub fn project(&self) -> &Path {
        &self.project
    }

    /// Where the store lives: resolved like a permission path, so it names the file the store
    /// will actually open, and never under a directory the guest may read.
    pub fn database(&self) -> &Path {
        &self.database
    }

    /// The port the guest is told to listen on.
    pub fn port(&self) -> u16 {
        self.port
    }

    /// The configuration for one restricted instance: reading the project and Node's package
    /// lookup locations above it (`<ancestor>/node_modules`, which the application's package
    /// tooling probes, whether or not they exist yet), the network, the allowed environment
    /// names, native addons, and `host:guestbook` over `store`. It may write nothing; the host
    /// owns the only state.
    ///
    /// # Panics
    ///
    /// Never: every allowed environment name is a constant, valid variable name.
    pub fn runtime_config(&self, store: GuestBookStore) -> RuntimeConfig {
        let permissions = Permissions::restricted(PermissionSet {
            fs_read: Scope::only(self.readable.iter().map(|directory| beneath(directory))),
            env: Scope::only(
                ALLOWED_ENV.map(|name| EnvPattern::parse(name).expect("a variable name")),
            ),
            net: true,
            addons: true,
            ..PermissionSet::default()
        });
        RuntimeConfig {
            env: BTreeMap::from([
                ("PORT".to_owned(), self.port.to_string()),
                ("NODE_ENV".to_owned(), self.mode.node_env().to_owned()),
            ]),
            host_modules: vec![guest_book_module(store)],
            ..RuntimeConfig::new(self.project.clone(), permissions)
        }
    }
}

/// The canonical `directory` and everything beneath it, as `--allow-fs-read=<directory>/*`
/// covers it even before it exists.
fn beneath(directory: &Path) -> PathPattern {
    let mut prefix = directory.as_os_str().to_owned();
    prefix.push("/");
    PathPattern::Prefix(PathBuf::from(prefix))
}

/// The guest may read anything under `readable`, so the store it must not see has to resolve
/// elsewhere; both sides are compared canonical so a symlink cannot hide the placement.
fn database_outside(database: &Path, readable: &BTreeSet<PathBuf>) -> Result<PathBuf, LaunchError> {
    let database_error = |source| LaunchError::Database { path: database.to_owned(), source };
    let database = std::path::absolute(database)
        .and_then(|absolute| canonical_path(&absolute))
        .map_err(database_error)?;
    if let Some(readable) = readable.iter().find(|readable| database.starts_with(readable)) {
        return Err(LaunchError::DatabaseReadable { database, readable: readable.clone() });
    }
    Ok(database)
}
