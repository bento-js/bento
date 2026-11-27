use std::fs;
use std::iter;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

use clap::Parser;
use tempfile::TempDir;

use bento_host::{Grants, NetTarget, Scope};

use super::GrantFlags;

#[derive(Parser)]
struct Flags {
    #[command(flatten)]
    grants: GrantFlags,
}

/// The grants `flags` give a launch started in `directory` under `ceiling`.
fn resolve(directory: &Path, flags: &[&str], ceiling: Option<&Grants>) -> Grants {
    let parsed = Flags::try_parse_from(iter::once("bento").chain(flags.iter().copied())).unwrap();
    parsed.grants.resolve(directory, ceiling).unwrap()
}

/// A project directory holding `data/inner/`, `db/`, and `node_modules/`, and its canonical
/// path, which is how grants name it.
fn project() -> (TempDir, PathBuf) {
    let directory = tempfile::tempdir().unwrap();
    for relative in ["data/inner", "db", "node_modules"] {
        fs::create_dir_all(directory.path().join(relative)).unwrap();
    }
    let root = directory.path().canonicalize().unwrap();
    (directory, root)
}

fn paths<const N: usize>(paths: [PathBuf; N]) -> Scope<PathBuf> {
    Scope::only(paths)
}

fn names<const N: usize>(names: [&str; N]) -> Scope<String> {
    Scope::only(names.map(str::to_owned))
}

#[test]
fn the_guest_book_grant_set_resolves_to_canonical_scopes() {
    let (_directory, root) = project();
    let flags = [
        "--allow-read=.",
        "--allow-write=./db",
        "--allow-net",
        "--allow-env=PORT,DATABASE_URL,NODE_ENV",
        "--allow-addons=./node_modules",
    ];

    let grants = resolve(&root, &flags, None);

    let expected = Grants {
        read: paths([root.clone()]),
        write: paths([root.join("db")]),
        net: Scope::All,
        env: names(["PORT", "DATABASE_URL", "NODE_ENV"]),
        addons: paths([root.join("node_modules")]),
        language_model: false,
    };
    assert_eq!(grants, expected);
}

/// The temporary directory is reached through a symlink, such as `/var` on macOS, so its
/// own path is not its canonical one.
#[test]
fn grant_paths_resolve_from_the_invocation_directory_through_symlinks() {
    let (directory, root) = project();
    symlink(root.join("data"), root.join("linked-data")).unwrap();
    let through_link = directory.path().join("db");
    let read = format!("--allow-read=../linked-data,{}", through_link.display());

    let grants = resolve(&directory.path().join("data"), &[&read], None);

    assert_eq!(grants.read, paths([root.join("data"), root.join("db")]));
    assert_eq!(grants.write, Scope::None);
}

#[test]
fn net_targets_and_env_names_are_listed_items() {
    let (_directory, root) = project();
    let flags = ["--allow-net=localhost:3000,[::1]:8080,example.com", "--allow-env=PORT"];

    let grants = resolve(&root, &flags, None);

    let targets = [
        NetTarget::new("localhost", Some(3000)),
        NetTarget::new("::1", Some(8080)),
        NetTarget::new("example.com", None),
    ];
    assert_eq!(grants.net, Scope::only(targets));
    assert_eq!(grants.env, names(["PORT"]));
}

#[test]
fn a_launch_without_grant_flags_inherits_its_ceiling() {
    let (_directory, root) = project();
    let ceiling = Grants {
        read: paths([root.join("data")]),
        net: Scope::only([NetTarget::new("localhost", None)]),
        env: Scope::All,
        ..Grants::none()
    };

    assert_eq!(resolve(&root, &[], Some(&ceiling)), ceiling);
}

#[test]
fn grant_flags_may_narrow_the_ceiling() {
    let (_directory, root) = project();
    let ceiling = Grants {
        read: paths([root.join("data")]),
        net: Scope::only([NetTarget::new("localhost", None)]),
        ..Grants::none()
    };
    let flags = ["--allow-read=data/inner", "--allow-net=localhost:3000"];

    let grants = resolve(&root, &flags, Some(&ceiling));

    let expected = Grants {
        read: paths([root.join("data/inner")]),
        net: Scope::only([NetTarget::new("localhost", Some(3000))]),
        ..Grants::none()
    };
    assert_eq!(grants, expected);
}

#[test]
fn a_grant_flag_may_spell_an_inherited_endpoint_differently() {
    let (_directory, root) = project();
    let endpoint = "[::1]:18540".parse::<NetTarget>().unwrap();
    let ceiling = Grants { net: Scope::only([endpoint.clone()]), ..Grants::none() };

    let grants = resolve(&root, &["--allow-net=[0:0:0:0:0:0:0:1]:18540"], Some(&ceiling));

    assert_eq!(grants.net, Scope::only([endpoint]));
}

#[test]
fn the_language_model_flag_grants_model_access_and_nothing_else() {
    let (_directory, root) = project();

    let granted = resolve(&root, &["--allow-language-model"], None);
    let denied = resolve(&root, &["--allow-language-model=false"], None);

    assert_eq!(granted, Grants { language_model: true, ..Grants::none() });
    assert_eq!(denied, Grants::none());
}

#[test]
fn the_language_model_capability_is_inherited_or_denied_under_a_ceiling() {
    let (_directory, root) = project();
    let ceiling = Grants { language_model: true, ..Grants::none() };

    let inherited = resolve(&root, &[], Some(&ceiling));
    let narrowed = resolve(&root, &["--allow-language-model=false"], Some(&ceiling));

    assert!(inherited.language_model);
    assert!(!narrowed.language_model);
}
