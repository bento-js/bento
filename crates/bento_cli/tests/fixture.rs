//! The guest-book fixture run through the `bento` command, as its README describes.

#[path = "fixture/dev_watch.rs"]
mod dev_watch;
mod support;

use std::process::Command;
use std::time::Duration;

use support::guest_book::{
    GuestBook, README_GRANTS, fixture_file, run_with_readme_grants, tool_path,
};
use support::guest_book_page::{
    assert_development_assets, assert_guest_book_page, assert_production_assets, assert_signing,
    first_page,
};
use support::running::{Background, loopback, port_is_taken};
use support::{SYSTEM_PATH, path_without_other_runtimes, stderr, stdout};

const START_PORT: u16 = 17901;
const SERVER_PORT: u16 = 17902;
const DENIED_ADDONS_PORT: u16 = 17903;
const EMBEDDED_PORT: u16 = 17904;
const DEV_PORT: u16 = 17905;
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

/// `command` serving on `port` from the seeded database.
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

#[test]
fn a_frozen_install_adds_dependencies_and_dev_tools_and_keeps_the_lockfile() {
    let guest_book = GuestBook::install();

    for installed in ["node_modules/remix", "node_modules/.bin/oxfmt", "node_modules/.bin/tsc"] {
        assert!(guest_book.path(installed).exists(), "{installed} is missing");
    }
    assert_eq!(guest_book.read("pnpm-lock.yaml"), fixture_file("pnpm-lock.yaml"));
}

/// `start` runs `bento run server.ts`, which serves production assets. Interrupting the task
/// reaches the server's own SIGINT listener, which closes it and exits 0, and so does `bento`.
#[test]
fn the_start_task_serves_the_guest_book_with_no_other_runtime_on_path() {
    let guest_book = seeded_guest_book();
    let command =
        guest_book.bento(&run_with_readme_grants("start"), &path_without_other_runtimes());
    let mut bento = serving(&guest_book, command, START_PORT);

    let page = first_page(START_PORT);

    assert_guest_book_page(&page, &SEEDED_ENTRY);
    assert_production_assets(START_PORT, &page.body);
    assert_signing(START_PORT);
    assert_eq!(interrupt(&mut bento, START_PORT), Some(0), "{}", bento.stderr.text());
    bento.wait_for_stderr("task `start` was cancelled by SIGINT");
}

#[test]
fn server_ts_runs_with_the_readme_grants_and_ends_with_its_own_exit_code() {
    let guest_book = seeded_guest_book();
    let command = guest_book.bento(&run_with_readme_grants("server.ts"), SYSTEM_PATH);
    let mut bento = serving(&guest_book, command, SERVER_PORT);

    assert_guest_book_page(&first_page(SERVER_PORT), &SEEDED_ENTRY);
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
    let arguments = run_with_readme_grants("server.embedded.ts");

    let report = failed_startup(&guest_book, &arguments, EMBEDDED_PORT);

    assert!(report.contains("a module could not be resolved"), "{report}");
    assert!(report.contains("host:guestbook"), "{report}");
}

/// With neither a `NODE_ENV` nor any environment file, the watch launch that `dev` starts
/// still serves development assets.
#[test]
fn dev_migrates_once_and_restarts_the_server_when_its_source_changes() {
    let guest_book = GuestBook::install();
    let command = guest_book.bento(&run_with_readme_grants("dev"), &tool_path());
    let mut bento = serving(&guest_book, command, DEV_PORT);
    let page = first_page(DEV_PORT);
    assert_guest_book_page(&page, &[]);
    assert_development_assets(DEV_PORT, &page.body);

    dev_watch::sign_without_restarting(&bento, DEV_PORT);
    dev_watch::edit_server_source(&guest_book, &bento, DEV_PORT);
    dev_watch::edit_browser_source(&guest_book, &bento, DEV_PORT);
    dev_watch::break_and_repair(&guest_book, &bento, DEV_PORT);

    assert_eq!(bento.stdout.count("applied 20260414120000_create_guest_book"), 1);
    assert_eq!(interrupt(&mut bento, DEV_PORT), Some(0), "{}", bento.stderr.text());
}

/// The outer launch has no mode, so only `.env` applies to it and to the migration `dev` runs;
/// the watch launch it starts selects `development` and so also reads `.env.development`. The
/// database named there is migrated first with that mode, as the README says.
#[test]
fn dev_serves_on_the_env_port_from_the_development_file_s_database() {
    let guest_book = GuestBook::install();
    guest_book.write(".env", &format!("PORT={ENVIRONMENT_FILE_PORT}\n"));
    guest_book.write(".env.development", "DATABASE_URL=db/development.db\n");
    let migrated = guest_book
        .bento(&["run", "db:migrate"], &tool_path())
        .env("NODE_ENV", "development")
        .output()
        .unwrap();
    assert!(migrated.status.success(), "{}", stderr(&migrated));
    assert!(!guest_book.path("db/data.db").exists(), "the mode file's database was not used");
    guest_book.add_entry("db/development.db", "Development visitor", "From the mode file");
    let mut bento =
        Background::spawn(guest_book.bento(&run_with_readme_grants("dev"), &tool_path()));

    let page = first_page(ENVIRONMENT_FILE_PORT);

    assert_guest_book_page(&page, &["Development visitor", "From the mode file"]);
    assert!(!page.body.contains(".@"), "fingerprinted assets in development: {}", page.body);
    assert!(guest_book.path("db/data.db").exists(), "dev's own migration read the mode file");
    assert_eq!(interrupt(&mut bento, ENVIRONMENT_FILE_PORT), Some(0));
}

#[test]
fn check_stops_at_formatting_and_check_fix_repairs_it() {
    let guest_book = GuestBook::install();
    let run = |task: &str| guest_book.bento(&["run", task], &tool_path()).output().unwrap();
    let source = guest_book.read("app/env.ts");
    let clean = run("check");
    assert!(clean.status.success(), "{}{}", stdout(&clean), stderr(&clean));

    // Misformatted, and declared with a `const` the lint rules reject, so oxlint would say so
    // had it run.
    guest_book.write("app/env.ts", &format!("{source}const   unformatted = 1;\n"));
    let failed = run("check");
    assert_eq!(failed.status.code(), Some(1));
    assert!(stdout(&failed).contains("Format issues found"), "{}", stdout(&failed));
    assert!(stderr(&failed).contains("`oxfmt --check`"), "{}", stderr(&failed));
    assert!(!format!("{}{}", stdout(&failed), stderr(&failed)).contains("prefer-let"));

    guest_book.write("app/env.ts", &source.replace("let Env = ", "let   Env = "));
    let fixed = run("check:fix");
    assert!(fixed.status.success(), "{}{}", stdout(&fixed), stderr(&fixed));
    let output = stdout(&fixed);
    let formatted = output.find("Finished in").expect("oxfmt ran");
    let linted = output.find("Found 0 warnings and 0 errors.").expect("oxlint ran");
    assert!(formatted < linted, "{output}");
    assert_eq!(guest_book.read("app/env.ts"), source);
    assert!(run("check").status.success());
}
