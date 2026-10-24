//! HTTP/1.1 requests to a server an instance of the runtime is running, with this suite's
//! timeouts.

use std::io;
use std::time::Duration;

use bento_test_support::http::Client;
pub(crate) use bento_test_support::http::Response;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(1);
const READ_TIMEOUT: Duration = Duration::from_secs(15);

fn client(port: u16) -> Client {
    Client::new(port, CONNECT_TIMEOUT, READ_TIMEOUT)
}

pub(crate) fn get(port: u16, path: &str) -> io::Result<Response> {
    client(port).get(path)
}

/// Submits `fields` to `path` as a browser without JavaScript would.
pub(crate) fn post_form(port: u16, path: &str, fields: &[(&str, &str)]) -> io::Result<Response> {
    client(port).post_form(path, fields)
}
