use std::fs;
use std::os::unix::fs::symlink;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread;
use std::time::Duration;

use tempfile::TempDir;

use super::SourceWatcher;

const SCRATCH: &str = "/tmp/bento-cli/watch";
const SETTLE: Duration = Duration::from_millis(300);
const CHANGE_DEADLINE: Duration = Duration::from_secs(5);
const QUIET_WINDOW: Duration = Duration::from_millis(600);

/// A disposable directory holding a project in `app/` and files outside it.
struct Workspace {
    _directory: TempDir,
    base: PathBuf,
}

impl Workspace {
    fn new() -> Workspace {
        fs::create_dir_all(SCRATCH).unwrap();
        let directory = tempfile::Builder::new().prefix("watch-").tempdir_in(SCRATCH).unwrap();
        let base = directory.path().canonicalize().unwrap();
        let workspace = Workspace { _directory: directory, base };
        workspace.write("app/package.json", r#"{ "type": "module" }"#);
        workspace.write("app/server.mjs", "import './message.mjs';\n");
        workspace.write("app/message.mjs", "export default 'first';\n");
        workspace
    }

    fn root(&self) -> PathBuf {
        self.path("app")
    }

    fn path(&self, relative: &str) -> PathBuf {
        self.base.join(relative)
    }

    fn write(&self, relative: &str, contents: &str) -> PathBuf {
        let path = self.path(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, contents).unwrap();
        path
    }

    /// Starts watching the project and lets the file-system notifications for setting it up
    /// pass, so that each test observes only its own changes.
    fn watch(&self) -> (SourceWatcher, Receiver<Vec<PathBuf>>) {
        let (sender, changes) = mpsc::channel();
        let watcher = SourceWatcher::start(&self.root(), move |changed| {
            let _ = sender.send(changed);
        })
        .unwrap();
        thread::sleep(SETTLE);
        while changes.try_recv().is_ok() {}
        (watcher, changes)
    }
}

fn next_change(changes: &Receiver<Vec<PathBuf>>) -> Vec<PathBuf> {
    changes.recv_timeout(CHANGE_DEADLINE).expect("no change was reported")
}

fn assert_quiet(changes: &Receiver<Vec<PathBuf>>) {
    match changes.recv_timeout(QUIET_WINDOW) {
        Err(RecvTimeoutError::Timeout) => {}
        Ok(changed) => panic!("unexpected change: {changed:?}"),
        Err(RecvTimeoutError::Disconnected) => panic!("the watcher stopped"),
    }
}

/// Edits `server.mjs` and expects a change naming only it, which shows that nothing
/// written before it was reported.
fn assert_only_the_next_edit_is_a_change(workspace: &Workspace, changes: &Receiver<Vec<PathBuf>>) {
    let server = workspace.write("app/server.mjs", "import './message.mjs';\n// edited\n");
    assert_eq!(next_change(changes), vec![server]);
}

#[test]
fn a_burst_of_edits_is_one_change() {
    let workspace = Workspace::new();
    let (_watcher, changes) = workspace.watch();

    for edit in 0..5 {
        workspace.write("app/message.mjs", &format!("export default {edit};\n"));
        thread::sleep(Duration::from_millis(10));
    }

    assert_eq!(next_change(&changes), vec![workspace.path("app/message.mjs")]);
    assert_quiet(&changes);
}

#[test]
fn deleting_a_module_and_creating_it_again_are_changes() {
    let workspace = Workspace::new();
    let message = workspace.path("app/message.mjs");
    let (_watcher, changes) = workspace.watch();

    fs::remove_file(&message).unwrap();
    assert_eq!(next_change(&changes), vec![message.clone()]);

    workspace.write("app/message.mjs", "export default 'again';\n");
    assert_eq!(next_change(&changes), vec![message]);
}

#[test]
fn replacing_a_module_by_renaming_a_file_over_it_is_a_change() {
    let workspace = Workspace::new();
    let replacement = workspace.write("message.mjs.next", "export default 'replaced';\n");
    let (_watcher, changes) = workspace.watch();

    fs::rename(&replacement, workspace.path("app/message.mjs")).unwrap();

    assert_eq!(next_change(&changes), vec![workspace.path("app/message.mjs")]);
}

#[test]
fn databases_and_their_sqlite_side_files_are_not_changes() {
    let workspace = Workspace::new();
    fs::create_dir(workspace.path("app/db")).unwrap();
    let (_watcher, changes) = workspace.watch();

    for database_file in [
        "app/db/app.db",
        "app/db/app.db-wal",
        "app/db/app.db-shm",
        "app/db/app.db-journal",
        "app/data.sqlite",
        "app/data.sqlite3",
    ] {
        workspace.write(database_file, "rows");
    }

    assert_only_the_next_edit_is_a_change(&workspace, &changes);
}

#[test]
fn dependencies_version_control_build_output_and_caches_are_not_changes() {
    let workspace = Workspace::new();
    let (_watcher, changes) = workspace.watch();

    for generated in [
        "app/node_modules/dependency/index.js",
        "app/.git/HEAD",
        "app/dist/server.js",
        "app/build/server.js",
        "app/target/debug/output",
        "app/.cache/entry",
        "app/coverage/lcov.info",
        "app/.DS_Store",
    ] {
        workspace.write(generated, "generated");
    }

    assert_only_the_next_edit_is_a_change(&workspace, &changes);
}

#[test]
fn environment_files_are_not_changes() {
    let workspace = Workspace::new();
    let (_watcher, changes) = workspace.watch();

    workspace.write("app/.env", "PORT=3000\n");
    workspace.write("app/.env.development", "PORT=3001\n");

    assert_only_the_next_edit_is_a_change(&workspace, &changes);
}

#[test]
fn a_loaded_module_outside_the_project_is_watched_once_it_is_reported() {
    let workspace = Workspace::new();
    let shared = workspace.write("shared/format.mjs", "export const format = String;\n");
    let unloaded = workspace.write("shared/unused.mjs", "export {};\n");
    let dependency = workspace.write("shared/node_modules/dependency/index.js", "export {};\n");
    let (mut watcher, changes) = workspace.watch();

    watcher.watch_modules([shared.clone(), dependency.clone()]).unwrap();
    fs::write(&unloaded, "export const edited = true;\n").unwrap();
    fs::write(&dependency, "export const edited = true;\n").unwrap();
    assert_quiet(&changes);

    fs::write(&shared, "export const format = value => `${value}`;\n").unwrap();
    assert_eq!(next_change(&changes), vec![shared]);
}

#[test]
fn a_loaded_module_in_an_excluded_project_directory_is_watched() {
    let workspace = Workspace::new();
    let entry = workspace.write("app/dist/server.js", "console.log('built');\n");
    let (mut watcher, changes) = workspace.watch();

    watcher.watch_modules([entry.clone()]).unwrap();
    fs::write(&entry, "console.log('rebuilt');\n").unwrap();

    assert_eq!(next_change(&changes), vec![entry]);
}

/// `app/tsconfig.json` extends a file outside the project, which extends another, and a file
/// from a package installed the way pnpm installs it, behind a symbolic link.
#[test]
fn the_tsconfig_extends_chain_is_watched_outside_the_project_and_in_node_modules() {
    let workspace = Workspace::new();
    workspace.write(
        "app/tsconfig.json",
        r#"{ "extends": ["../shared/tsconfig.base.json", "@configs/strict/tsconfig.json"] }"#,
    );
    workspace.write("shared/tsconfig.base.json", r#"{ "extends": "./tsconfig.root.json" }"#);
    let root_config = workspace
        .write("shared/tsconfig.root.json", r#"{ "compilerOptions": { "jsx": "react-jsx" } }"#);
    let installed = "app/node_modules/.pnpm/@configs+strict@1.0.0/node_modules/@configs/strict";
    workspace.write(&format!("{installed}/package.json"), r#"{ "name": "@configs/strict" }"#);
    let package_config = workspace.write(
        &format!("{installed}/tsconfig.json"),
        r#"{ "compilerOptions": { "strict": true } }"#,
    );
    fs::create_dir_all(workspace.path("app/node_modules/@configs")).unwrap();
    symlink(workspace.path(installed), workspace.path("app/node_modules/@configs/strict")).unwrap();
    let (_watcher, changes) = workspace.watch();

    fs::write(&root_config, r#"{ "compilerOptions": { "jsx": "react-jsxdev" } }"#).unwrap();
    assert_eq!(next_change(&changes), vec![root_config]);

    fs::write(&package_config, r#"{ "compilerOptions": { "strict": false } }"#).unwrap();
    assert_eq!(next_change(&changes), vec![package_config]);
}

#[test]
fn dropping_the_watcher_stops_watching() {
    let workspace = Workspace::new();
    let (watcher, changes) = workspace.watch();

    drop(watcher);
    workspace.write("app/message.mjs", "export default 'unwatched';\n");

    assert_eq!(changes.recv_timeout(CHANGE_DEADLINE), Err(RecvTimeoutError::Disconnected));
}
