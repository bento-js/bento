use std::collections::{BTreeMap, BTreeSet};
use std::net::TcpListener;

use tempfile::{TempDir, tempdir_in};

use bento_host::Scope;
use bento_runtime::RuntimeConfig;

use super::{config, fixture};

pub(crate) fn prepare() -> (RuntimeConfig, TempDir, TempDir, u16) {
    let fixture_directory = fixture();
    let root = fixture_directory.path();
    let database_directory = tempdir_in(root.join("db")).unwrap();
    let database = database_directory.path().join("test.db");
    let migrated = std::process::Command::new(root.join("node_modules/.bin/remix"))
        .args(["db", "migrate"])
        .env("DATABASE_URL", &database)
        .current_dir(root)
        .status()
        .unwrap();
    assert!(migrated.success(), "fixture migration failed");
    let port = TcpListener::bind(("127.0.0.1", 0)).unwrap().local_addr().unwrap().port();
    let mut settings = config(root);
    settings.grants.write =
        Scope::Only(BTreeSet::from([database_directory.path().canonicalize().unwrap()]));
    settings.grants.net = Scope::All;
    settings.grants.env =
        Scope::Only(BTreeSet::from(["PORT".into(), "DATABASE_URL".into(), "NODE_ENV".into()]));
    settings.grants.addons =
        Scope::Only(BTreeSet::from([settings.project_root.join("node_modules")]));
    settings.env = BTreeMap::from([
        ("PORT".into(), port.to_string()),
        ("DATABASE_URL".into(), database.display().to_string()),
    ]);
    (settings, fixture_directory, database_directory, port)
}
