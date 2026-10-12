#[path = "support/assets.rs"]
mod assets;
#[path = "support/guestbook.rs"]
mod guestbook;
#[path = "support/http.rs"]
mod http;
mod support;

use std::collections::BTreeMap;
use std::net::TcpListener;
use std::time::{Duration, Instant};

use bento_host::Scope;
use bento_runtime::{
    ErrorKind, RunOutcome, RunningRuntime, ShutdownOutcome, package_lookup_paths, start,
};
use support::{completed, config, fixture, run};

#[test]
fn real_fixture_renders_assets_validates_persists_and_restarts_in_both_modes() {
    if support::isolated_fixture_test(
        "real_fixture_renders_assets_validates_persists_and_restarts_in_both_modes",
    ) {
        return;
    }
    let started = Instant::now();
    let (mut settings, _fixture_directory, _database_directory, port) = guestbook::prepare();
    let root = settings.project_root.clone();
    settings.grants.read = Scope::Only(
        std::iter::once(root.clone()).chain(package_lookup_paths(&root).unwrap()).collect(),
    );
    completed(run(
        settings.clone(),
        "seed.mjs",
        r#"
import { DatabaseSync } from 'node:sqlite';
const db = new DatabaseSync(process.env.DATABASE_URL);
const insert = db.prepare('INSERT INTO guest_book (name, message) VALUES (?, ?)');
insert.run('Seeded visitor', 'A seeded greeting'); db.close();
"#,
    ));
    let mut minified_scripts = BTreeMap::new();
    for development in [true, false] {
        if development {
            settings.env.insert("NODE_ENV".into(), "development".into());
        } else {
            settings.env.remove("NODE_ENV");
        }
        let runtime = start(settings.clone(), &root.join("server.ts")).unwrap();
        let page = wait_for_page(&runtime, port);
        assert_document(&page);
        assets::check(port, &page.body, development, &mut minified_scripts);
        check_forms(port, development);
        assets::check_rules(port, development);
        shutdown(runtime, port);
        let replacement = start(settings.clone(), &root.join("server.ts")).unwrap();
        let persisted = wait_for_page(&replacement, port);
        assert_document(&persisted);
        assert!(
            persisted.body.contains(if development {
                "Persistent visitor"
            } else {
                "Production visitor"
            }) && persisted.body.contains("Saved through Bento")
        );
        assert_eq!(assets::urls(&persisted.body), assets::urls(&page.body));
        shutdown(replacement, port);
    }
    settings.env.insert("NODE_ENV".into(), "development".into());
    let comparison = start(settings, &root.join("server.ts")).unwrap();
    assert_document(&wait_for_page(&comparison, port));
    assets::compare_minified_with_development(port, &minified_scripts);
    shutdown(comparison, port);
    eprintln!("fixture wall time: {:?}", started.elapsed());
}

#[test]
fn project_only_reads_preserve_asset_lookup_permission_error_and_location() {
    if support::isolated_fixture_test(
        "project_only_reads_preserve_asset_lookup_permission_error_and_location",
    ) {
        return;
    }
    let (settings, _fixture_directory, _database_directory, _) = guestbook::prepare();
    let runtime = start(settings.clone(), &settings.project_root.join("server.ts")).unwrap();
    let RunOutcome::Failed(error) = runtime.wait() else {
        panic!("project-only reads unexpectedly allowed asset lookup");
    };
    assert_eq!(error.kind, ErrorKind::Permission, "{error}");
    assert!(error.message.contains("NotCapable"), "{error}");
    let location = error.location().expect("permission denial must identify asset code");
    assert!(location.path.starts_with(settings.project_root.join("node_modules")), "{location:?}");
    assert!(location.path.ends_with("dist/lib/access.js"), "{location:?}");
    assert!(location.line > 0 && location.column > 0, "{location:?}");
}

fn wait_for_page(runtime: &RunningRuntime, port: u16) -> http::Response {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if let Ok(page) = http::request(port, "GET", "/", "") {
            return page;
        }
        assert!(
            !runtime.is_finished(),
            "fixture ended before listening: {:?}",
            runtime.handle().shutdown()
        );
        assert!(
            Instant::now() < deadline,
            "fixture did not listen: {:?}",
            runtime.handle().shutdown()
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn shutdown(runtime: RunningRuntime, port: u16) {
    assert_eq!(
        runtime.handle().shutdown().outcome,
        ShutdownOutcome::Graceful { exit_code: Some(0) }
    );
    assert!(matches!(runtime.wait(), RunOutcome::Shutdown(_)));
    drop(TcpListener::bind(("127.0.0.1", port)).expect("fixture retained listener"));
}

fn assert_document(page: &http::Response) {
    assert_eq!(page.status, 200, "{}", page.body);
    for content in [
        "<!DOCTYPE html>",
        "Welcome to Remix 3",
        "Guest Book",
        "Seeded visitor",
        "A seeded greeting",
        "<form",
        "name=\"name\"",
        "name=\"message\"",
        "Sign",
        "type=\"importmap\"",
        "type=\"module\"",
        "rel=\"stylesheet\"",
    ] {
        assert!(page.body.contains(content), "missing {content}: {}", page.body);
    }
}

fn check_forms(port: u16, development: bool) {
    let visitor = if development { "Persistent+visitor" } else { "Production+visitor" };
    let valid =
        http::request(port, "POST", "/", &format!("name={visitor}&message=Saved+through+Bento"))
            .unwrap();
    assert_eq!(valid.status, 303, "{}", valid.body);
    assert_eq!(valid.headers["location"], "/");
    let page = http::request(port, "GET", "/", "").unwrap();
    assert!(
        page.body.contains(&visitor.replace('+', " ")) && page.body.contains("Saved through Bento")
    );
    let invalid = http::request(port, "POST", "/", "name=+++&message=++").unwrap();
    assert_eq!(invalid.status, 400);
    for content in [
        "Enter your name.",
        "Write a message to sign the guest book.",
        "value=\"   \"",
        ">  </textarea>",
    ] {
        assert!(invalid.body.contains(content), "missing {content}: {}", invalid.body);
    }
    let too_long =
        http::request(port, "POST", "/", &format!("name=Long&message={}", "x".repeat(281)))
            .unwrap();
    assert_eq!(too_long.status, 400);
    assert!(too_long.body.contains("Keep your message to 280 characters or fewer."));
}

#[test]
fn fixture_wait_reports_an_ended_instances_error_without_the_listen_timeout() {
    let root = support::project();
    let entry = root.path().join("failed.mjs");
    std::fs::write(&entry, "throw new Error('fixture startup failed');").unwrap();
    let port = TcpListener::bind(("127.0.0.1", 0)).unwrap().local_addr().unwrap().port();
    let runtime = start(config(root.path()), &entry).unwrap();
    let started = Instant::now();
    let failure = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = wait_for_page(&runtime, port);
    }))
    .expect_err("ended fixture unexpectedly served a page");
    let message = failure
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| failure.downcast_ref::<&str>().copied())
        .expect("fixture failure message");
    assert!(message.contains("fixture startup failed"), "{message}");
    assert!(started.elapsed() < Duration::from_secs(2), "{message}");
    assert!(matches!(runtime.wait(), RunOutcome::Failed(_)));
}
