mod support;

use std::fs;
use std::thread;
use std::time::{Duration, Instant};

use serde_json::json;

use support::TempProject;
use support::running::{Background, free_port, get, loopback, port_is_taken, wait_until};

const GRANTS: [&str; 3] = ["--allow-read=.", "--allow-net", "--allow-env=PORT"];
const RESPONSE_DEADLINE: Duration = Duration::from_secs(15);
/// Long enough for a change to have caused a restart, had it caused one.
const SETTLE: Duration = Duration::from_secs(1);

/// `server.mjs` serves the message it imports from `message.mjs`, and prints `listening`
/// each time it starts.
const SERVER: &str = "import http from 'node:http';\n\
import { message } from './message.mjs';\n\
const server = http.createServer((request, response) => response.end(message));\n\
server.listen(Number(process.env.PORT), '127.0.0.1', () => console.log('listening'));\n";

fn message_module(message: &str) -> String {
    format!("export const message = '{message}';\n")
}

fn server_project(message: &str) -> TempProject {
    let temp = TempProject::new(json!({ "type": "module" }));
    temp.write("server.mjs", SERVER);
    temp.write("message.mjs", &message_module(message));
    temp
}

/// `bento run --watch` with the server's grants and `PORT`, started in `temp`'s root.
fn watch(temp: &TempProject, port: u16, script: &str) -> Background {
    let mut command = temp.bento(&[&["run", "--watch"], &GRANTS[..], &[script]].concat());
    command.env("PORT", port.to_string());
    Background::spawn(command)
}

fn wait_for_response(port: u16, expected: &str) {
    wait_until(RESPONSE_DEADLINE, || get(port).is_some_and(|response| response.contains(expected)));
}

#[test]
fn editing_an_imported_module_restarts_the_server_with_the_new_code() {
    let temp = server_project("message one");
    let port = free_port();
    let bento = watch(&temp, port, "server.mjs");
    wait_for_response(port, "message one");

    temp.write("message.mjs", &message_module("message two"));

    wait_for_response(port, "message two");
    assert_eq!(bento.stdout.count("listening"), 2);
    assert!(bento.stderr.text().contains("message.mjs"), "{}", bento.stderr.text());
}

#[test]
fn deleting_an_imported_module_reports_the_error_keeps_watching_and_recreating_it_recovers() {
    let temp = server_project("message one");
    let port = free_port();
    let mut bento = watch(&temp, port, "server.mjs");
    wait_for_response(port, "message one");

    fs::remove_file(temp.path("message.mjs")).unwrap();
    bento.wait_for_stderr("waiting for changes");

    assert!(bento.is_running());
    assert!(get(port).is_none(), "the old code is still served");
    assert!(bento.stderr.text().contains("message.mjs"), "{}", bento.stderr.text());
    temp.write("message.mjs", &message_module("message three"));
    wait_for_response(port, "message three");
}

#[test]
fn a_failed_first_load_keeps_watching_until_the_missing_module_is_created() {
    let temp = TempProject::new(json!({ "type": "module" }));
    temp.write("server.mjs", SERVER);
    let port = free_port();
    let mut bento = watch(&temp, port, "server.mjs");
    bento.wait_for_stderr("waiting for changes");
    assert!(bento.is_running());

    temp.write("message.mjs", &message_module("created"));

    wait_for_response(port, "created");
}

#[test]
fn creating_a_missing_import_outside_the_project_recovers_watch() {
    let temp = TempProject::new(json!({}));
    temp.write("app/package.json", &json!({ "type": "module" }).to_string());
    temp.write("app/server.mjs", &SERVER.replace("./message.mjs", "../sibling/missing.mjs"));
    temp.write("sibling/keep", "");
    let port = free_port();
    let mut bento = watch(&temp, port, "app/server.mjs");
    bento.wait_for_stderr("waiting for changes");
    assert!(bento.is_running());
    temp.write("sibling/missing.mjs", &message_module("repaired outside"));
    wait_for_response(port, "repaired outside");
}

