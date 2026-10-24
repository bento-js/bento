//! HTTP/1.1 requests to a server a `bento` is running, with this suite's timeouts.

use std::io;
use std::time::Duration;

use bento_test_support::http::Client;
pub(crate) use bento_test_support::http::Response;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(1);
/// Generous, because a first request may wait for assets to compile in a debug build.
const READ_TIMEOUT: Duration = Duration::from_secs(30);

fn client(port: u16) -> Client {
    Client::new(port, CONNECT_TIMEOUT, READ_TIMEOUT)
}

pub(crate) fn get(port: u16, path: &str) -> io::Result<Response> {
    client(port).get(path)
}

/// Submits `fields` to `/` as a browser without JavaScript would.
pub(crate) fn post_form(port: u16, fields: &[(&str, &str)]) -> io::Result<Response> {
    client(port).post_form("/", fields)
}
