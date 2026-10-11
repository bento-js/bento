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
    assert!(!grants.env.allows("PORT"));
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
fn net_target_rejects_anything_but_a_hostname_or_ip_address_with_an_optional_port() {
    let long_label = format!("{}.com", "a".repeat(64));
    let long_name = ["abcdefghi"; 26].join(".");
    let malformed = [
        "localhost/path",
        "[bogus]:18540",
        "white space",
        "exa_mple.com",
        "-leading.com",
        "trailing-.com",
        "double..dot",
        ".leading.dot",
        "trailing.dot.",
        "*.example.com",
        "http://example.com",
        "[127.0.0.1]:80",
        "[::1]x",
        "::1",
        "localhost:",
        "localhost:+80",
        "localhost:65536",
        "127.1",
        "0x7f.0.0.1",
        "0x7f000001",
        "127.000.0.1",
        "256.0.0.1",
        long_label.as_str(),
        long_name.as_str(),
    ];
    for text in malformed {
        assert!(text.parse::<NetTarget>().is_err(), "`{text}` parsed");
    }
}

#[test]
fn net_target_accepts_hostnames_ipv4_and_bracketed_ipv6() {
    let valid = [
        "localhost",
        "example.com:443",
        "1password.com",
        "xn--bcher-kva.example",
        "a-b.c-d.e",
        "127.0.0.1:80",
        "[::1]",
        "[2001:db8::1]:8080",
    ];
    for text in valid {
        assert_eq!(text.parse::<NetTarget>().unwrap().to_string(), text);
    }
}

#[test]
fn equivalent_spellings_of_an_endpoint_are_one_target() {
    let spellings = [
        ("[0:0:0:0:0:0:0:1]:18540", "[::1]:18540"),
        ("[0000::0001]", "[::1]"),
        ("[2001:DB8::1]:80", "[2001:db8::1]:80"),
        ("[::ffff:127.0.0.1]:80", "127.0.0.1:80"),
        ("LocalHost:3000", "localhost:3000"),
        ("localhost:03000", "localhost:3000"),
    ];
    for (spelling, canonical) in spellings {
        let target: NetTarget = spelling.parse().unwrap();
        assert_eq!(target, canonical.parse().unwrap(), "{spelling}");
        assert_eq!(target.to_string(), canonical, "{spelling}");
    }
    assert_eq!(NetTarget::new("0:0:0:0:0:0:0:1", Some(1)), "[::1]:1".parse().unwrap());
    assert_eq!(NetTarget::new("EXAMPLE.com", None), "example.com".parse().unwrap());
}

#[test]
fn nested_request_spelling_an_inherited_endpoint_differently_fits_the_ceiling() {
    let ceiling = Grants { net: nets(&["[::1]:18540", "Example.com"]), ..Grants::none() };
    let request = GrantRequest {
        net: Some(nets(&["[0:0:0:0:0:0:0:1]:18540", "example.COM:443"])),
        ..GrantRequest::default()
    };
    assert_eq!(
        request.resolve(Some(&ceiling)).unwrap().net,
        nets(&["[::1]:18540", "example.com:443"])
    );
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

/// `allowed/link` points at `outside/child`, and both `allowed` and `outside` hold a `secret.txt`.
struct LinkedLayout {
    _root: tempfile::TempDir,
    allowed: PathBuf,
    outside: PathBuf,
}

fn linked_layout() -> LinkedLayout {
    let root = tempfile::tempdir().unwrap();
    let allowed = root.path().join("allowed");
    let outside = root.path().join("outside");
    std::fs::create_dir_all(outside.join("child")).unwrap();
    std::fs::create_dir_all(&allowed).unwrap();
    std::fs::write(allowed.join("secret.txt"), "allowed").unwrap();
    std::fs::write(outside.join("secret.txt"), "outside").unwrap();
    std::os::unix::fs::symlink(outside.join("child"), allowed.join("link")).unwrap();
    let allowed = std::fs::canonicalize(allowed).unwrap();
    let outside = std::fs::canonicalize(outside).unwrap();
    LinkedLayout { _root: root, allowed, outside }
}

#[test]
fn canonical_path_applies_parent_steps_after_a_symlink_to_the_link_target() {
    let layout = linked_layout();
    let through_link = layout.allowed.join("link/../secret.txt");

    assert_eq!(canonical_path(&through_link).unwrap(), layout.outside.join("secret.txt"));
    assert_eq!(
        canonical_path(&layout.allowed.join("link/../not-yet/created.db")).unwrap(),
        layout.outside.join("not-yet/created.db")
    );
    let read = Scope::only([layout.allowed.clone()]);
    assert!(!read.allows(&canonical_path(&through_link).unwrap()));
}

#[test]
fn canonical_path_resolves_names_after_a_missing_directory_is_left() {
    let layout = linked_layout();

    assert_eq!(
        canonical_path(&layout.allowed.join("missing/../link/../secret.txt")).unwrap(),
        layout.outside.join("secret.txt")
    );
    assert_eq!(
        canonical_path(&layout.allowed.join("missing/deeper/../file.db")).unwrap(),
        layout.allowed.join("missing/file.db")
    );
}

#[test]
fn canonical_path_follows_a_dangling_symlink_to_the_file_it_would_create() {
    let layout = linked_layout();
    std::os::unix::fs::symlink(layout.outside.join("new.db"), layout.allowed.join("new.db"))
        .unwrap();
    std::os::unix::fs::symlink("../outside/fresh/dir", layout.allowed.join("relative")).unwrap();

    assert_eq!(
        canonical_path(&layout.allowed.join("new.db")).unwrap(),
        layout.outside.join("new.db")
    );
    assert_eq!(
        canonical_path(&layout.allowed.join("relative/file.db")).unwrap(),
        layout.outside.join("fresh/dir/file.db")
    );
    let write = Scope::only([layout.allowed.clone()]);
    assert!(!write.allows(&canonical_path(&layout.allowed.join("new.db")).unwrap()));
}

#[test]
fn canonical_path_reports_symlink_loops() {
    let layout = linked_layout();
    std::os::unix::fs::symlink(layout.allowed.join("loop-b"), layout.allowed.join("loop-a"))
        .unwrap();
    std::os::unix::fs::symlink(layout.allowed.join("loop-a"), layout.allowed.join("loop-b"))
        .unwrap();

    assert!(canonical_path(&layout.allowed.join("loop-a")).is_err());
    assert!(canonical_path(&layout.allowed.join("loop-a/missing")).is_err());
}
