use std::collections::BTreeMap;
use std::net::TcpListener;

use tempfile::{TempDir, tempdir_in};

use bento_host::{EnvPattern, PathPattern, PermissionSet, Permissions, Scope};
use bento_runtime::{RuntimeConfig, package_lookup_paths};

use crate::support::{config_with, fixture};

/// A migrated copy of the guest-book fixture, its database directory, a free port, and a
/// restricted instance serving it there. The instance may read the project, and also Node's
/// package lookup locations above it when `package_lookups`; write only the database
/// directory; use the network; see `PORT`, `DATABASE_URL`, and `NODE_ENV`; and load native
/// addons.
pub(crate) fn prepare(package_lookups: bool) -> (RuntimeConfig, TempDir, TempDir, u16) {
    let fixture_directory = fixture();
    let root = fixture_directory.path().canonicalize().unwrap();
    let database_directory = tempdir_in(root.join("db")).unwrap();
    let database = database_directory.path().join("test.db");
    let migrated = std::process::Command::new(root.join("node_modules/.bin/remix"))
        .args(["db", "migrate"])
        .env("DATABASE_URL", &database)
        .current_dir(&root)
        .status()
        .unwrap();
    assert!(migrated.success(), "fixture migration failed");
    let port = TcpListener::bind(("127.0.0.1", 0)).unwrap().local_addr().unwrap().port();
    let mut readable = vec![PathPattern::resolve(&root, &root).unwrap()];
    if package_lookups {
        for lookup in package_lookup_paths(&root).unwrap() {
            readable.push(PathPattern::resolve(lookup.join("*"), &root).unwrap());
        }
    }
    let permissions = Permissions::restricted(PermissionSet {
        fs_read: Scope::only(readable),
        fs_write: Scope::only([PathPattern::resolve(database_directory.path(), &root).unwrap()]),
        net: true,
        env: Scope::only(
            ["PORT", "DATABASE_URL", "NODE_ENV"].map(|name| EnvPattern::parse(name).unwrap()),
        ),
        addons: true,
        ..PermissionSet::default()
    });
    let mut settings = config_with(&root, permissions);
    settings.env = BTreeMap::from([
        ("PORT".into(), port.to_string()),
        ("DATABASE_URL".into(), database.display().to_string()),
    ]);
    (settings, fixture_directory, database_directory, port)
}
