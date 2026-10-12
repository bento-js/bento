mod support;

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::LazyLock;

use support::{bento_in, path_without_other_runtimes, stderr, stdout};

const EXAMPLE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples/direct-source/");
const EXAMPLE_COPY: &str = "/tmp/bento-cli/direct-source";
/// What `main.ts` prints: its TSX component rendered to HTML.
const RENDERED_MENU: &str = "<ul><li>Tamagoyaki: 10 minutes</li><li>Onigiri: 5 minutes</li></ul>\n";

/// A copy of the example with its locked dependencies installed by `bento install`, as its
/// README says. Reusing one location keeps repeated runs from accumulating installations.
fn installed_example() -> &'static Path {
    static EXAMPLE_ROOT: LazyLock<PathBuf> = LazyLock::new(|| {
        fs::create_dir_all(EXAMPLE_COPY).unwrap();
        let copied = Command::new("rsync")
            .args(["-a", "--delete", "--exclude", "node_modules", EXAMPLE, EXAMPLE_COPY])
            .status()
            .unwrap();
        assert!(copied.success());
        let installed = bento_in(Path::new(EXAMPLE_COPY), &["install", "--frozen-lockfile"])
            .env("PATH", std::env::var_os("PATH").unwrap())
            .env("HOME", std::env::var_os("HOME").unwrap())
            .output()
            .unwrap();
        assert!(installed.status.success(), "{}", stderr(&installed));
        Path::new(EXAMPLE_COPY).canonicalize().unwrap()
    });
    &EXAMPLE_ROOT
}

/// Every file and link beneath `directory`, installed packages included, without following
/// links.
fn files_beneath(directory: &Path) -> BTreeSet<PathBuf> {
    let mut files = BTreeSet::new();
    for entry in fs::read_dir(directory).unwrap() {
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_dir() {
            files.extend(files_beneath(&entry.path()));
        } else {
            files.insert(entry.path());
        }
    }
    files
}

/// `bento run` with the README's grant, checking that running wrote nothing to the project.
fn run_main(path: &str) -> Output {
    let root = installed_example();
    let before = files_beneath(root);

    let output =
        bento_in(root, &["run", "--allow-read=.", "main.ts"]).env("PATH", path).output().unwrap();

    assert_eq!(files_beneath(root), before, "running created or removed files");
    output
}

#[test]
fn the_example_runs_from_typescript_and_tsx_without_emitting_javascript() {
    let output = run_main(support::SYSTEM_PATH);

    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(stdout(&output), RENDERED_MENU);
}

#[test]
fn the_example_runs_without_node_bun_deno_or_pnpm_on_path() {
    let output = run_main(&path_without_other_runtimes());

    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(stdout(&output), RENDERED_MENU);
}
