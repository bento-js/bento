use std::path::{Path, PathBuf};

use bento_host::{GrantKind, GrantRequest, Grants, NetTarget, Scope, canonical_path};

fn paths(items: &[&str]) -> Scope<PathBuf> {
    Scope::only(items.iter().map(PathBuf::from))
}

fn nets(items: &[&str]) -> Scope<NetTarget> {
    Scope::only(items.iter().map(|item| item.parse::<NetTarget>().unwrap()))
}

#[test]
fn top_level_request_without_options_grants_nothing() {
    let grants = GrantRequest::default().resolve(None).unwrap();
    assert_eq!(grants, Grants::none());
    assert!(!grants.read.allows(Path::new("/project")));
    assert!(!grants.env.allows(&"PORT".to_string()));
}

#[test]
fn unscoped_option_grants_the_whole_kind() {
    let request = GrantRequest { net: Some(Scope::All), ..GrantRequest::default() };
    let grants = request.resolve(None).unwrap();
    assert!(grants.net.allows(&"example.com:443".parse().unwrap()));
    assert_eq!(grants.read, Scope::None);
}

#[test]
fn path_scope_covers_descendants_but_not_siblings_or_prefix_lookalikes() {
    let read = paths(&["/project"]);
    assert!(read.allows(Path::new("/project")));
    assert!(read.allows(Path::new("/project/app/router.ts")));
    assert!(!read.allows(Path::new("/project-other/secret")));
    assert!(!read.allows(Path::new("/etc/passwd")));
}

#[test]
fn net_target_without_port_covers_every_port_of_that_host() {
    let net = nets(&["localhost", "[::]:1612"]);
    assert!(net.allows(&"localhost:80".parse().unwrap()));
    assert!(net.allows(&"LOCALHOST:443".parse().unwrap()));
    assert!(net.allows(&"[::]:1612".parse().unwrap()));
    assert!(!net.allows(&"[::]:1613".parse().unwrap()));
    assert!(!net.allows(&"0.0.0.0:1612".parse().unwrap()));
}

#[test]
fn net_target_parses_hosts_ports_and_bracketed_ipv6() {
    let target: NetTarget = "[::1]:8080".parse().unwrap();
    assert_eq!(target.host(), "::1");
    assert_eq!(target.port(), Some(8080));
    let target: NetTarget = "example.com".parse().unwrap();
    assert_eq!(target.port(), None);
    assert_eq!(target.to_string(), "example.com");
    assert_eq!("[::]:1612".parse::<NetTarget>().unwrap().to_string(), "[::]:1612");
    assert!("".parse::<NetTarget>().is_err());
    assert!("host:notaport".parse::<NetTarget>().is_err());
    assert!("[::1".parse::<NetTarget>().is_err());
}

#[test]
fn nested_request_without_options_inherits_the_ceiling() {
    let ceiling = GrantRequest {
        read: Some(paths(&["/project"])),
        env: Some(Scope::only(["PORT".to_string()])),
        ..GrantRequest::default()
    }
    .resolve(None)
    .unwrap();
    let nested = GrantRequest::default().resolve(Some(&ceiling)).unwrap();
    assert_eq!(nested, ceiling);
}

#[test]
fn nested_request_narrows_only_the_kinds_it_names() {
    let ceiling = GrantRequest {
        read: Some(paths(&["/project"])),
        write: Some(paths(&["/project/db"])),
        ..GrantRequest::default()
    }
    .resolve(None)
    .unwrap();
    let nested = GrantRequest { read: Some(paths(&["/project/app"])), ..GrantRequest::default() }
        .resolve(Some(&ceiling))
        .unwrap();
    assert_eq!(nested.read, paths(&["/project/app"]));
    assert_eq!(nested.write, paths(&["/project/db"]));
}

#[test]
fn nested_request_wider_than_the_ceiling_fails() {
    let ceiling = GrantRequest {
        read: Some(paths(&["/project"])),
        net: Some(nets(&["localhost:1612"])),
        ..GrantRequest::default()
    }
    .resolve(None)
    .unwrap();

    let error = GrantRequest { read: Some(Scope::All), ..GrantRequest::default() }
        .resolve(Some(&ceiling))
        .unwrap_err();
    assert_eq!(error.kind, GrantKind::Read);

    let error = GrantRequest { read: Some(paths(&["/etc"])), ..GrantRequest::default() }
        .resolve(Some(&ceiling))
        .unwrap_err();
    assert_eq!(error.kind, GrantKind::Read);
    assert!(error.to_string().contains("/etc"), "{error}");

    let error = GrantRequest { net: Some(nets(&["localhost"])), ..GrantRequest::default() }
        .resolve(Some(&ceiling))
        .unwrap_err();
    assert_eq!(error.kind, GrantKind::Net);

    let error = GrantRequest { addons: Some(paths(&["/project"])), ..GrantRequest::default() }
        .resolve(Some(&ceiling))
        .unwrap_err();
    assert_eq!(error.kind, GrantKind::Addons);
}

#[test]
fn ceiling_round_trips_through_its_serialized_form() {
    let ceiling = Grants {
        read: paths(&["/project"]),
        write: Scope::None,
        net: Scope::All,
        env: Scope::only(["PORT".to_string(), "NODE_ENV".to_string()]),
        addons: paths(&["/project/node_modules"]),
    };
    let encoded = ceiling.to_ceiling_value();
    assert_eq!(Grants::from_ceiling_value(&encoded).unwrap(), ceiling);
    assert!(Grants::from_ceiling_value("not json").is_err());
}

#[test]
fn canonical_path_resolves_symlinks_and_keeps_missing_tails() {
    let root = tempfile::tempdir().unwrap();
    let real = root.path().join("real");
    std::fs::create_dir(&real).unwrap();
    let link = root.path().join("link");
    std::os::unix::fs::symlink(&real, &link).unwrap();

    let canonical_real = std::fs::canonicalize(&real).unwrap();
    assert_eq!(canonical_path(&link).unwrap(), canonical_real);
    assert_eq!(
        canonical_path(&link.join("not-yet/created.db")).unwrap(),
        canonical_real.join("not-yet/created.db")
    );
    assert_eq!(canonical_path(&link.join("a/../b")).unwrap(), canonical_real.join("b"));
    assert!(canonical_path(Path::new("relative/path")).is_err());
}

#[test]
fn symlinked_request_cannot_escape_a_canonical_ceiling() {
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("project");
    let outside = root.path().join("outside");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::create_dir_all(&outside).unwrap();
    std::os::unix::fs::symlink(&outside, project.join("escape")).unwrap();

    let ceiling =
        Grants { read: Scope::only([canonical_path(&project).unwrap()]), ..Grants::none() };
    let request = GrantRequest {
        read: Some(Scope::only([canonical_path(&project.join("escape")).unwrap()])),
        ..GrantRequest::default()
    };
    assert_eq!(request.resolve(Some(&ceiling)).unwrap_err().kind, GrantKind::Read);
}