#[test]
fn a_burst_of_edits_restarts_the_server_once() {
    let temp = server_project("edit 0");
    let port = free_port();
    let bento = watch(&temp, port, "server.mjs");
    wait_for_response(port, "edit 0");

    for edit in 1..=5 {
        temp.write("message.mjs", &message_module(&format!("edit {edit}")));
        thread::sleep(Duration::from_millis(20));
    }

    wait_for_response(port, "edit 5");
    thread::sleep(SETTLE);
    assert_eq!(bento.stdout.count("listening"), 2, "{}", bento.stderr.text());
}

#[test]
fn writing_a_database_and_its_sqlite_side_files_does_not_restart_the_server() {
    let temp = server_project("message one");
    let port = free_port();
    let bento = watch(&temp, port, "server.mjs");
    wait_for_response(port, "message one");

    for database_file in ["guests.db", "guests.db-wal", "guests.db-shm", "guests.db-journal"] {
        temp.write(database_file, "rows");
    }

    assert_eq!(bento.stdout.count("listening"), 1, "{}", bento.stderr.text());
}

#[test]
fn interrupt_stops_the_watcher_and_the_server() {
    let temp = server_project("message one");
    let port = free_port();
    let mut bento = watch(&temp, port, "server.mjs");
    wait_for_response(port, "message one");

    bento.signal(libc::SIGINT);

    assert_eq!(bento.wait(Duration::from_secs(10)).code(), Some(130));
    assert!(!port_is_taken(loopback(port)), "the server's port is still bound");
}

#[test]
fn a_module_first_imported_after_startup_is_watched() {
    let temp = TempProject::new(json!({}));
    temp.write("app/package.json", &json!({ "type": "module" }).to_string());
    temp.write(
        "app/server.mjs",
        "import http from 'node:http';\n\
         const server = http.createServer(async (request, response) => {\n\
           const { message } = await import('../shared/late.mjs');\n\
           response.end(message);\n\
         });\n\
         server.listen(Number(process.env.PORT), '127.0.0.1', () => console.log('listening'));\n",
    );
    temp.write("shared/late.mjs", &message_module("late one"));
    let port = free_port();
    let bento = watch(&temp, port, "app/server.mjs");
    wait_for_response(port, "late one");
    // Bento asks the runtime for newly loaded modules a few times a second.
    thread::sleep(SETTLE);

    temp.write("shared/late.mjs", &message_module("late two"));

    wait_for_response(port, "late two");
    assert_eq!(bento.stdout.count("listening"), 2);
}

#[test]
fn a_script_that_finishes_keeps_watching_in_development_mode() {
    let temp = TempProject::new(json!({ "type": "module" }));
    temp.write("mode.mjs", "console.log(`mode ${process.env.NODE_ENV}`);\n");
    let mut command =
        temp.bento(&["run", "--watch", "--allow-read=.", "--allow-env=NODE_ENV", "mode.mjs"]);
    command.env_remove("NODE_ENV");
    let mut bento = Background::spawn(command);
    bento.wait_for_stdout("mode development", 1);
    bento.wait_for_stderr("waiting for changes");

    temp.write("mode.mjs", "console.log(`again ${process.env.NODE_ENV}`);\n");
    bento.wait_for_stdout("again development", 1);
    // A script that ends by itself after the interrupt reached it ends `bento` with its own
    // code, so the interrupt waits until no script runs.
    wait_until(Duration::from_secs(5), || bento.stderr.count("watch: mode.mjs finished") == 2);
    bento.signal(libc::SIGINT);

    assert_eq!(bento.wait(Duration::from_secs(5)).code(), Some(130));
}

#[test]
fn interrupt_to_a_task_stops_the_watch_server_its_nested_bento_runs() {
    let temp = server_project("nested message");
    let manifest = json!({ "type": "module", "tasks": { "dev": "bento run --watch server.mjs" } });
    temp.write("package.json", &manifest.to_string());
    let port = free_port();
    let mut command = temp.bento(&[&["run"], &GRANTS[..], &["dev"]].concat());
    command.env("PORT", port.to_string());
    let mut bento = Background::spawn(command);
    wait_for_response(port, "nested message");

    bento.signal(libc::SIGINT);

    assert_eq!(bento.wait(Duration::from_secs(20)).code(), Some(130));
    assert!(!port_is_taken(loopback(port)), "the nested server's port is still bound");
}

