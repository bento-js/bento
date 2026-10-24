//! HTTP/1.1 against a loopback server, one connection per request, read whole.

use std::collections::BTreeMap;
use std::io::{self, Read, Write};
use std::net::TcpStream;
use std::time::Duration;

use crate::ports::loopback;

/// A response read to the end of its connection.
#[derive(Debug)]
pub struct Response {
    pub status: u16,
    /// Header names in lower case; a name sent more than once keeps its last value.
    pub headers: BTreeMap<String, String>,
    pub body: String,
}

impl Response {
    /// The value sent for `name`, matched without regard to case.
    #[must_use]
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(&name.to_ascii_lowercase()).map(String::as_str)
    }
}

/// Requests to one server on the loopback interface.
///
/// The timeouts belong to the suite that builds the client: how long a server may take to
/// answer differs between a runtime started in this process and a `bento` that compiles a
/// project's assets before its first response.
#[derive(Clone, Copy, Debug)]
pub struct Client {
    port: u16,
    connect_timeout: Duration,
    read_timeout: Duration,
}

impl Client {
    /// Requests to `port`, giving up on a connection after `connect_timeout` and on a response
    /// after `read_timeout`.
    #[must_use]
    pub const fn new(port: u16, connect_timeout: Duration, read_timeout: Duration) -> Client {
        Client { port, connect_timeout, read_timeout }
    }

    /// The response to `GET path`.
    ///
    /// # Errors
    ///
    /// When nothing answers on the port, the response does not arrive in time, or it is not
    /// HTTP/1.1 this can read.
    pub fn get(&self, path: &str) -> io::Result<Response> {
        self.request("GET", path, "")
    }

    /// Submits `fields` to `path` as a browser without JavaScript does: one percent-encoded
    /// `application/x-www-form-urlencoded` body.
    ///
    /// # Errors
    ///
    /// As [`Client::get`].
    pub fn post_form(&self, path: &str, fields: &[(&str, &str)]) -> io::Result<Response> {
        let form = fields
            .iter()
            .map(|(name, value)| format!("{}={}", form_encode(name), form_encode(value)))
            .collect::<Vec<_>>()
            .join("&");
        self.request("POST", path, &form)
    }

    fn request(&self, method: &str, path: &str, form: &str) -> io::Result<Response> {
        let mut stream = TcpStream::connect_timeout(&loopback(self.port), self.connect_timeout)?;
        stream.set_read_timeout(Some(self.read_timeout))?;
        write!(
            stream,
            "{method} {path} HTTP/1.1\r\nHost: localhost:{port}\r\nConnection: close\r\n\
             Content-Type: application/x-www-form-urlencoded\r\nContent-Length: {length}\r\n\r\n\
             {form}",
            port = self.port,
            length = form.len()
        )?;
        let mut bytes = Vec::new();
        stream.read_to_end(&mut bytes)?;
        parse(&bytes).ok_or_else(|| io::Error::other("malformed HTTP response"))
    }
}

fn parse(bytes: &[u8]) -> Option<Response> {
    let boundary = bytes.windows(4).position(|window| window == b"\r\n\r\n")?;
    let head = std::str::from_utf8(&bytes[..boundary]).ok()?;
    let mut lines = head.lines();
    let status = lines.next()?.split_whitespace().nth(1)?.parse().ok()?;
    let headers: BTreeMap<String, String> = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.trim().to_ascii_lowercase(), value.trim().to_owned()))
        .collect();
    let payload = &bytes[boundary + 4..];
    let chunked = headers
        .get("transfer-encoding")
        .is_some_and(|coding| coding.eq_ignore_ascii_case("chunked"));
    let body = if chunked { decode_chunks(payload)? } else { payload.to_vec() };
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

/// Percent-encodes one form field, with `+` for a space as a browser writes it.
fn form_encode(text: &str) -> String {
    let mut encoded = String::new();
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(char::from(byte));
            }
            b' ' => encoded.push('+'),
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::parse;

    #[test]
    fn a_chunked_body_is_decoded_whatever_the_case_of_its_coding() {
        for coding in ["chunked", "Chunked"] {
            let raw = format!(
                "HTTP/1.1 200 OK\r\nTransfer-Encoding: {coding}\r\n\r\n5\r\nhello\r\n0\r\n\r\n"
            );
            let response = parse(raw.as_bytes()).expect("a well-formed response");
            assert_eq!(response.body, "hello", "Transfer-Encoding: {coding}");
        }
    }
}
