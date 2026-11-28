use std::path::{Path, PathBuf};

use bento_host::{PathPattern, canonical_path};

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
    let allowed = PathPattern::resolve(&layout.allowed, Path::new("/")).unwrap();
    assert!(!allowed.covers(&canonical_path(&through_link).unwrap()));
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
    let allowed = PathPattern::resolve(&layout.allowed, Path::new("/")).unwrap();
    assert!(!allowed.covers(&canonical_path(&layout.allowed.join("new.db")).unwrap()));
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