#[test]
fn interrupt_stops_a_watched_script_still_running_at_the_deadline() {
    let temp = server_project("message one");
    let stuck = "process.on('SIGINT', () => console.log('never runs'));\n\
                 setTimeout(() => { console.log('spinning'); for (;;) {} }, 100);\n";
    temp.write("server.mjs", &format!("{SERVER}{stuck}"));
    let port = free_port();
    let mut bento = watch(&temp, port, "server.mjs");
    bento.wait_for_stdout("spinning", 1);
    let interrupted = Instant::now();

    bento.signal(libc::SIGINT);

    assert_eq!(bento.wait(Duration::from_secs(15)).code(), Some(130));
    let elapsed = interrupted.elapsed();
    assert!(elapsed >= Duration::from_secs(5), "stopped {elapsed:?} after the signal");
    assert!(elapsed < Duration::from_secs(6), "stopped {elapsed:?} after the signal");
    assert!(!port_is_taken(loopback(port)), "the server's port is still bound");
}

#[test]
fn a_file_tsconfig_begins_to_extend_after_an_edit_is_watched() {
    let temp = TempProject::new(json!({}));
    temp.write("app/package.json", &json!({ "type": "module" }).to_string());
    temp.write("app/server.mjs", SERVER);
    temp.write("app/message.mjs", &message_module("configured"));
    temp.write("app/tsconfig.json", "{}\n");
    temp.write("shared/tsconfig.base.json", r#"{ "compilerOptions": { "strict": true } }"#);
    let port = free_port();
    let bento = watch(&temp, port, "app/server.mjs");
    wait_for_response(port, "configured");

    temp.write("app/tsconfig.json", r#"{ "extends": "../shared/tsconfig.base.json" }"#);
    bento.wait_for_stdout("listening", 2);
    temp.write("shared/tsconfig.base.json", r#"{ "compilerOptions": { "strict": false } }"#);

    bento.wait_for_stdout("listening", 3);
}

fn recover_missing_configuration(extended: &str, create_parent: bool) {
    let temp = TempProject::new(json!({}));
    temp.write("app/package.json", &json!({ "type": "module" }).to_string());
    temp.write("app/tsconfig.json", &json!({ "extends": extended }).to_string());
    temp.write("app/main.ts", "console.log('configuration recovered');\n");
    let configuration = temp.path("app").join(extended);
    if create_parent {
        fs::create_dir_all(configuration.parent().unwrap()).unwrap();
    }
    let mut command =
        support::bento_in(&temp.path("app"), &["run", "--allow-read", "--watch", "main.ts"]);
    command.env_remove("NODE_PATH");
    let mut bento = Background::spawn(command);
    bento.wait_for_stderr("waiting for changes");
    assert!(bento.is_running());
    assert!(bento.stderr.text().contains("base.json"), "{}", bento.stderr.text());

    fs::create_dir_all(configuration.parent().unwrap()).unwrap();
    fs::write(configuration, "{}\n").unwrap();

    bento.wait_for_stdout("configuration recovered", 1);
}

#[test]
fn proposal_0001_creating_a_missing_extended_configuration_recovers_watch() {
    recover_missing_configuration("../configs/base.json", true);
}

#[test]
fn proposal_0001_creating_missing_configuration_parent_directories_recovers_watch() {
    recover_missing_configuration("../configs/deep/base.json", false);
}

#[test]
fn proposal_0001_installing_a_missing_configuration_package_recovers_watch() {
    let temp = TempProject::new(json!({ "type": "module" }));
    temp.write("tsconfig.json", &json!({ "extends": "@configs/strict/tsconfig.json" }).to_string());
    temp.write("main.ts", "console.log('package configuration recovered');\n");
    let mut command = temp.bento(&["run", "--allow-read", "--watch", "main.ts"]);
    command.env_remove("NODE_PATH");
    let bento = Background::spawn(command);
    bento.wait_for_stderr("waiting for changes");

    temp.write("node_modules/@configs/strict/tsconfig.json", "{}\n");

    bento.wait_for_stdout("package configuration recovered", 1);
}

#[test]
fn proposal_0001_creating_a_configuration_through_a_directory_symlink_recovers_watch() {
    let temp = TempProject::new(json!({}));
    temp.write("app/package.json", &json!({ "type": "module" }).to_string());
    temp.write("app/tsconfig.json", &json!({ "extends": "../shared/deep/base.json" }).to_string());
    temp.write("app/main.ts", "console.log('symlink configuration recovered');\n");
    fs::create_dir(temp.path("real")).unwrap();
    std::os::unix::fs::symlink(temp.path("real"), temp.path("shared")).unwrap();
    let command =
        support::bento_in(&temp.path("app"), &["run", "--allow-read", "--watch", "main.ts"]);
    let bento = Background::spawn(command);
    bento.wait_for_stderr("waiting for changes");

    temp.write("real/deep/base.json", "{}\n");

    bento.wait_for_stdout("symlink configuration recovered", 1);
}

#[test]
fn an_unwatchable_configuration_warns_once_and_retries_after_source_changes() {
    let temp = TempProject::new(json!({}));
    temp.write("app/package.json", &json!({ "type": "module" }).to_string());
    temp.write("app/tsconfig.json", &json!({ "extends": "../loop/base.json" }).to_string());
    temp.write("app/main.ts", "console.log('unwatchable configuration recovered');\n");
    std::os::unix::fs::symlink("loop", temp.path("loop")).unwrap();
    let command =
        support::bento_in(&temp.path("app"), &["run", "--allow-read", "--watch", "main.ts"]);
    let bento = Background::spawn(command);
    bento.wait_for_stderr("waiting for changes");
    let configuration = temp.path("").canonicalize().unwrap().join("loop/base.json");
    let configuration = configuration.display();
    let warning = format!(
        "warning: watch: cannot watch {configuration} for changes yet, so Bento will keep trying:"
    );
    let warning_count =
        || bento.stderr.all().iter().filter(|line| line.starts_with(&warning)).count();
    assert_eq!(warning_count(), 1, "{}", bento.stderr.text());

    temp.write("app/main.ts", "console.log('retry configuration');\n");
    wait_until(RESPONSE_DEADLINE, || {
        bento.stderr.count("watch: waiting for changes before restarting") >= 2
    });
    assert_eq!(warning_count(), 1, "{}", bento.stderr.text());
    fs::remove_file(temp.path("loop")).unwrap();
    temp.write("loop/base.json", "{}\n");
    temp.write("app/main.ts", "console.log('unwatchable configuration recovered');\n");

    bento.wait_for_stdout("unwatchable configuration recovered", 1);
    assert_eq!(warning_count(), 1, "{}", bento.stderr.text());
}

#[test]
fn proposal_0001_a_loaded_entry_beneath_a_node_modules_ancestor_is_watched() {
    let temp = TempProject::new(json!({}));
    let project = temp.path("node_modules/application");
    temp.write("node_modules/application/package.json", &json!({ "type": "module" }).to_string());
    temp.write("node_modules/application/dist/main.mjs", "console.log('version one');\n");
    let command = support::bento_in(&project, &["run", "--allow-read", "--watch", "dist/main.mjs"]);
    let bento = Background::spawn(command);
    bento.wait_for_stdout("version one", 1);
    bento.wait_for_stderr("waiting for changes");

    temp.write("node_modules/application/dist/main.mjs", "console.log('version two');\n");

    bento.wait_for_stdout("version two", 1);
}

/// A module loaded before its directory moves remains watched through the nearest existing ancestor.
#[test]
fn a_loaded_module_stays_watched_when_its_directory_is_moved_and_restored() {
    let temp = TempProject::new(json!({}));
    temp.write("app/package.json", &json!({ "type": "module" }).to_string());
    temp.write(
        "app/server.mjs",
        "import fs from 'node:fs';\n\
         import http from 'node:http';\n\
         const server = http.createServer(async (request, response) => {\n\
           const { message } = await import('../shared/message.mjs');\n\
           if (!fs.existsSync('moved-once')) {\n\
             fs.renameSync('shared', 'moved');\n\
             fs.writeFileSync('moved-once', '');\n\
           }\n\
           response.end(message);\n\
         });\n\
         server.listen(Number(process.env.PORT), '127.0.0.1', () => console.log('listening'));\n",
    );
    temp.write("shared/message.mjs", &message_module("shared one"));
    let port = free_port();
    let grants = ["--allow-read=.", "--allow-write=.", "--allow-net", "--allow-env=PORT"];
    let mut command = temp.bento(&[&["run", "--watch"], &grants[..], &["app/server.mjs"]].concat());
    command.env("PORT", port.to_string());
    let bento = Background::spawn(command);
    wait_for_response(port, "shared one");
    thread::sleep(SETTLE);
    let warnings = bento.stderr.all().into_iter().filter(|line| line.contains("cannot watch"));
    assert_eq!(warnings.count(), 0, "{}", bento.stderr.text());

    fs::rename(temp.path("moved"), temp.path("shared")).unwrap();
    thread::sleep(SETTLE);
    temp.write("shared/message.mjs", &message_module("shared two"));

    wait_for_response(port, "shared two");
}

fn watch_external_script(temp: &TempProject, entry: &str) -> Background {
    Background::spawn(support::bento_in(
        &temp.path("app"),
        &["run", "--allow-read", "--watch", entry],
    ))
}

#[test]
fn proposal_0001_failed_commonjs_resolutions_recover_when_external_candidates_are_created() {
    for (entry, source) in [
        ("main.cjs", "console.log(require('../shared/deep/missing').message);"),
        (
            "main.cjs",
            "if (require.resolve('../shared/deep/missing').endsWith('missing.js')) console.log('repaired commonjs');",
        ),
    ] {
        let temp = TempProject::new(json!({}));
        temp.write("app/package.json", "{}");
        temp.write(&format!("app/{entry}"), source);
        let bento = watch_external_script(&temp, entry);
        bento.wait_for_stderr("waiting for changes");
        temp.write("shared/deep/missing.js", "exports.message = 'repaired commonjs';");
        bento.wait_for_stdout("repaired commonjs", 1);
    }
}

#[test]
fn proposal_0001_dependency_wrappers_preserve_missing_require_recovery() {
    for hook in ["Module.prototype.require", "Module._resolveFilename"] {
        let temp = TempProject::new(json!({}));
        temp.write("app/package.json", "{}");
        temp.write(
            "app/node_modules/instrumentation/package.json",
            r#"{"name":"instrumentation","main":"index.cjs"}"#,
        );
        temp.write(
            "app/node_modules/instrumentation/index.cjs",
            &format!("const Module = require('node:module'); const original = {hook}; {hook} = function (...args) {{ return original.apply(this, args); }};"),
        );
        temp.write(
            "app/main.cjs",
            "require('instrumentation'); console.log(require('../shared/missing.cjs').message);",
        );
        let bento = watch_external_script(&temp, "main.cjs");
        bento.wait_for_stderr("waiting for changes");
        temp.write("shared/missing.cjs", "exports.message = 'repaired wrapped require';");
        bento.wait_for_stdout("repaired wrapped require", 1);
    }
}

fn workspace_manifest_reloads(
    entry: &str,
    source: &str,
    shared: &str,
    manifest: serde_json::Value,
) {
    let temp = TempProject::new(json!({}));
    temp.write("app/package.json", &json!({ "type": "module" }).to_string());
    temp.write(&format!("app/{entry}"), source);
    temp.write("shared/package.json", &manifest.to_string());
    temp.write("shared/value.js", shared);
    temp.write("shared/one.cjs", "exports.message = 'workspace one';");
    temp.write("shared/two.cjs", "exports.message = 'workspace two';");
    let bento = watch_external_script(&temp, entry);
    bento.wait_for_stdout("workspace one", 1);
    bento.wait_for_stderr("waiting for changes");

    temp.write("shared/package.json", &manifest.to_string().replace("one.cjs", "two.cjs"));

    bento.wait_for_stdout("workspace two", 1);
}

#[test]
fn proposal_0001_workspace_import_maps_are_watched_for_both_module_systems() {
    workspace_manifest_reloads(
        "main.mjs",
        "import { message } from '../shared/value.js'; console.log(message);",
        "export { message } from '#message';",
        json!({ "type": "module", "imports": { "#message": "./one.cjs" } }),
    );
    workspace_manifest_reloads(
        "main.cjs",
        "console.log(require('../shared/value.js').message);",
        "exports.message = require('#message').message;",
        json!({ "type": "commonjs", "imports": { "#message": "./one.cjs" } }),
    );
}

#[test]
fn proposal_0001_workspace_self_reference_exports_are_watched() {
    workspace_manifest_reloads(
        "main.mjs",
        "import { message } from '../shared/value.js'; console.log(message);",
        "export { message } from 'workspace/message';",
        json!({ "name": "workspace", "type": "module", "exports": { "./message": "./one.cjs" } }),
    );
}

#[test]
fn proposal_0001_creating_a_nearer_workspace_package_scope_restarts_resolution() {
    let temp = TempProject::new(json!({}));
    temp.write("app/package.json", &json!({ "type": "module" }).to_string());
    temp.write(
        "app/main.mjs",
        "import { message } from '../shared/nested/value.mjs'; console.log(message);",
    );
    temp.write(
        "shared/package.json",
        &json!({ "imports": { "#message": "./one.cjs" } }).to_string(),
    );
    temp.write("shared/nested/value.mjs", "export { message } from '#message';");
    temp.write("shared/one.cjs", "exports.message = 'outer scope';");
    temp.write("shared/nested/two.cjs", "exports.message = 'nearer scope';");
    let bento = watch_external_script(&temp, "main.mjs");
    bento.wait_for_stdout("outer scope", 1);
    bento.wait_for_stderr("waiting for changes");

    temp.write(
        "shared/nested/package.json",
        &json!({ "imports": { "#message": "./two.cjs" } }).to_string(),
    );

    bento.wait_for_stdout("nearer scope", 1);
}

#[test]
fn proposal_0001_creating_a_workspace_package_scope_recovers_a_failed_import_map() {
    let temp = TempProject::new(json!({}));
    temp.write("app/package.json", &json!({ "type": "module" }).to_string());
    temp.write(
        "app/main.mjs",
        "import { message } from '../shared/value.mjs'; console.log(message);",
    );
    temp.write("shared/value.mjs", "export { message } from '#message';");
    temp.write("shared/message.cjs", "exports.message = 'created scope';");
    let bento = watch_external_script(&temp, "main.mjs");
    bento.wait_for_stderr("waiting for changes");

    temp.write(
        "shared/package.json",
        &json!({ "imports": { "#message": "./message.cjs" } }).to_string(),
    );

    bento.wait_for_stdout("created scope", 1);
}

#[test]
fn proposal_0001_workspace_package_type_is_watched() {
    let temp = TempProject::new(json!({}));
    temp.write("app/package.json", &json!({ "type": "module" }).to_string());
    temp.write("app/main.mjs", "import '../shared/value.js';");
    temp.write("shared/package.json", &json!({ "type": "module" }).to_string());
    temp.write("shared/value.js", "console.log('require is ' + typeof require);");
    let bento = watch_external_script(&temp, "main.mjs");
    bento.wait_for_stdout("require is undefined", 1);
    bento.wait_for_stderr("waiting for changes");

    temp.write("shared/package.json", &json!({ "type": "commonjs" }).to_string());

    bento.wait_for_stdout("require is function", 1);
}

#[test]
fn proposal_0001_successful_require_resolve_does_not_watch_workspace_sources() {
    let temp = TempProject::new(json!({}));
    temp.write("app/package.json", "{}");
    temp.write(
        "app/main.cjs",
        "require.resolve('../shared/value.cjs'); console.log('resolved source');",
    );
    temp.write("shared/value.cjs", "exports.message = 'one';");
    let bento = watch_external_script(&temp, "main.cjs");
    bento.wait_for_stdout("resolved source", 1);
    bento.wait_for_stderr("waiting for changes");
    thread::sleep(SETTLE);
    let runs = bento.stdout.count("resolved source");

    temp.write("shared/value.cjs", "exports.message = 'two';");

    thread::sleep(SETTLE);
    assert_eq!(bento.stdout.count("resolved source"), runs, "{}", bento.stderr.text());
}

#[test]
fn proposal_0001_missing_esm_package_map_targets_recover_in_watch() {
    for (entry, target, value) in [
        ("main.mjs", "target.mjs", "export { message } from '#message';"),
        ("main.mjs", "deep/target.mjs", "export { message } from '#message';"),
        ("main.mjs", "deep/target.mjs", "export { message } from 'workspace/message';"),
        (
            "main.mjs",
            "deep/target.mjs",
            "export const { message } = await import(import.meta.resolve('#message'));",
        ),
        ("main.cjs", "deep/target.mjs", "export { message } from '#message';"),
    ] {
        let temp = TempProject::new(json!({}));
        temp.write("app/package.json", &json!({ "type": "module" }).to_string());
        let source = if entry.ends_with(".cjs") {
            "import('../shared/value.mjs').then(({ message }) => console.log(message));"
        } else {
            "import { message } from '../shared/value.mjs'; console.log(message);"
        };
        temp.write(&format!("app/{entry}"), source);
        temp.write(
            "shared/package.json",
            &json!({
                "name": "workspace",
                "type": "module",
                "imports": { "#message": format!("./{target}") },
                "exports": { "./message": format!("./{target}") }
            })
            .to_string(),
        );
        temp.write("shared/value.mjs", value);
        let bento = watch_external_script(&temp, entry);
        bento.wait_for_stderr("waiting for changes");

        temp.write(&format!("shared/{target}"), &message_module("repaired esm package map"));

        bento.wait_for_stdout("repaired esm package map", 1);
    }
}

#[test]
fn proposal_0001_successful_import_meta_resolve_does_not_watch_workspace_sources() {
    let temp = TempProject::new(json!({}));
    temp.write("app/package.json", &json!({ "type": "module" }).to_string());
    temp.write(
        "app/main.mjs",
        "try { import.meta.resolve('../shared/value.mjs'); console.log('resolved input'); } catch { console.log('unresolvable input'); }",
    );
    temp.write("shared/value.mjs", "throw new Error('not loaded');");
    let bento = watch_external_script(&temp, "main.mjs");
    bento.wait_for_stdout("resolved input", 1);
    bento.wait_for_stderr("waiting for changes");
    thread::sleep(SETTLE);
    let runs = bento.stdout.count("resolved input");

    fs::remove_file(temp.path("shared/value.mjs")).unwrap();

    thread::sleep(SETTLE);
    assert_eq!(bento.stdout.count("resolved input"), runs, "{}", bento.stderr.text());
    assert_eq!(bento.stdout.count("unresolvable input"), 0);
}

#[test]
fn proposal_0001_successful_commonjs_directory_resolution_does_not_watch_sibling_probes() {
    let temp = TempProject::new(json!({}));
    temp.write("app/package.json", "{}");
    temp.write("app/main.cjs", "console.log(require('../shared').message);");
    temp.write("shared/index.js", "exports.message = 'directory source';");
    let bento = watch_external_script(&temp, "main.cjs");
    bento.wait_for_stdout("directory source", 1);
    bento.wait_for_stderr("waiting for changes");
    thread::sleep(SETTLE);
    let runs = bento.stdout.count("directory source");

    temp.write("shared.js", "exports.message = 'unloaded candidate';");

    thread::sleep(SETTLE);
    assert_eq!(bento.stdout.count("directory source"), runs, "{}", bento.stderr.text());
    assert_eq!(bento.stdout.count("unloaded candidate"), 0);
}

#[test]
fn proposal_0001_dependency_config_search_does_not_watch_the_project_parent() {
    let temp = TempProject::new(json!({}));
    temp.write("app/package.json", "{}");
    temp.write("app/main.cjs", "console.log(require('config-loader')(__dirname));");
    temp.write(
        "app/node_modules/config-loader/package.json",
        r#"{"name":"config-loader","main":"index.js"}"#,
    );
    temp.write(
        "app/node_modules/config-loader/index.js",
        "const path = require('node:path'); module.exports = (start) => { for (let dir = start; ; dir = path.dirname(dir)) { try { return require(require.resolve(path.join(dir, 'probe.config.js'))); } catch {} if (dir === path.dirname(dir)) return 'no configuration'; } };",
    );
    let bento = watch_external_script(&temp, "main.cjs");
    bento.wait_for_stdout("no configuration", 1);
    bento.wait_for_stderr("waiting for changes");
    thread::sleep(SETTLE);
    let runs = bento.stdout.count("no configuration");

    temp.write("probe.config.js", "module.exports = 'parent configuration';");

    thread::sleep(SETTLE);
    assert_eq!(bento.stdout.count("no configuration"), runs, "{}", bento.stderr.text());
    assert_eq!(bento.stdout.count("parent configuration"), 0);
}

#[test]
fn proposal_0001_missing_custom_require_extensions_recover_when_created() {
    let temp = TempProject::new(json!({}));
    temp.write("app/package.json", "{}");
    temp.write(
        "app/main.cjs",
        "require.extensions['.ts'] = (m, f) => { m.exports = require('node:fs').readFileSync(f, 'utf8'); }; console.log(require('../shared/missing'));",
    );
    let bento = watch_external_script(&temp, "main.cjs");
    bento.wait_for_stderr("waiting for changes");

    temp.write("shared/missing.ts", "repaired custom require");

    bento.wait_for_stdout("repaired custom require", 1);
}

#[test]
fn proposal_0001_creating_missing_worker_scripts_recovers_watch() {
    for node in [false, true] {
        for target in ["../shared/worker.mjs", "./dist/worker.mjs"] {
            let temp = TempProject::new(json!({}));
            temp.write("app/package.json", r#"{"type":"module"}"#);
            let source = if node {
                format!(
                    "import {{ Worker }} from 'node:worker_threads';\n\
                     const worker = new Worker(new URL('{target}', import.meta.url));\n\
                     worker.once('error', () => console.log('WORKER FAILED'));\n\
                     worker.once('message', message => {{ console.log(message); worker.terminate(); }});"
                )
            } else {
                format!(
                    "const worker = new Worker(new URL('{target}', import.meta.url), {{ type: 'module' }});\n\
                     worker.onerror = event => {{ event.preventDefault(); console.log('WORKER FAILED'); worker.terminate(); }};\n\
                     worker.onmessage = event => {{ console.log(event.data); worker.terminate(); }};"
                )
            };
            temp.write("app/main.mjs", &source);
            let worker = temp.path("app").join(target);
            fs::create_dir_all(worker.parent().unwrap()).unwrap();
            let bento = watch_external_script(&temp, "main.mjs");
            bento.wait_for_stdout("WORKER FAILED", 1);
            bento.wait_for_stderr("waiting for changes");

            let source = if node {
                "import { parentPort } from 'node:worker_threads'; parentPort.postMessage('WORKER RECOVERED');"
            } else {
                "postMessage('WORKER RECOVERED');"
            };
            fs::write(worker, source).unwrap();

            bento.wait_for_stdout("WORKER RECOVERED", 1);
            assert_eq!(bento.stdout.count("WORKER FAILED"), 1, "{}", bento.stderr.text());
        }
    }
}
