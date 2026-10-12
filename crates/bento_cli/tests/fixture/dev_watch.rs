//! The steps of the `dev` test, each changing the project of the guest book it runs.

use std::thread;
use std::time::Duration;

use crate::support::guest_book::GuestBook;
use crate::support::guest_book_page::{assert_guest_book_page, wait_for_page};
use crate::support::http;
use crate::support::running::{Background, wait_until};

const PAGE: &str = "app/actions/guest-book/page.tsx";
const BROWSER_ENTRY: &str = "app/actions/public/entry.ts";
const PERSISTED_ENTRY: [&str; 2] = ["Persistent visitor", "Saved before the restarts"];
/// Long enough for a change to have caused a restart, had it caused one.
const SETTLE: Duration = Duration::from_secs(1);
const ASSET_DEADLINE: Duration = Duration::from_secs(30);

fn restarts(bento: &Background) -> usize {
    bento.stderr.all().iter().filter(|line| line.starts_with("watch: restarting")).count()
}

fn restarted_for(bento: &Background, file: &str) -> usize {
    bento.stderr.count(&format!("watch: restarting after a change to {file}"))
}

fn edit(guest_book: &GuestBook, file: &str, from: &str, to: &str) {
    let source = guest_book.read(file);
    assert!(source.contains(from), "{file} does not contain {from}");
    guest_book.write(file, &source.replacen(from, to, 1));
}

/// Saving an entry writes the database, which must not restart the server.
pub fn sign_without_restarting(bento: &Background, port: u16) {
    let [name, message] = PERSISTED_ENTRY.map(|text| text.replace(' ', "+"));
    let signed = http::post_form(port, &format!("name={name}&message={message}")).unwrap();
    assert_eq!(signed.status, 303, "{}", signed.body);
    thread::sleep(SETTLE);
    assert_eq!(restarts(bento), 0, "{}", bento.stderr.text());
}

/// The page module is imported by the server through the router, not by `server.ts`.
pub fn edit_server_source(guest_book: &GuestBook, bento: &Background, port: u16) {
    edit(guest_book, PAGE, "Welcome to Remix 3<", "Welcome to Remix 3, served by Bento<");

    let page = wait_for_page(port, |page| page.body.contains("served by Bento"));

    assert_guest_book_page(&page, &PERSISTED_ENTRY);
    assert_eq!(restarted_for(bento, PAGE), 1, "{}", bento.stderr.text());
}

/// The browser entry is compiled by the asset server, never imported by the server.
pub fn edit_browser_source(guest_book: &GuestBook, bento: &Background, port: u16) {
    edit(guest_book, BROWSER_ENTRY, "Frame adoption failed:", "Frame adoption stopped:");
    let url = format!("/assets/{BROWSER_ENTRY}");

    wait_until(ASSET_DEADLINE, || {
        http::get(port, &url).is_ok_and(|asset| asset.body.contains("Frame adoption stopped:"))
    });

    assert_eq!(restarted_for(bento, BROWSER_ENTRY), 1, "{}", bento.stderr.text());
}

/// A syntax error stops the server and is reported at its place; fixing it serves again.
pub fn break_and_repair(guest_book: &GuestBook, bento: &Background, port: u16) {
    let working = guest_book.read(PAGE);
    let broken_line = "let unfinished: number = ;";
    let declaration = "export interface";
    let broken = working.replacen(declaration, &format!("{broken_line}\n{declaration}"), 1);
    let line = broken.lines().position(|line| line == broken_line).unwrap() + 1;
    guest_book.write(PAGE, &broken);

    bento.wait_for_stderr("watch: waiting for changes before restarting");
    thread::sleep(SETTLE);
    assert!(http::get(port, "/").is_err(), "the old server still answers");
    let report = bento.stderr.text();
    assert!(report.contains(&format!("{PAGE}:{line}:26")), "{report}");
    let quoted = report.lines().any(|quote| quote.contains('│') && quote.ends_with(broken_line));
    assert!(quoted, "{report}");
    assert!(!report.contains('\u{1b}'), "{report}");

    guest_book.write(PAGE, &working);
    let page = wait_for_page(port, |page| page.body.contains("served by Bento"));
    assert_guest_book_page(&page, &PERSISTED_ENTRY);
}
