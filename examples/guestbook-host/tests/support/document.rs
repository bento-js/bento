//! What a served guest book page contains, and the hydration assets it references: the
//! stylesheet, the module scripts, and the import map, which must all be served from the same
//! instance in the shape the mode promises.

use std::collections::BTreeSet;

use bento_guestbook_host::launch::Mode;

use super::http::{self, Response};

/// The page rendered the guest book with its form and every listed entry.
pub(crate) fn assert_page(page: &Response, entries: &[(&str, &str)]) {
    assert_eq!(page.status, 200, "{}", page.body);
    for content in [
        "<!DOCTYPE html>",
        "Guest Book",
        "<form",
        "name=\"name\"",
        "name=\"message\"",
        "type=\"importmap\"",
        "type=\"module\"",
        "rel=\"stylesheet\"",
    ] {
        assert!(page.body.contains(content), "missing {content}: {}", page.body);
    }
    if entries.is_empty() {
        assert!(page.body.contains("No one has signed yet"), "{}", page.body);
    }
    for (name, message) in entries {
        assert!(page.body.contains(name), "missing entry {name}: {}", page.body);
        assert!(page.body.contains(message), "missing message {message}: {}", page.body);
    }
}

/// The page's sign form is blank: neither field echoes a submitted value.
pub(crate) fn assert_form_cleared(page: &Response) {
    let (name, _) = named_element(&page.body, "input", "name");
    let value = name.split_once(" value=\"").map(|(_, value)| value.split('"').next().unwrap());
    assert!(value.unwrap_or_default().is_empty(), "the name field kept a value: <input{name}>");
    let (_, after_message) = named_element(&page.body, "textarea", "message");
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

/// Every `/assets/` URL the document references through `href`, `src`, or its import map.
pub(crate) fn asset_urls(document: &str) -> BTreeSet<String> {
    let mut urls = BTreeSet::new();
    for attribute in ["href=\"", "src=\""] {
        for tail in document.split(attribute).skip(1) {
            let url = tail.split('"').next().unwrap().replace("&amp;", "&");
            if url.starts_with("/assets/") {
                urls.insert(url);
            }
        }
    }
    for script in document.split("<script").skip(1) {
        let (attributes, contents) = script.split_once('>').unwrap();
        if attributes.contains("type=\"importmap\"") {
            let contents = contents.split_once("</script>").unwrap().0;
            collect_import_map_urls(&serde_json::from_str(contents).unwrap(), &mut urls);
        }
    }
    urls
}

fn collect_import_map_urls(import_map: &serde_json::Value, urls: &mut BTreeSet<String>) {
    match import_map {
        serde_json::Value::String(url) if url.starts_with("/assets/") && !url.ends_with('/') => {
            urls.insert(url.clone());
        }
        serde_json::Value::Object(fields) => {
            for field in fields.values() {
                collect_import_map_urls(field, urls);
            }
        }
        _ => {}
    }
}

/// Every referenced asset is served with its content type. Development serves plain names with
/// source maps; production serves fingerprinted, immutable names.
pub(crate) fn assert_assets_served(port: u16, document: &str, mode: Mode) {
    let urls = asset_urls(document);
    assert!(urls.iter().any(|url| url.ends_with(".js")), "{urls:?}");
    assert!(urls.iter().any(|url| url.ends_with(".css")), "{urls:?}");
    for url in urls {
        let asset = http::get(port, &url);
        assert_eq!(asset.status, 200, "{url}: {}", asset.body);
        let content_type = asset.header("content-type").unwrap_or_default();
        let expected = if url.ends_with(".css") { "text/css" } else { "javascript" };
        assert!(content_type.contains(expected), "{url}: {content_type}");
        match mode {
            Mode::Development => {
                assert!(!url.contains(".@"), "{url}");
                if expected == "javascript" {
                    assert_source_map_served(port, &url, &asset.body);
                }
            }
            Mode::Production => {
                let (_, suffix) = url.split_once(".@").expect("production fingerprint");
                assert_eq!(suffix.split_once('.').unwrap().0.len(), 6, "{url}");
                assert_eq!(
                    asset.header("cache-control"),
                    Some("public, max-age=31536000, immutable"),
                    "{url}"
                );
            }
        }
    }
}

fn assert_source_map_served(port: u16, url: &str, source: &str) {
    let map = source
        .rsplit_once("sourceMappingURL=")
        .unwrap_or_else(|| panic!("{url} has no source map"))
        .1
        .split_whitespace()
        .next()
        .unwrap();
    let map_url = if map.starts_with('/') {
        map.to_owned()
    } else {
        format!("{}/{map}", url.rsplit_once('/').unwrap().0)
    };
    let response = http::get(port, &map_url);
    assert_eq!(response.status, 200, "{map_url}");
    let map: serde_json::Value = serde_json::from_str(&response.body).unwrap();
    assert_eq!(map["version"], 3, "{map_url}");
}
