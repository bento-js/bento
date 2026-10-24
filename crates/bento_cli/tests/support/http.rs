//! HTTP/1.1 requests to a server on a loopback port, read whole.

use std::collections::BTreeMap;
use std::io::{self, Read, Write};
use std::net::TcpStream;
use std::time::Duration;

use super::running::loopback;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(1);
/// Generous, because a first request may wait for assets to compile in a debug build.
const READ_TIMEOUT: Duration = Duration::from_secs(30);

pub(crate) struct Response {
    pub status: u16,
    /// Header names in lower case.
    pub headers: BTreeMap<String, String>,
    pub body: String,
}

impl Response {
    pub(crate) fn header(&self, name: &str) -> &str {
        self.headers.get(name).map_or("", String::as_str)
    }
}

pub(crate) fn get(port: u16, path: &str) -> io::Result<Response> {
    request(port, "GET", path, "")
}

/// Submits `form`, already URL-encoded, to `/` as a browser without JavaScript would.
pub(crate) fn post_form(port: u16, form: &str) -> io::Result<Response> {
    request(port, "POST", "/", form)
}

fn request(port: u16, method: &str, path: &str, form: &str) -> io::Result<Response> {
    let mut stream = TcpStream::connect_timeout(&loopback(port), CONNECT_TIMEOUT)?;
    stream.set_read_timeout(Some(READ_TIMEOUT))?;
    write!(
        stream,
        "{method} {path} HTTP/1.1\r\nHost: localhost:{port}\r\nConnection: close\r\n\
         Content-Type: application/x-www-form-urlencoded\r\nContent-Length: {}\r\n\r\n{form}",
        form.len()
    )?;
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes)?;
    parse(&bytes).ok_or_else(|| io::Error::other("malformed HTTP response"))
}

fn parse(bytes: &[u8]) -> Option<Response> {
    let boundary = bytes.windows(4).position(|window| window == b"\r\n\r\n")?;
    let head = std::str::from_utf8(&bytes[..boundary]).ok()?;
    let mut lines = head.lines();
    let status = lines.next()?.split_whitespace().nth(1)?.parse().ok()?;
    let headers: BTreeMap<String, String> = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.to_ascii_lowercase(), value.trim().to_owned()))
        .collect();
    let payload = &bytes[boundary + 4..];
    let body = if headers.get("transfer-encoding").is_some_and(|coding| coding == "chunked") {
        decode_chunks(payload)?
    } else {
        payload.to_vec()
    };
    Some(Response { status, headers, body: String::from_utf8(body).ok()? })
}

fn decode_chunks(mut payload: &[u8]) -> Option<Vec<u8>> {
    let mut body = Vec::new();
    loop {
        let line_end = payload.windows(2).position(|window| window == b"\r\n")?;
        let size = std::str::from_utf8(&payload[..line_end]).ok()?.split(';').next()?;
        let size = usize::from_str_radix(size.trim(), 16).ok()?;
        if size == 0 {
            return Some(body);
        }
        payload = &payload[line_end + 2..];
        body.extend_from_slice(payload.get(..size)?);
        payload = payload.get(size + 2..)?;
    }
}
