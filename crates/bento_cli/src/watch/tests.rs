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
        let mut watcher = SourceWatcher::start(&self.root(), move |changed| {
            let _ = sender.send(changed);
        })
        .unwrap();
        assert!(watcher.watch_configuration().is_empty());
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

    assert!(watcher.watch_modules([shared.clone(), dependency.clone()]).is_empty());
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

    assert!(watcher.watch_modules([entry.clone()]).is_empty());
    fs::write(&entry, "console.log('rebuilt');\n").unwrap();

    assert_eq!(next_change(&changes), vec![entry]);
}

#[test]
fn a_missing_module_directory_is_watched_through_its_existing_ancestor() {
    let workspace = Workspace::new();
    let module = workspace.path("later/module.mjs");
    let (mut watcher, changes) = workspace.watch();

    assert!(watcher.watch_modules([module.clone()]).is_empty());
    assert!(watcher.watch_modules([module.clone()]).is_empty());
    workspace.write("later/module.mjs", "export {};\n");
    assert_eq!(next_change(&changes), vec![module.clone()]);

    fs::write(&module, "export const edited = true;\n").unwrap();
    assert_eq!(next_change(&changes), vec![module]);
}

#[test]
fn an_unwatchable_module_failure_is_reported_only_once() {
    let workspace = Workspace::new();
    let (mut watcher, _changes) = workspace.watch();
    let module = PathBuf::from("unwatchable-relative-module.mjs");
    let failures = watcher.watch_modules([module.clone()]);
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].0, module);
    assert!(watcher.watch_modules([module]).is_empty());
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
fn refreshing_configuration_watches_a_retargeted_extended_symlink() {
    let workspace = Workspace::new();
    workspace.write("app/tsconfig.json", r#"{ "extends": "../shared/base.json" }"#);
    let original = workspace.write("original/base.json", "{}\n");
    let replacement = workspace.write("replacement/base.json", "{}\n");
    fs::create_dir(workspace.path("shared")).unwrap();
    let link = workspace.path("shared/base.json");
    symlink(&original, &link).unwrap();
    let (mut watcher, changes) = workspace.watch();
    fs::remove_file(&link).unwrap();
    symlink(&replacement, &link).unwrap();

    assert!(watcher.watch_configuration().is_empty());
    thread::sleep(SETTLE);
    while changes.try_recv().is_ok() {}
    fs::write(&replacement, r#"{ "compilerOptions": { "strict": true } }"#).unwrap();

    assert_eq!(next_change(&changes), vec![replacement]);
}

#[test]
fn proposal_0001_missing_extended_configuration_candidates_are_retained() {
    let workspace = Workspace::new();
    for (extended, expected) in [
        ("../configs/base.json", "configs/base.json"),
        ("../configs/deep/base.json", "configs/deep/base.json"),
        ("../configs/base", "configs/base.json"),
    ] {
        workspace.write("app/tsconfig.json", &format!(r#"{{ "extends": "{extended}" }}"#));

        let candidates = super::configuration::files(&workspace.root());

        assert!(candidates.contains(&workspace.path(expected)), "{candidates:?}");
    }
}

#[test]
fn proposal_0001_creating_a_missing_configuration_outside_the_project_is_a_change() {
    let workspace = Workspace::new();
    workspace.write("app/tsconfig.json", r#"{ "extends": "../configs/base.json" }"#);
    fs::create_dir(workspace.path("configs")).unwrap();
    let (_watcher, changes) = workspace.watch();

    let configuration = workspace.write("configs/base.json", "{}\n");

    assert_eq!(next_change(&changes), vec![configuration]);
}

#[test]
fn proposal_0001_creating_missing_configuration_directories_is_a_change() {
    let workspace = Workspace::new();
    workspace.write("app/tsconfig.json", r#"{ "extends": "../configs/deep/base.json" }"#);
    let (_watcher, changes) = workspace.watch();

    let configuration = workspace.write("configs/deep/base.json", "{}\n");

    assert_eq!(next_change(&changes), vec![configuration]);
}

#[test]
fn proposal_0001_missing_configuration_packages_do_not_watch_project_ancestors() {
    let workspace = Workspace::new();
    workspace.write("app/tsconfig.json", r#"{ "extends": "@configs/strict/tsconfig.json" }"#);

    let candidates = super::configuration::files(&workspace.root());
    assert!(candidates.iter().all(|path| path.starts_with(workspace.root())), "{candidates:?}");
    let (watcher, _changes) = workspace.watch();
    assert!(watcher.directories_outside_project.is_empty());
}

#[test]
fn proposal_0001_installing_a_missing_configuration_package_is_a_change() {
    let workspace = Workspace::new();
    workspace.write("app/tsconfig.json", r#"{ "extends": "@configs/strict/tsconfig.json" }"#);
    let (_watcher, changes) = workspace.watch();

    workspace.write("app/node_modules/@configs/strict/tsconfig.json", "{}\n");

    assert!(!next_change(&changes).is_empty());
}

#[test]
fn proposal_0001_a_missing_package_extended_by_an_outside_configuration_stays_watched() {
    let workspace = Workspace::new();
    workspace.write("app/tsconfig.json", r#"{ "extends": "../shared/base.json" }"#);
    workspace.write("shared/base.json", r#"{ "extends": "@configs/strict/tsconfig.json" }"#);
    let (_watcher, changes) = workspace.watch();

    workspace.write("shared/node_modules/@configs/strict/tsconfig.json", "{}\n");

    assert!(!next_change(&changes).is_empty());
}

#[test]
fn proposal_0001_creating_a_missing_configuration_through_a_directory_symlink_is_a_change() {
    let workspace = Workspace::new();
    workspace.write("app/tsconfig.json", r#"{ "extends": "../shared/deep/base.json" }"#);
    fs::create_dir(workspace.path("real")).unwrap();
    symlink(workspace.path("real"), workspace.path("shared")).unwrap();
    let (_watcher, changes) = workspace.watch();

    let configuration = workspace.write("real/deep/base.json", "{}\n");

    assert_eq!(next_change(&changes), vec![configuration]);
}

#[test]
fn a_missing_module_through_a_directory_symlink_is_a_change() {
    let workspace = Workspace::new();
    fs::create_dir(workspace.path("real")).unwrap();
    symlink(workspace.path("real"), workspace.path("shared")).unwrap();
    let (mut watcher, changes) = workspace.watch();
    assert!(watcher.watch_modules([workspace.path("shared/deep/module.mjs")]).is_empty());

    let module = workspace.write("real/deep/module.mjs", "export {};\n");

    assert_eq!(next_change(&changes), vec![module]);
}

#[test]
fn a_loaded_dependency_inside_the_project_is_not_watched() {
    let workspace = Workspace::new();
    let dependency = workspace.write("app/node_modules/dependency/index.js", "export {};\n");
    let (mut watcher, changes) = workspace.watch();
    assert!(watcher.watch_modules([dependency.clone()]).is_empty());

    fs::write(dependency, "export const edited = true;\n").unwrap();

    assert_quiet(&changes);
}

#[test]
fn dropping_the_watcher_stops_watching() {
    let workspace = Workspace::new();
    let (watcher, changes) = workspace.watch();

    drop(watcher);
    workspace.write("app/message.mjs", "export default 'unwatched';\n");

    assert_eq!(changes.recv_timeout(CHANGE_DEADLINE), Err(RecvTimeoutError::Disconnected));
}
