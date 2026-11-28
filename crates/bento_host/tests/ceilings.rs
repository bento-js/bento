use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use bento_host::{EnvPattern, Excess, PathPattern, Permission, PermissionSet, Permissions, Scope};
use proptest::prelude::*;

fn prefix(path: &str) -> PathPattern {
    PathPattern::prefix(path)
}

fn exact(path: &str) -> PathPattern {
    PathPattern::Exact(PathBuf::from(path))
}

fn names(patterns: &[&str]) -> Scope<EnvPattern> {
    Scope::only(patterns.iter().map(|pattern| EnvPattern::parse(pattern).unwrap()))
}

fn project_ceiling() -> Permissions {
    Permissions::restricted(PermissionSet {
        fs_read: Scope::only([prefix("/srv/app/"), prefix("/tmp/cache")]),
        fs_write: Scope::only([exact("/srv/app/data.db")]),
        env: names(&["APP_*", "PORT"]),
        net: true,
        ..PermissionSet::default()
    })
}

#[test]
fn permissions_round_trip_through_their_ceiling_encoding() {
    let mut dropped = project_ceiling();
    dropped.drop_variable("APP_SECRET");
    dropped.drop_whole(Permission::Net);
    for permissions in [
        Permissions::unrestricted(),
        Permissions::unrestricted_denying(PermissionSet {
            fs_read: Scope::only([exact("/srv/app/.env")]),
            addons: true,
            ..PermissionSet::default()
        }),
        project_ceiling(),
        dropped,
    ] {
        let encoded = permissions.to_ceiling_value().unwrap();

        assert_eq!(Permissions::from_ceiling_value(&encoded).unwrap(), permissions);
    }
}

#[test]
fn text_that_is_not_an_encoding_is_an_invalid_ceiling() {
    for text in ["", "{}", "not json", r#"{"read":{"scope":"all"}}"#] {
        assert!(Permissions::from_ceiling_value(text).is_err(), "{text:?} is rejected");
    }
}

#[test]
fn a_path_that_is_not_utf8_cannot_be_encoded_as_a_ceiling() {
    let path = Path::new(OsStr::from_bytes(b"/srv/\xff"));
    let permissions = Permissions::restricted(PermissionSet {
        fs_read: Scope::only([PathPattern::Exact(path.to_owned())]),
        ..PermissionSet::default()
    });

    assert!(permissions.to_ceiling_value().is_err());
}

#[test]
fn an_unrestricted_ceiling_holds_any_request() {
    let request = PermissionSet::everything();

    assert_eq!(Permissions::unrestricted().excess(&request), None);
}

#[test]
fn a_request_within_the_ceiling_fits() {
    let request = PermissionSet {
        fs_read: Scope::only([
            prefix("/srv/app/src/"),
            exact("/srv/app/package.json"),
            prefix("/tmp/cache-2"),
        ]),
        fs_write: Scope::only([exact("/srv/app/data.db")]),
        env: names(&["APP_PORT", "APP_DB_*", "PORT"]),
        net: true,
        ..PermissionSet::default()
    };

    assert_eq!(project_ceiling().excess(&request), None);
}

#[test]
fn the_first_part_of_a_request_beyond_the_ceiling_is_named() {
    let ceiling = project_ceiling();
    let cases = [
        (
            PermissionSet { fs_read: Scope::All, ..PermissionSet::default() },
            Excess { permission: Permission::FsRead, requested: Some("*".into()) },
        ),
        (
            PermissionSet { fs_read: Scope::only([prefix("/srv/")]), ..PermissionSet::default() },
            Excess { permission: Permission::FsRead, requested: Some("/srv/*".into()) },
        ),
        (
            PermissionSet {
                fs_write: Scope::only([prefix("/srv/app/")]),
                ..PermissionSet::default()
            },
            Excess { permission: Permission::FsWrite, requested: Some("/srv/app/*".into()) },
        ),
        (
            PermissionSet { env: names(&["APP*"]), ..PermissionSet::default() },
            Excess { permission: Permission::Env, requested: Some("APP*".into()) },
        ),
        (
            PermissionSet { child_process: true, ..PermissionSet::default() },
            Excess { permission: Permission::ChildProcess, requested: None },
        ),
    ];

    for (request, excess) in cases {
        assert_eq!(ceiling.excess(&request), Some(excess));
    }
}

#[test]
fn a_request_overlapping_a_dropped_part_of_the_ceiling_does_not_fit() {
    let mut ceiling = project_ceiling();
    ceiling.drop_variable("APP_SECRET");

    let request = PermissionSet { env: names(&["APP_*"]), ..PermissionSet::default() };

    assert_eq!(
        ceiling.excess(&request),
        Some(Excess { permission: Permission::Env, requested: Some("APP_*".into()) })
    );
    let narrower = PermissionSet { env: names(&["APP_PORT"]), ..PermissionSet::default() };
    assert_eq!(ceiling.excess(&narrower), None);
}

