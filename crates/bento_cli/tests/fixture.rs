//! The guest-book fixture run through the `bento` command, as its README describes.

mod support;
#[path = "fixture/watch_steps.rs"]
mod watch_steps;

use std::process::Command;
use std::time::Duration;

use bento_test_support::ports::{loopback, port_is_taken};
use support::guest_book::{GuestBook, README_GRANTS, fixture_file, run_with_readme_grants};
use support::guest_book_page::{
    assert_development_assets, assert_guest_book_page, assert_production_assets, assert_signing,
    first_page,
};
use support::running::Background;
use support::{SYSTEM_PATH, path_without_other_runtimes, stderr, stdout};

const SERVER_PORT: u16 = 17902;
const DENIED_ADDONS_PORT: u16 = 17903;
const EMBEDDED_PORT: u16 = 17904;
const WATCH_PORT: u16 = 17905;
const ENVIRONMENT_FILE_PORT: u16 = 17906;
const PROJECT_READS_PORT: u16 = 17907;

const DATABASE: &str = "db/test.db";
const SEEDED_ENTRY: [&str; 2] = ["Seeded visitor", "A seeded greeting"];
const STOP_DEADLINE: Duration = Duration::from_secs(15);

/// A copy whose migrated database at `DATABASE` holds `SEEDED_ENTRY`.
fn seeded_guest_book() -> GuestBook {
    let guest_book = GuestBook::install();
    guest_book.migrate(DATABASE);
    guest_book.add_entry(DATABASE, SEEDED_ENTRY[0], SEEDED_ENTRY[1]);
    guest_book
}

/// `command` serving on `port` from the database at `DATABASE`.
fn serving(guest_book: &GuestBook, mut command: Command, port: u16) -> Background {
    command.env("PORT", port.to_string()).env("DATABASE_URL", guest_book.path(DATABASE));
    Background::spawn(command)
}

/// Interrupts `bento` and returns its exit code once it has exited and freed `port`.
fn interrupt(bento: &mut Background, port: u16) -> Option<i32> {
    bento.signal(libc::SIGINT);
    let code = bento.wait(STOP_DEADLINE).code();
    assert!(!port_is_taken(loopback(port)), "port {port} is still bound");
    code
}

/// The fixture locks pnpm 12.11.2 as its package manager, and pnpm records the version that
/// installed `node_modules`, so the install used the pinned pnpm whatever `pnpm` is on `PATH`.
#[test]
fn a_frozen_install_adds_dependencies_and_dev_tools_and_keeps_the_lockfile() {
    let guest_book = GuestBook::install();

    for installed in ["node_modules/remix", "node_modules/.bin/oxfmt", "node_modules/.bin/tsc"] {
        assert!(guest_book.path(installed).exists(), "{installed} is missing");
    }
    assert_eq!(guest_book.read("pnpm-lock.yaml"), fixture_file("pnpm-lock.yaml"));
    let modules: serde_json::Value =
        serde_json::from_str(&guest_book.read("node_modules/.modules.yaml")).unwrap();
    assert_eq!(modules["packageManager"], "pnpm@12.11.2");
}

/// Without `NODE_ENV=development`, `server.ts` serves production assets. Interrupting it
/// reaches the server's own SIGINT listener, which closes it and exits 0, and so does `bento`.
#[test]
fn server_ts_serves_the_guest_book_with_no_other_runtime_on_path() {
    let guest_book = seeded_guest_book();
    let command =
        guest_book.bento(&run_with_readme_grants(&["server.ts"]), &path_without_other_runtimes());
    let mut bento = serving(&guest_book, command, SERVER_PORT);

    let page = first_page(SERVER_PORT);

    assert_guest_book_page(&page, &SEEDED_ENTRY);
    assert_production_assets(SERVER_PORT, &page.body);
    assert_signing(SERVER_PORT);
    bento.wait_for_stdout(&format!("Server running at http://localhost:{SERVER_PORT}"), 1);
    assert_eq!(interrupt(&mut bento, SERVER_PORT), Some(0), "{}", bento.stderr.text());
}

/// `bento run server.ts` with the README's grants, except that the grant of `kind` is
/// `replacement`, or left out.
fn server_ts_with<'a>(kind: &str, replacement: Option<&'a str>) -> Vec<&'a str> {
    let grants = README_GRANTS
        .iter()
        .filter_map(|grant| if grant.starts_with(kind) { replacement } else { Some(*grant) });
    ["run"].into_iter().chain(grants).chain(["server.ts"]).collect()
}

