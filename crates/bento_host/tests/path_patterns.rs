use std::path::{Path, PathBuf};

use bento_host::PathPattern;

/// A canonical temporary directory holding `existing/sub/file.txt`, `file.txt`, and the
/// directories `test`, `test2`, and `testing`.
struct Layout {
    _root: tempfile::TempDir,
    base: PathBuf,
}

fn layout() -> Layout {
    let root = tempfile::tempdir().unwrap();
    let base = std::fs::canonicalize(root.path()).unwrap();
    std::fs::create_dir_all(base.join("existing/sub")).unwrap();
    std::fs::write(base.join("existing/sub/file.txt"), "").unwrap();
    std::fs::write(base.join("file.txt"), "").unwrap();
    for directory in ["test", "test2", "testing"] {
        std::fs::create_dir(base.join(directory)).unwrap();
    }
    Layout { _root: root, base }
}

impl Layout {
    fn pattern(&self, value: &str) -> PathPattern {
        PathPattern::resolve(value, &self.base).unwrap()
    }

    fn covers(&self, value: &str, path: &str) -> bool {
        self.pattern(value).covers(&self.base.join(path))
    }
}

#[test]
fn an_existing_directory_covers_itself_and_everything_beneath_it() {
    let layout = layout();

    assert!(layout.covers("existing", "existing"));
    assert!(layout.covers("existing", "existing/sub/file.txt"));
    assert!(layout.covers("./existing/", "existing/not-yet-created"));
    assert!(!layout.covers("existing", "existingx"));
    assert!(!layout.covers("existing", ""));
}

#[test]
fn a_file_covers_exactly_itself() {
    let layout = layout();

    assert!(layout.covers("file.txt", "file.txt"));
    assert!(!layout.covers("file.txt", "file.txt/child"));
    assert!(!layout.covers("file.txt", "file.txt2"));
}

#[test]
fn a_path_that_does_not_exist_covers_exactly_itself() {
    let layout = layout();

    assert!(layout.covers("build", "build"));
    assert!(!layout.covers("build", "build/out.js"));
    assert!(!layout.covers("missing/deeper", "missing/deeper/file"));
}

#[test]
fn a_trailing_wildcard_covers_a_directory_the_guest_will_create() {
    let layout = layout();

    assert!(layout.covers("./build/*", "build"));
    assert!(layout.covers("./build/*", "build/out/app.js"));
    assert!(!layout.covers("./build/*", "buildx"));
    assert!(!layout.covers("./build/*", ""));
}

#[test]
fn a_wildcard_covers_every_path_beginning_with_the_text_before_it() {
    let layout = layout();

    assert!(layout.covers("./test*", "test"));
    assert!(layout.covers("./test*", "test2"));
    assert!(layout.covers("./test*", "testing/file"));
    assert!(layout.covers("./test*", "test-not-yet-created/file"));
    assert!(!layout.covers("./test*", "tes"));
    assert!(!layout.covers("./test*", "existing"));
}

#[test]
fn the_text_after_the_first_wildcard_is_ignored() {
    let layout = layout();

    assert_eq!(layout.pattern("existing/*zzz"), layout.pattern("existing/*"));
    assert_eq!(layout.pattern("te*st*"), layout.pattern("te*"));
    assert!(layout.covers("existing/*zzz", "existing/sub/file.txt"));
}

#[test]
fn absolute_values_ignore_the_base() {
    let layout = layout();
    let elsewhere = tempfile::tempdir().unwrap();

    let pattern = PathPattern::resolve(layout.base.join("existing"), elsewhere.path()).unwrap();

    assert!(pattern.covers(&layout.base.join("existing/sub")));
}

#[test]
fn values_are_resolved_to_real_locations_before_wildcards() {
    let layout = layout();
    std::os::unix::fs::symlink(layout.base.join("existing"), layout.base.join("link")).unwrap();

    assert_eq!(layout.pattern("link"), layout.pattern("existing"));
    assert_eq!(layout.pattern("link/su*"), layout.pattern("existing/su*"));
    assert!(layout.covers("link/*", "existing/sub/file.txt"));
    assert!(!layout.covers("link/*", "link/sub/file.txt"), "paths are checked where they lead");
}

#[test]
fn a_temporary_directory_link_covers_where_it_leads() {
    let target = tempfile::tempdir().unwrap();
    let real = std::fs::canonicalize(target.path()).unwrap();
    let links = tempfile::tempdir().unwrap();
    let link = links.path().join("app");
    std::os::unix::fs::symlink(&real, &link).unwrap();

    let pattern = PathPattern::resolve(&link, Path::new("/")).unwrap();

    assert!(pattern.covers(&real.join("data.db")));
}

#[test]
fn patterns_read_as_command_line_values() {
    let layout = layout();
    let base = layout.base.display();

    assert_eq!(layout.pattern("file.txt").to_string(), format!("{base}/file.txt"));
    assert_eq!(layout.pattern("existing").to_string(), format!("{base}/existing/*"));
    assert_eq!(layout.pattern("test*").to_string(), format!("{base}/test*"));
    assert_eq!(layout.pattern("missing").to_string(), format!("{base}/missing"));
}

#[test]
fn the_root_directory_is_one_prefix() {
    let pattern = PathPattern::resolve("/", Path::new("/")).unwrap();

    assert_eq!(pattern, PathPattern::Prefix(PathBuf::from("/")));
    assert!(pattern.covers(Path::new("/")));
    assert!(pattern.covers(Path::new("/etc/hosts")));
}
