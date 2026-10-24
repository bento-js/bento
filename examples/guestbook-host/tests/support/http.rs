//! HTTP/1.1 requests to the served guest book, with this suite's timeouts.
//!
//! A request that cannot be answered is a test failure here, so these panic instead of
//! reporting: every caller is asserting on the response.

use std::time::Duration;

use bento_test_support::http::Client;
pub(crate) use bento_test_support::http::Response;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
/// Generous, because a first request waits for the guest book's assets to compile.
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(60);

fn client(port: u16) -> Client {
    Client::new(port, CONNECT_TIMEOUT, RESPONSE_TIMEOUT)
}

pub(crate) fn get(port: u16, path: &str) -> Response {
    client(port).get(path).unwrap_or_else(|error| panic!("GET {path} on port {port}: {error}"))
}

/// `POST`s `fields` as `application/x-www-form-urlencoded`, as a browser form submission does.
pub(crate) fn post_form(port: u16, path: &str, fields: &[(&str, &str)]) -> Response {
    client(port)
        .post_form(path, fields)
        .unwrap_or_else(|error| panic!("POST {path} on port {port}: {error}"))
}
