//! What the host settles about a guest book instance before it opens the store or runs anything.

use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::path::{Path, PathBuf};

use clap::ValueEnum;
use thiserror::Error;

use bento_host::{Grants, Scope, canonical_path};
use bento_runtime::RuntimeConfig;

use crate::module::guest_book_module;
use crate::store::GuestBookStore;

// `DATABASE_URL` is granted because the application's environment schema declares it; the host
// never sets it, so the database location stays on the Rust side.
const GRANTED_ENV: [&str; 3] = ["PORT", "NODE_ENV", "DATABASE_URL"];

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
        "the database at {} is inside the project at {}, which the guest may read; keep the store outside the project",
        database.display(),
        project.display()
    )]
    DatabaseInsideProject { database: PathBuf, project: PathBuf },
}

/// A located guest book project, the store's location outside it, and what its guest is told:
/// the port and the mode.
#[derive(Clone, Debug)]
pub struct Launch {
    project: PathBuf,
    node_modules: PathBuf,
    database: PathBuf,
    port: u16,
    mode: Mode,
}

impl Launch {
    /// Resolves `project` to the canonical directory grants are checked against and `database`
    /// to the path the store will open, refusing a database that would fall under the guest's
    /// read grant.
    pub fn prepare(
        project: &Path,
        database: &Path,
        port: u16,
        mode: Mode,
    ) -> Result<Launch, LaunchError> {
        let project_error = |source| LaunchError::Project { path: project.to_owned(), source };
        let project = std::fs::canonicalize(project).map_err(project_error)?;
        let node_modules = canonical_path(&project.join("node_modules")).map_err(project_error)?;
        let database = database_outside(database, &project)?;
        Ok(Launch { project, node_modules, database, port, mode })
    }

    pub fn project(&self) -> &Path {
        &self.project
    }

    /// Where the store lives: resolved like a grant path, so it names the file the store will
    /// actually open, and never inside the project.
    pub fn database(&self) -> &Path {
        &self.database
    }

    /// The configuration for one instance: read access to the project, network access, the
    /// granted environment names, native addons under the project's `node_modules`, and
    /// `host:guestbook` over `store`. There is no write grant; the host owns the only state.
    pub fn runtime_config(&self, store: GuestBookStore) -> RuntimeConfig {
        let mut config = RuntimeConfig::new(self.project.clone());
        config.grants = Grants {
            read: Scope::Only(BTreeSet::from([self.project.clone()])),
            write: Scope::None,
            net: Scope::All,
            env: Scope::Only(GRANTED_ENV.map(str::to_owned).into()),
            addons: Scope::Only(BTreeSet::from([self.node_modules.clone()])),
        };
        config.env = BTreeMap::from([
            ("PORT".to_owned(), self.port.to_string()),
            ("NODE_ENV".to_owned(), self.mode.node_env().to_owned()),
        ]);
        config.host_modules = vec![guest_book_module(store)];
        config
    }
}

/// The guest may read anything under `project`, so the store it must not see has to resolve
/// elsewhere; both sides are compared canonical so a symlink cannot hide the placement.
fn database_outside(database: &Path, project: &Path) -> Result<PathBuf, LaunchError> {
    let database_error = |source| LaunchError::Database { path: database.to_owned(), source };
    let database = std::path::absolute(database)
        .and_then(|absolute| canonical_path(&absolute))
        .map_err(database_error)?;
    if database.starts_with(project) {
        return Err(LaunchError::DatabaseInsideProject { database, project: project.to_owned() });
    }
    Ok(database)
}