#[test]
fn a_ceiling_describes_how_much_of_a_permission_it_holds() {
    let mut ceiling = project_ceiling();
    ceiling.drop_variable("APP_SECRET");

    assert_eq!(ceiling.describe(Permission::Net), "all");
    assert_eq!(ceiling.describe(Permission::Worker), "none");
    assert_eq!(ceiling.describe(Permission::FsRead), "/srv/app/*, /tmp/cache*");
    assert_eq!(ceiling.describe(Permission::Env), "APP_*, PORT except APP_SECRET");
    assert_eq!(Permissions::unrestricted().describe(Permission::FsRead), "all");
}

/// Paths beneath `/r` built from a few short names, so generated patterns and paths often
/// share prefixes.
fn path_text() -> impl Strategy<Value = String> {
    prop::collection::vec(prop::sample::select(vec!["a", "b", "ab", "a.b"]), 0..4)
        .prop_map(|names| format!("/r/{}", names.join("/")))
}

fn path_pattern() -> impl Strategy<Value = PathPattern> {
    (path_text(), 0..3u8, prop::sample::select(vec!["", "a"])).prop_map(|(text, kind, partial)| {
        match kind {
            0 => PathPattern::Exact(PathBuf::from(text)),
            1 => PathPattern::prefix(format!("{text}/")),
            _ => PathPattern::prefix(format!("{text}{partial}")),
        }
    })
}

fn path_scope() -> impl Strategy<Value = Scope<PathPattern>> {
    prop_oneof![
        1 => Just(Scope::None),
        1 => Just(Scope::All),
        6 => prop::collection::btree_set(path_pattern(), 1..4).prop_map(Scope::Only),
    ]
}

fn env_scope() -> impl Strategy<Value = Scope<EnvPattern>> {
    let pattern = prop::sample::select(vec!["A", "AB", "B", "A*", "AB*", "B*"])
        .prop_map(|text| EnvPattern::parse(text).unwrap());
    prop_oneof![
        1 => Just(Scope::None),
        1 => Just(Scope::All),
        6 => prop::collection::btree_set(pattern, 1..4).prop_map(Scope::Only),
    ]
}

fn permission_set() -> impl Strategy<Value = PermissionSet> {
    (path_scope(), path_scope(), env_scope(), any::<[bool; 4]>()).prop_map(
        |(fs_read, fs_write, env, [net, child_process, worker, addons])| PermissionSet {
            fs_read,
            fs_write,
            env,
            net,
            child_process,
            worker,
            addons,
            ..PermissionSet::default()
        },
    )
}

fn ceiling() -> impl Strategy<Value = Permissions> {
    let dropped = prop::collection::vec(prop::sample::select(vec!["A", "AB", "B", "ABC"]), 0..2);
    (permission_set(), any::<bool>(), dropped).prop_map(|(set, restricted, dropped)| {
        let mut permissions = if restricted {
            Permissions::restricted(set)
        } else {
            Permissions::unrestricted_denying(set)
        };
        for name in dropped {
            permissions.drop_variable(name);
        }
        permissions
    })
}

/// A nested launch's request: mostly a single pattern or nothing per permission, so that it
/// often fits a generated ceiling.
fn request() -> impl Strategy<Value = PermissionSet> {
    fn sparse<T: Ord + Clone + std::fmt::Debug>(
        pattern: impl Strategy<Value = T> + 'static,
    ) -> impl Strategy<Value = Scope<T>> {
        prop_oneof![
            4 => Just(Scope::None),
            1 => Just(Scope::All),
            3 => pattern.prop_map(|pattern| Scope::only([pattern])),
        ]
    }
    let env = prop::sample::select(vec!["A", "AB", "B", "A*", "AB*", "B*"])
        .prop_map(|text| EnvPattern::parse(text).unwrap());
    let flag = || prop::bool::weighted(0.2);
    (sparse(path_pattern()), sparse(path_pattern()), sparse(env), flag(), flag(), flag()).prop_map(
        |(fs_read, fs_write, env, net, child_process, worker)| PermissionSet {
            fs_read,
            fs_write,
            env,
            net,
            child_process,
            worker,
            ..PermissionSet::default()
        },
    )
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 4096, failure_persistence: None, ..ProptestConfig::default() })]

    /// A nested launch can never widen the authority it inherited: whatever fits the ceiling
    /// holds nothing the ceiling does not.
    #[test]
    fn a_request_that_fits_never_holds_more_than_its_ceiling(
        ceiling in ceiling(),
        request in request(),
        paths in prop::collection::vec((path_text(), prop::sample::select(vec!["", "a", "/a"])), 1..6),
        variables in prop::collection::vec(prop::sample::select(vec!["A", "AB", "ABC", "B", "BA", "C"]), 1..4),
    ) {
        if ceiling.excess(&request).is_some() {
            return Ok(());
        }
        let nested = Permissions::restricted(request);

        for (text, suffix) in &paths {
            let path = PathBuf::from(format!("{text}{suffix}"));
            for permission in [Permission::FsRead, Permission::FsWrite] {
                prop_assert!(
                    !nested.holds_path(permission, &path) || ceiling.holds_path(permission, &path),
                    "{permission:?} {path:?}"
                );
            }
        }
        for name in &variables {
            prop_assert!(!nested.holds_variable(name) || ceiling.holds_variable(name), "{name}");
        }
        for permission in Permission::ALL {
            prop_assert!(!nested.holds(permission) || ceiling.holds(permission), "{permission:?}");
        }
    }
}
