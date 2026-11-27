use std::path::{Path, PathBuf};

use bento_host::{EnvPattern, PathPattern, Permission, PermissionSet, Permissions, Scope};

fn prefix(path: &str) -> PathPattern {
    PathPattern::prefix(path)
}

fn exact(path: &str) -> PathPattern {
    PathPattern::Exact(PathBuf::from(path))
}

fn names(patterns: &[&str]) -> Scope<EnvPattern> {
    Scope::only(patterns.iter().map(|pattern| EnvPattern::parse(pattern).unwrap()))
}

const FLAGS: [Permission; 10] = [
    Permission::Net,
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

#[test]
fn an_unrestricted_instance_holds_every_permission_and_withholds_nothing() {
    let permissions = Permissions::unrestricted();

    assert!(!permissions.is_restricted());
    assert!(!permissions.withholds_anything());
    for permission in Permission::ALL {
        assert!(permissions.holds(permission), "{permission:?} is held");
    }
    assert!(permissions.holds_path(Permission::FsWrite, Path::new("/etc/hosts")));
    assert!(permissions.holds_variable("DATABASE_URL"));
    assert!(!permissions.removes_variable("DATABASE_URL"));
}

#[test]
fn a_restricted_instance_holds_only_what_it_was_allowed() {
    let permissions = Permissions::restricted(PermissionSet {
        fs_read: Scope::only([prefix("/srv/app/")]),
        env: names(&["PORT"]),
        net: true,
        ..PermissionSet::default()
    });

    assert!(permissions.is_restricted());
    assert!(permissions.withholds_anything());
    assert!(permissions.holds(Permission::Net));
    assert!(permissions.holds_path(Permission::FsRead, Path::new("/srv/app/main.ts")));
    assert!(!permissions.holds_path(Permission::FsRead, Path::new("/etc/hosts")));
    assert!(!permissions.holds_path(Permission::FsWrite, Path::new("/srv/app/main.ts")));
    assert!(permissions.holds_variable("PORT"));
    assert!(!permissions.holds_variable("HOST"));
    for permission in FLAGS.into_iter().filter(|permission| *permission != Permission::Net) {
        assert!(!permissions.holds(permission), "{permission:?} is not held");
    }
}

#[test]
fn a_restricted_instance_allowed_nothing_still_withholds_everything() {
    let permissions = Permissions::restricted(PermissionSet::default());

    assert!(permissions.withholds_anything());
    for permission in Permission::ALL {
        assert!(!permissions.holds_any(permission), "{permission:?} is withheld");
    }
}

#[test]
fn only_an_allowance_of_everything_holds_all_of_files_or_the_environment() {
    let partial = Permissions::restricted(PermissionSet {
        fs_read: Scope::only([prefix("/")]),
        env: names(&["A*"]),
        ..PermissionSet::default()
    });
    let whole = Permissions::restricted(PermissionSet {
        fs_read: Scope::All,
        env: Scope::All,
        ..PermissionSet::default()
    });

    assert!(!partial.holds(Permission::FsRead), "has('fs.read') needs --allow-fs-read=*");
    assert!(partial.holds_any(Permission::FsRead));
    assert!(!partial.holds(Permission::Env));
    assert!(whole.holds(Permission::FsRead));
    assert!(whole.holds(Permission::Env));
}

#[test]
fn a_denial_takes_away_exactly_what_the_matching_allowance_would_give() {
    let permissions = Permissions::unrestricted_denying(PermissionSet {
        fs_read: Scope::only([prefix("/secrets/"), exact("/srv/app/.env")]),
        env: names(&["AWS_*"]),
        child_process: true,
        ..PermissionSet::default()
    });

    assert!(!permissions.is_restricted());
    assert!(permissions.withholds_anything());
    assert!(!permissions.holds_path(Permission::FsRead, Path::new("/secrets/key")));
    assert!(!permissions.holds_path(Permission::FsRead, Path::new("/srv/app/.env")));
    assert!(permissions.holds_path(Permission::FsRead, Path::new("/srv/app/.env.example")));
    assert!(permissions.holds_path(Permission::FsWrite, Path::new("/secrets/key")));
    assert!(!permissions.holds_variable("AWS_SECRET_ACCESS_KEY"));
    assert!(permissions.holds_variable("PORT"));
    assert!(!permissions.holds(Permission::ChildProcess));
    assert!(permissions.holds(Permission::Worker));
}

#[test]
fn any_denial_takes_the_whole_of_a_permission_away() {
    let permissions = Permissions::unrestricted_denying(PermissionSet {
        fs_read: Scope::only([exact("/srv/app/.env")]),
        env: names(&["SECRET"]),
        ..PermissionSet::default()
    });

    assert!(!permissions.holds(Permission::FsRead));
    assert!(permissions.holds_any(Permission::FsRead));
    assert!(!permissions.holds(Permission::Env));
    assert!(permissions.holds(Permission::FsWrite));
}

#[test]
fn variables_node_always_keeps_survive_removal_but_answer_to_drops() {
    let mut permissions = Permissions::restricted(PermissionSet {
        env: names(&["APP_*"]),
        ..PermissionSet::default()
    });

    assert!(!permissions.removes_variable("HOME"));
    assert!(!permissions.removes_variable("APP_PORT"));
    assert!(permissions.removes_variable("NODE_ENV"));
    assert!(permissions.has_variable("HOME"), "has('env', 'HOME') is true, as in Node");
    assert!(!permissions.holds_variable("HOME"));

    permissions.drop_variable("HOME");

    assert!(!permissions.has_variable("HOME"));
}

#[test]
fn denying_the_whole_environment_removes_everything_but_the_always_kept_variables() {
    let permissions = Permissions::unrestricted_denying(PermissionSet {
        env: Scope::All,
        ..PermissionSet::default()
    });

    assert!(permissions.removes_variable("PORT"));
    assert!(!permissions.removes_variable("PATH"));
    assert!(!permissions.removes_variable("LC_ALL"));
}

#[test]
fn dropping_a_variable_removes_it_even_under_an_allowance_of_everything() {
    let mut permissions =
        Permissions::restricted(PermissionSet { env: Scope::All, ..PermissionSet::default() });

    permissions.drop_variable("APP_A");

    assert!(!permissions.holds_variable("APP_A"));
    assert!(permissions.holds_variable("APP_B"));
    assert!(!permissions.holds(Permission::Env));
}

#[test]
fn dropping_the_environment_forgets_single_drops_and_keeps_answering_for_kept_variables() {
    let mut permissions = Permissions::unrestricted();
    permissions.drop_variable("HOME");
    assert!(!permissions.has_variable("HOME"));

    permissions.drop_whole(Permission::Env);

    assert!(permissions.has_variable("HOME"), "Node answers for an always-kept name again");
    assert!(permissions.has_variable("PATH"));
    assert!(!permissions.has_variable("APP_A"));
    assert!(!permissions.holds_any(Permission::Env));
    assert!(permissions.withholds_anything());

    permissions.drop_variable("PATH");

    assert!(!permissions.has_variable("PATH"), "a name dropped afterwards is dropped anew");
}

#[test]
fn a_dropped_name_with_a_wildcard_changes_nothing() {
    let mut permissions = Permissions::restricted(PermissionSet {
        env: names(&["APP_*"]),
        ..PermissionSet::default()
    });
    let before = permissions.clone();

    permissions.drop_variable("APP_*");

    assert_eq!(permissions, before);
}

#[test]
fn dropping_a_pattern_removes_exactly_that_allowance() {
    let mut permissions = Permissions::restricted(PermissionSet {
        fs_read: Scope::only([
            prefix("/srv/existing/"),
            prefix("/srv/existing/sub/"),
            prefix("/srv/test"),
        ]),
        ..PermissionSet::default()
    });

    permissions.drop_pattern(Permission::FsRead, &prefix("/srv/existing/"));

    assert!(!permissions.holds_path(Permission::FsRead, Path::new("/srv/existing/file")));
    assert!(permissions.holds_path(Permission::FsRead, Path::new("/srv/existing/sub/file")));

    permissions.drop_pattern(Permission::FsRead, &exact("/srv/test2"));

    assert!(
        permissions.holds_path(Permission::FsRead, Path::new("/srv/test2")),
        "a reference matching no allowance changes nothing"
    );
}

#[test]
fn dropping_a_pattern_from_an_allowance_of_everything_changes_nothing() {
    let mut restricted =
        Permissions::restricted(PermissionSet { fs_write: Scope::All, ..PermissionSet::default() });
    let mut unrestricted = Permissions::unrestricted();

    restricted.drop_pattern(Permission::FsWrite, &exact("/x"));
    unrestricted.drop_pattern(Permission::FsWrite, &exact("/x"));

    assert!(restricted.holds(Permission::FsWrite));
    assert!(unrestricted.holds_path(Permission::FsWrite, Path::new("/x")));
}

#[test]
fn dropping_a_whole_permission_removes_it_in_either_kind_of_instance() {
    let mut restricted = Permissions::restricted(PermissionSet::everything());
    let mut unrestricted = Permissions::unrestricted();

    for permission in Permission::ALL {
        restricted.drop_whole(permission);
        unrestricted.drop_whole(permission);
        assert!(!restricted.holds_any(permission), "{permission:?} dropped");
        assert!(!unrestricted.holds_any(permission), "{permission:?} dropped");
    }
    assert!(unrestricted.withholds_anything());
}

#[test]
fn each_permission_names_its_flag_and_scope() {
    let named: Vec<_> = Permission::ALL
        .into_iter()
        .map(|permission| (permission.flag(), permission.scope()))
        .collect();

    assert_eq!(
        named,
        [
            ("--allow-fs-read", Some("fs.read")),
            ("--allow-fs-write", Some("fs.write")),
            ("--allow-net", Some("net")),
            ("--allow-env", Some("env")),
            ("--allow-child-process", Some("child")),
            ("--allow-worker", Some("worker")),
            ("--allow-addons", Some("addon")),
            ("--allow-wasi", Some("wasi")),
            ("--allow-ffi", Some("ffi")),
            ("--allow-inspector", Some("inspector")),
            ("--allow-openssl-store", Some("openssl.store")),
            ("--allow-fs-vfs", None),
            ("--allow-language-model", Some("language-model")),
        ]
    );
    assert_eq!(Permission::from_scope("child"), Some(Permission::ChildProcess));
    assert_eq!(Permission::from_scope("fs"), None);
    assert_eq!(Permission::from_scope("fs.vfs"), None);
}
