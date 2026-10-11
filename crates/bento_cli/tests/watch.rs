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