/// Runs `arguments` in `guest_book`, expects startup to fail, and returns the report.
fn failed_startup(guest_book: &GuestBook, arguments: &[&str], port: u16) -> String {
    let mut command = guest_book.bento(arguments, SYSTEM_PATH);
    command.env("PORT", port.to_string()).env("DATABASE_URL", guest_book.path(DATABASE));

    let output = command.output().unwrap();

    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    assert!(stdout(&output).is_empty(), "{}", stdout(&output));
    stderr(&output)
}

#[test]
fn without_the_addon_grant_startup_is_a_permission_denial_naming_the_addon() {
    let guest_book = seeded_guest_book();
    let arguments = server_ts_with("--allow-addons", None);

    let report = failed_startup(&guest_book, &arguments, DENIED_ADDONS_PORT);

    assert!(report.contains("permission denied"), "{report}");
    assert!(report.contains(".node"), "the denial does not name the addon: {report}");
}

/// The asset pipeline looks for optional packages in every ancestor's `node_modules`.
#[test]
fn reading_only_the_project_denies_the_asset_pipeline_s_package_search() {
    let guest_book = seeded_guest_book();
    let arguments = server_ts_with("--allow-read", Some("--allow-read=."));

    let report = failed_startup(&guest_book, &arguments, PROJECT_READS_PORT);

    assert!(report.contains("permission denied"), "{report}");
    assert!(report.contains("node_modules"), "{report}");
}

#[test]
fn the_stock_cli_cannot_resolve_the_embedding_host_s_module() {
    let guest_book = GuestBook::install();
    let arguments = run_with_readme_grants(&["server.embedded.ts"]);

    let report = failed_startup(&guest_book, &arguments, EMBEDDED_PORT);

    assert!(report.contains("a module could not be resolved"), "{report}");
    assert!(report.contains("host:guestbook"), "{report}");
}

/// With neither a `NODE_ENV` nor any environment file, the watch launch still serves
/// development assets.
#[test]
fn watching_server_ts_restarts_the_server_when_its_source_changes() {
    let guest_book = GuestBook::install();
    guest_book.migrate(DATABASE);
    let command = guest_book.bento(&run_with_readme_grants(&["--watch", "server.ts"]), SYSTEM_PATH);
    let mut bento = serving(&guest_book, command, WATCH_PORT);
    let page = first_page(WATCH_PORT);
    assert_guest_book_page(&page, &[]);
    assert_development_assets(WATCH_PORT, &page.body);

    watch_steps::sign_without_restarting(&bento, WATCH_PORT);
    watch_steps::edit_server_source(&guest_book, &bento, WATCH_PORT);
    watch_steps::edit_browser_source(&guest_book, &bento, WATCH_PORT);
    watch_steps::break_and_repair(&guest_book, &bento, WATCH_PORT);

    assert_eq!(interrupt(&mut bento, WATCH_PORT), Some(0), "{}", bento.stderr.text());
}

/// The watch launch selects the `development` mode, so it reads `.env.development` after
/// `.env`: it serves on the `.env` port from the database `.env.development` names.
#[test]
fn watching_server_ts_serves_on_the_env_port_from_the_development_file_s_database() {
    let guest_book = GuestBook::install();
    guest_book.write(".env", &format!("PORT={ENVIRONMENT_FILE_PORT}\n"));
    guest_book.write(".env.development", "DATABASE_URL=db/development.db\n");
    guest_book.migrate("db/development.db");
    guest_book.add_entry("db/development.db", "Development visitor", "From the mode file");
    let arguments = run_with_readme_grants(&["--watch", "server.ts"]);
    let mut bento = Background::spawn(guest_book.bento(&arguments, SYSTEM_PATH));

    let page = first_page(ENVIRONMENT_FILE_PORT);

    assert_guest_book_page(&page, &["Development visitor", "From the mode file"]);
    assert!(!page.body.contains(".@"), "fingerprinted assets in development: {}", page.body);
    assert_eq!(interrupt(&mut bento, ENVIRONMENT_FILE_PORT), Some(0));
}

/// The fixture coordinates its own workflow with Mise, which runs the package tools from
/// `node_modules/.bin` under Node. `db:migrate` creates the database the app defaults to.
#[test]
fn the_fixture_s_mise_tasks_check_and_migrate_the_installed_copy() {
    let guest_book = GuestBook::install();

    let checked = guest_book.mise(&["run", "check"]).output().unwrap();
    let migrated = guest_book.mise(&["run", "db:migrate"]).output().unwrap();

    assert!(checked.status.success(), "{}{}", stdout(&checked), stderr(&checked));
    assert!(migrated.status.success(), "{}{}", stdout(&migrated), stderr(&migrated));
    assert!(guest_book.path("db/data.db").exists(), "db:migrate created no database");
}
