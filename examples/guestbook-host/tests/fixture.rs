//! The disposable guest book copy the serving tests run against.

mod support;

use std::fs;

use serde_json::Value;

use support::fixture;

#[test]
fn the_fixture_copy_has_its_locked_dependencies_installed() {
    let copy = fixture();
    assert!(copy.join("server.embedded.ts").is_file());
    assert!(copy.join("app/data/host-store.ts").is_file());
    assert!(!copy.join("db/data.db").exists(), "the copy must not carry a database");

    let remix_manifest = fs::read_to_string(copy.join("node_modules/remix/package.json")).unwrap();
    let remix: Value = serde_json::from_str(&remix_manifest).unwrap();
    assert_eq!(remix["version"], "3.0.0");
    assert!(copy.join("node_modules/.bin/remix").exists());
    let remix_real_path = fs::canonicalize(copy.join("node_modules/remix")).unwrap();
    let assets_link = remix_real_path.parent().unwrap().join("@remix-run/assets");
    let assets_real_path = fs::canonicalize(assets_link).unwrap();
    let scope_siblings = assets_real_path.parent().unwrap().parent().unwrap();
    let oxc_transform = scope_siblings.join("oxc-transform/package.json");
    assert!(oxc_transform.is_file(), "remix/assets resolves oxc-transform beside it");
}
