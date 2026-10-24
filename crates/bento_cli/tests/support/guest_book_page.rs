//! What a served guest book must show: its page, its forms' responses, and its assets.

use std::collections::BTreeSet;
use std::thread;
use std::time::{Duration, Instant};

use super::http::{self, Response};

/// How long a server may take to serve a page: it compiles its assets before listening.
const PAGE_DEADLINE: Duration = Duration::from_secs(60);
const POLL_INTERVAL: Duration = Duration::from_millis(50);
const FINGERPRINT_LENGTH: usize = 6;
const IMMUTABLE: &str = "public, max-age=31536000, immutable";

/// Waits until `GET /` on `port` answers with a page `wanted` accepts, and returns it.
pub(crate) fn wait_for_page(port: u16, wanted: impl Fn(&Response) -> bool) -> Response {
    let start = Instant::now();
    loop {
        match http::get(port, "/") {
            Ok(page) if wanted(&page) => return page,
            last => assert!(
                start.elapsed() < PAGE_DEADLINE,
                "no wanted page on port {port}; last: {:?}",
                last.map(|page| page.body)
            ),
        }
        thread::sleep(POLL_INTERVAL);
    }
}

/// The first page the server on `port` serves.
pub(crate) fn first_page(port: u16) -> Response {
    wait_for_page(port, |_| true)
}

/// Asserts that `page` is the whole rendered guest book, showing `entries`.
pub(crate) fn assert_guest_book_page(page: &Response, entries: &[&str]) {
    assert_eq!(page.status, 200, "{}", page.body);
    let document = [
        "<!DOCTYPE html>",
        "Welcome to Remix 3",
        "Guest Book",
        "<form",
        "name=\"name\"",
        "name=\"message\"",
        "Sign",
        "type=\"importmap\"",
        "type=\"module\"",
        "rel=\"stylesheet\"",
    ];
    for content in document.iter().chain(entries) {
        assert!(page.body.contains(content), "missing {content}: {}", page.body);
    }
}

/// Signs the guest book on `port` as a browser without JavaScript would: a valid entry is
/// saved, redirects home, and leaves a blank form; blank fields and a 281-character message are
/// refused.
pub(crate) fn assert_signing(port: u16) {
    let valid =
        http::post_form(port, "name=Bento+visitor&message=Signed+through+bento+run").unwrap();
    assert_eq!(valid.status, 303, "{}", valid.body);
    assert_eq!(valid.header("location"), "/");
    let page = http::get(port, "/").unwrap();
    assert_guest_book_page(&page, &["Bento visitor", "Signed through bento run"]);
    assert_form_cleared(&page.body);

    let blank = http::post_form(port, "name=+++&message=++").unwrap();
    assert_eq!(blank.status, 400, "{}", blank.body);
    for error in ["Enter your name.", "Write a message to sign the guest book.", "value=\"   \""] {
        assert!(blank.body.contains(error), "missing {error}: {}", blank.body);
    }

    let long = http::post_form(port, &format!("name=Long&message={}", "x".repeat(281))).unwrap();
    assert_eq!(long.status, 400, "{}", long.body);
    assert!(long.body.contains("Keep your message to 280 characters or fewer."), "{}", long.body);
}

/// Asserts that the sign form in `document` is blank: neither field echoes a submitted value.
fn assert_form_cleared(document: &str) {
    let (name, _) = named_element(document, "input", "name");
    let value = name.split_once(" value=\"").map(|(_, value)| value.split('"').next().unwrap());
    assert!(value.unwrap_or_default().is_empty(), "the name field kept a value: <input{name}>");
    let (_, after_message) = named_element(document, "textarea", "message");
    let message = after_message.split_once("</textarea>").expect("unclosed textarea").0;
    assert!(message.is_empty(), "the message field kept {message:?}");
}

/// The attributes of the first `<tag>` in `document` named `name`, and what follows its `>`.
fn named_element<'a>(document: &'a str, tag: &str, name: &str) -> (&'a str, &'a str) {
    let name_attribute = format!(" name=\"{name}\"");
    document
        .split(&format!("<{tag}"))
        .skip(1)
        .filter_map(|tail| tail.split_once('>'))
        .find(|(attributes, _)| attributes.contains(&name_attribute))
        .unwrap_or_else(|| panic!("no <{tag}> named {name}"))
}

/// Asserts that every asset `document` links is served, minified under a fingerprinted URL
/// that browsers may cache forever, as in production.
pub(crate) fn assert_production_assets(port: u16, document: &str) {
    for url in asset_urls(document) {
        let asset = served_asset(port, &url);
        assert!(has_fingerprint(&url), "{url} has no fingerprint");
        assert_eq!(asset.header("cache-control"), IMMUTABLE, "{url}");
    }
}

/// Asserts that every asset `document` links is served under its source path, scripts with
/// a source map, as in development.
pub(crate) fn assert_development_assets(port: u16, document: &str) {
    for url in asset_urls(document) {
        let asset = served_asset(port, &url);
        assert!(!url.contains(".@"), "{url} has a fingerprint in development");
        if asset.header("content-type").contains("javascript") {
            assert_source_map(port, &url, &asset.body);
        }
    }
}

/// The `/assets/` URLs `document` links to or loads.
pub(crate) fn asset_urls(document: &str) -> BTreeSet<String> {
    let urls: BTreeSet<String> = ["href=\"", "src=\""]
        .iter()
        .flat_map(|attribute| document.split(attribute).skip(1))
        .filter_map(|tail| tail.split('"').next())
        .filter(|url| url.starts_with("/assets/"))
        .map(|url| url.replace("&amp;", "&"))
        .collect();
    assert!(urls.iter().any(|url| url.ends_with(".css")), "no stylesheet: {urls:?}");
    assert!(urls.iter().any(|url| url.ends_with(".ts")), "no script: {urls:?}");
    urls
}

fn served_asset(port: u16, url: &str) -> Response {
    let asset = http::get(port, url).unwrap();
    assert_eq!(asset.status, 200, "{url}: {}", asset.body);
    asset
}

/// Whether `url` names its file as `name.@<fingerprint>.<extension>`.
fn has_fingerprint(url: &str) -> bool {
    let Some((_, fingerprinted)) = url.rsplit_once(".@") else { return false };
    fingerprinted
        .split_once('.')
        .is_some_and(|(fingerprint, _)| fingerprint.len() == FINGERPRINT_LENGTH)
}

fn assert_source_map(port: u16, url: &str, script: &str) {
    let reference = script
        .rsplit_once("//# sourceMappingURL=")
        .and_then(|(_, map)| map.split_whitespace().next())
        .unwrap_or_else(|| panic!("{url} has no source map"));
    let map_url = if reference.starts_with('/') {
        reference.to_owned()
    } else {
        format!("{}/{reference}", url.rsplit_once('/').unwrap().0)
    };
    let map = served_asset(port, &map_url);
    let map: serde_json::Value = serde_json::from_str(&map.body).unwrap();
    assert_eq!(map["version"], 3, "{map_url}");
}
