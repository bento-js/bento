use std::collections::{BTreeMap, BTreeSet};

use oxc::allocator::Allocator;
use oxc::ast::ast::{Statement, TemplateElement};
use oxc::ast_visit::Visit;
use oxc::parser::Parser;
use oxc::span::{GetSpan, SourceType, Span};

use super::http;

pub fn urls(document: &str) -> BTreeSet<String> {
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

pub fn check(
    port: u16,
    document: &str,
    development: bool,
    minified_scripts: &mut BTreeMap<String, String>,
) {
    let urls = urls(document);
    assert!(
        urls.iter().any(|url| url.ends_with(".js")) && urls.iter().any(|url| url.ends_with(".css")),
        "{urls:?}"
    );
    for url in urls {
        let response = http::request(port, "GET", &url, "").unwrap();
        assert_eq!(response.status, 200, "{url}: {}", response.body);
        let content_type = &response.headers["content-type"];
        let script = content_type.contains("javascript");
        assert!(
            if url.ends_with(".css") { content_type.contains("text/css") } else { script },
            "{url}: {content_type}"
        );
        if development {
            assert!(!url.contains(".@"), "{url}");
            check_source_map(port, &url, &response.body);
        } else {
            let (prefix, suffix) = url.split_once(".@").expect("production fingerprint");
            let (fingerprint, extension) = suffix.split_once('.').unwrap();
            assert_eq!(fingerprint.len(), 6, "{url}");
            assert_eq!(response.headers["cache-control"], "public, max-age=31536000, immutable");
            if script {
                minified_scripts.insert(format!("{prefix}.{extension}"), response.body);
            }
        }
    }
}

/// The fixture's `remix.json` allows `app/routes.ts` and public files, denies test files, and
/// never allows other server modules, so neither of the latter is served in either mode.
/// Production serves files only under fingerprinted URLs, so the allowed module is requested by
/// its source path in development.
pub fn check_rules(port: u16, development: bool) {
    for (url, source) in [
        ("/assets/app/actions/guest-book/controller.test.ts", "guest-book.test"),
        ("/assets/app/router.ts", "createRouter"),
    ] {
        let response = http::request(port, "GET", url, "").unwrap();
        assert_ne!(response.status, 200, "{url} was served: {}", response.body);
        assert!(!response.body.contains(source), "{url} leaked its source: {}", response.body);
    }
    if development {
        let routes = http::request(port, "GET", "/assets/app/routes.ts", "").unwrap();
        assert_eq!(routes.status, 200, "{}", routes.body);
        assert!(routes.body.contains("guestBook"), "{}", routes.body);
    }
}

fn check_source_map(port: u16, url: &str, source: &str) {
    let map = source
        .rsplit_once("sourceMappingURL=")
        .expect("development source map")
        .1
        .split_whitespace()
        .next()
        .unwrap();
    let map_url = if map.starts_with('/') {
        map.to_owned()
    } else {
        format!("{}/{}", url.rsplit_once('/').unwrap().0, map)
    };
    let response = http::request(port, "GET", &map_url, "").unwrap();
    assert_eq!(response.status, 200, "{map_url}");
    assert!(response.headers["content-type"].contains("json"), "{map_url}");
    let map: serde_json::Value = serde_json::from_str(&response.body).unwrap();
    assert_eq!(map["version"], 3);
    assert!(!map["sources"].as_array().unwrap().is_empty());
}

pub fn compare_minified_with_development(port: u16, minified_scripts: &BTreeMap<String, String>) {
    assert!(!minified_scripts.is_empty(), "no production JavaScript assets");
    for (url, minified) in minified_scripts {
        let response = http::request(port, "GET", url, "").unwrap();
        assert_eq!(response.status, 200, "{url}: {}", response.body);
        assert!(response.headers["content-type"].contains("javascript"), "{url}");
        assert!(
            is_minified(&response.body, minified),
            "JS not minified: {url}, development {} bytes, production {} bytes:\n{minified}",
            response.body.len(),
            minified.len()
        );
        check_source_map(port, url, &response.body);
    }
}

fn is_minified(development: &str, production: &str) -> bool {
    let source = development
        .rsplit_once("//# sourceMappingURL=")
        .map_or(development, |(source, _)| source)
        .trim_end();
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, production, SourceType::mjs()).parse();
    assert!(
        parsed.diagnostics.is_empty(),
        "invalid production JavaScript: {:?}",
        parsed.diagnostics
    );
    let mut templates = TemplateSpans(Vec::new());
    templates.visit_program(&parsed.program);
    templates.0.sort_unstable_by_key(|span| span.start);
    let compact = parsed.program.body.iter().all(|statement| {
        if matches!(statement, Statement::ImportDeclaration(_)) {
            return true;
        }
        let span = statement.span();
        let mut previous = span.start;
        for template in templates.0.iter().filter(|template| span.contains_inclusive(**template)) {
            if production[previous as usize..template.start as usize].contains('\n') {
                return false;
            }
            previous = template.end;
        }
        !production[previous as usize..span.end as usize].contains('\n')
    });
    compact && production.len() < source.len()
}

struct TemplateSpans(Vec<Span>);

impl<'a> Visit<'a> for TemplateSpans {
    fn visit_template_element(&mut self, element: &TemplateElement<'a>) {
        self.0.push(element.span);
    }
}

#[test]
fn unminified_javascript_without_a_source_map_is_not_minified() {
    let source = "export function greeting(name) {\n  return `Hello ${name}`;\n}\n";
    let development = format!("{source}//# sourceMappingURL=greeting.js.map\n");
    assert!(!is_minified(&development, source));
}

#[test]
fn compact_javascript_is_minified_relative_to_the_unmapped_source() {
    let development = "export function greeting(name) {\n  const message = `Hello ${name}`;\n  return message;\n}\n//# sourceMappingURL=greeting.js.map\n";
    let production = "export function greeting(n){return`Hello ${n}`}";
    assert!(is_minified(development, production));
}

#[test]
fn import_rewriting_does_not_hide_compact_executable_code() {
    let development = "import { helper } from '/assets/helper.js';\nexport function greeting(name) {\n  const message = `Hello ${helper(name)}`;\n  return message;\n}\n//# sourceMappingURL=greeting.js.map\n";
    let production = "import { helper as h } from '/assets/helper.js';\nexport function greeting(n){return`Hello ${h(n)}`}";
    assert!(is_minified(development, production));
}

#[test]
fn multiline_template_contents_do_not_count_as_javascript_formatting() {
    let development = "export function styles() {\n  return `body {\n    color: red;\n}`;\n}\n//# sourceMappingURL=styles.js.map\n";
    let production = "export function styles(){return`body {\n    color: red;\n}`}";
    assert!(is_minified(development, production));
}
