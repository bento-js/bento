//! The real guest book embedded in this process: `server.embedded.ts` served over
//! `host:guestbook` with the host's grants, in both modes, with its entries in the Rust-owned
//! store across instance disposal, isolated from other instances, and unable to read the store
//! file it lists from.

mod support;

use std::fs;
use std::time::{Duration, Instant};

use chrono::DateTime;

use bento_guestbook_host::launch::{Launch, Mode};
use bento_guestbook_host::store::{Access, GuestBookStore, NewEntry};
use bento_runtime::{ErrorKind, RunOutcome, RunningRuntime, ShutdownOutcome, start};
use bento_test_support::{ChildLimit, isolated_fixture_test};
use support::{
    assert_port_free, document, fixture, free_port, guests, http, scratch_dir, start_fixture,
    stored_rows,
};

const ENTRY: &str = "server.embedded.ts";
const DEADLINE: Duration = Duration::from_secs(5);

fn shutdown_gracefully(runtime: RunningRuntime, port: u16) {
    let requested = Instant::now();
    let report = runtime.handle().shutdown();
    assert_eq!(report.outcome, ShutdownOutcome::Graceful { exit_code: Some(0) });
    assert!(requested.elapsed() < DEADLINE, "{:?}", requested.elapsed());
    assert!(matches!(runtime.wait(), RunOutcome::Shutdown(_)));
    assert_port_free(port);
}

#[test]
fn the_guest_book_serves_signs_validates_and_persists_across_disposal_in_both_modes() {
    if isolated_fixture_test(
        "the_guest_book_serves_signs_validates_and_persists_across_disposal_in_both_modes",
        ChildLimit::OneAtATime,
    ) {
        return;
    }
    for mode in [Mode::Development, Mode::Production] {
        let scratch = scratch_dir();
        let database = scratch.path().join("book.db");
        let port = free_port();
        let launch = Launch::prepare(fixture(), &database, port, mode).unwrap();

        let first = start_fixture(&launch, Access::ReadWrite);
        let empty = http::get(port, "/");
        document::assert_page(&empty, &[]);
        document::assert_assets_served(port, &empty.body, mode);

        let visitor = ("Ada Lovelace", "Hello from 1843");
        let signed = http::post_form(port, "/", &[("name", visitor.0), ("message", visitor.1)]);
        assert_eq!(signed.status, 303, "{}", signed.body);
        assert_eq!(signed.header("location"), Some("/"));
        let signed_page = http::get(port, "/");
        document::assert_page(&signed_page, &[visitor]);
        document::assert_form_cleared(&signed_page);
        let rows = stored_rows(&database);
        assert_eq!(rows.len(), 1, "{rows:?}");
        assert_eq!((rows[0].0.as_str(), rows[0].1.as_str()), visitor);
        DateTime::parse_from_rfc3339(&rows[0].2).unwrap();

        let blank = http::post_form(port, "/", &[("name", "   "), ("message", "  ")]);
        assert_eq!(blank.status, 400, "{}", blank.body);
        assert!(blank.body.contains("Enter your name."), "{}", blank.body);
        assert!(blank.body.contains("Write a message to sign the guest book."), "{}", blank.body);
        let long_message = "x".repeat(281);
        let too_long = http::post_form(port, "/", &[("name", "Long"), ("message", &long_message)]);
        assert_eq!(too_long.status, 400, "{}", too_long.body);
        assert!(too_long.body.contains("Keep your message to 280 characters or fewer."));
        assert_eq!(stored_rows(&database), rows, "a rejected submission wrote to the store");
        shutdown_gracefully(first, port);

        let replacement = start_fixture(&launch, Access::ReadWrite);
        let persisted = http::get(port, "/");
        document::assert_page(&persisted, &[visitor]);
        assert_eq!(document::asset_urls(&persisted.body), document::asset_urls(&empty.body));
        shutdown_gracefully(replacement, port);
    }
}

#[test]
fn an_instance_without_the_registration_cannot_import_host_guestbook_while_another_serves() {
    if isolated_fixture_test(
        "an_instance_without_the_registration_cannot_import_host_guestbook_while_another_serves",
        ChildLimit::OneAtATime,
    ) {
        return;
    }
    let scratch = scratch_dir();
    let database = scratch.path().join("book.db");
    let port = free_port();
    let launch = Launch::prepare(fixture(), &database, port, Mode::Production).unwrap();
    let serving = start_fixture(&launch, Access::ReadWrite);
    document::assert_page(&http::get(port, "/"), &[]);

    let other = Launch::prepare(fixture(), &database, free_port(), Mode::Production).unwrap();
    let mut unregistered =
        other.runtime_config(GuestBookStore::open(&database, Access::ReadOnly).unwrap());
    unregistered.host_modules.clear();
    let RunOutcome::Failed(error) =
        start(unregistered, &other.project().join(ENTRY)).unwrap().wait()
    else {
        panic!("an instance without the registration imported host:guestbook");
    };
    assert_eq!(error.kind, ErrorKind::Resolution, "{error:?}");
    assert!(error.message.contains("host:guestbook"), "{}", error.message);

    let visitor = ("Grace Hopper", "Still serving");
    let signed = http::post_form(port, "/", &[("name", visitor.0), ("message", visitor.1)]);
    assert_eq!(signed.status, 303, "{}", signed.body);
    document::assert_page(&http::get(port, "/"), &[visitor]);
    shutdown_gracefully(serving, port);
}

/// The probe lives inside the fixture copy because the grants under test only let the guest
/// load modules from there; it lists through `host:guestbook` and then tries the store file.
#[test]
fn the_guest_cannot_read_the_store_file_with_the_grants_that_serve_the_guest_book() {
    if isolated_fixture_test(
        "the_guest_cannot_read_the_store_file_with_the_grants_that_serve_the_guest_book",
        ChildLimit::OneAtATime,
    ) {
        return;
    }
    let scratch = scratch_dir();
    let database = scratch.path().join("book.db");
    let store = GuestBookStore::open(&database, Access::ReadWrite).unwrap();
    let entry = NewEntry { name: "Ada Lovelace".to_owned(), message: "Kept".to_owned() };
    tokio::runtime::Runtime::new().unwrap().block_on(store.append(entry)).unwrap();

    let probe = fixture().join("probe-store-file.mjs");
    fs::write(&probe, guests::PROBE_STORE_FILE).unwrap();
    let launch = Launch::prepare(fixture(), &database, free_port(), Mode::Production).unwrap();
    let mut config = launch.runtime_config(store);
    config.argv = vec![launch.database().to_str().unwrap().to_owned()];
    let outcome = start(config, &probe).unwrap().wait();
    fs::remove_file(&probe).unwrap();
    assert!(matches!(outcome, RunOutcome::Completed), "{outcome:?}");
}
