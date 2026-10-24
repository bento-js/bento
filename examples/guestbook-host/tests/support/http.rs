//! A small HTTP/1.1 client over one loopback connection per request.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

const RESPONSE_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug)]
pub(crate) struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

impl Response {
    /// The first header with this name, compared case-insensitively.
    pub(crate) fn header(&self, name: &str) -> Option<&str> {
        let name = name.to_ascii_lowercase();
        self.headers.iter().find(|(header, _)| *header == name).map(|(_, value)| value.as_str())
    }
}

pub(crate) fn get(port: u16, path: &str) -> Response {
    request(port, "GET", path, &[], b"")
}

/// `POST`s `fields` as `application/x-www-form-urlencoded`, as a browser form submission does.
pub(crate) fn post_form(port: u16, path: &str, fields: &[(&str, &str)]) -> Response {
    let body = fields
        .iter()
        .map(|(name, value)| format!("{}={}", form_encode(name), form_encode(value)))
        .collect::<Vec<_>>()
        .join("&");
    request(
        port,
        "POST",
        path,
        &[("Content-Type", "application/x-www-form-urlencoded")],
        body.as_bytes(),
    )
}

fn request(port: u16, method: &str, path: &str, headers: &[(&str, &str)], body: &[u8]) -> Response {
    let address = SocketAddr::from(([127, 0, 0, 1], port));
    let mut stream = TcpStream::connect_timeout(&address, Duration::from_secs(5)).unwrap();
    stream.set_read_timeout(Some(RESPONSE_TIMEOUT)).unwrap();
    let mut head =
        format!("{method} {path} HTTP/1.1\r\nHost: localhost:{port}\r\nConnection: close\r\n");
    for (name, value) in headers {
        head.push_str(&format!("{name}: {value}\r\n"));
    }
    head.push_str(&format!("Content-Length: {}\r\n\r\n", body.len()));
    stream.write_all(head.as_bytes()).unwrap();
    stream.write_all(body).unwrap();
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).unwrap();
    parse(&raw)
}

fn parse(raw: &[u8]) -> Response {
    let head_end = raw.windows(4).position(|window| window == b"\r\n\r\n").expect("response head");
    let head = std::str::from_utf8(&raw[..head_end]).unwrap();
    let mut lines = head.split("\r\n");
    let status_line = lines.next().unwrap();
    let status = status_line.split(' ').nth(1).and_then(|code| code.parse().ok()).unwrap();
    let headers: Vec<(String, String)> = lines
        .map(|line| {
            let (name, value) = line.split_once(':').unwrap();
            (name.trim().to_ascii_lowercase(), value.trim().to_owned())
        })
        .collect();
    let body = &raw[head_end + 4..];
    let chunked = headers
        .iter()
        .any(|(name, value)| name == "transfer-encoding" && value.eq_ignore_ascii_case("chunked"));
    let body = if chunked { decode_chunked(body) } else { body.to_vec() };
    Response { status, headers, body: String::from_utf8_lossy(&body).into_owned() }
}

fn decode_chunked(mut body: &[u8]) -> Vec<u8> {
    let mut decoded = Vec::new();
    loop {
        let line_end = body.windows(2).position(|window| window == b"\r\n").expect("chunk size");
        let size_line = std::str::from_utf8(&body[..line_end]).unwrap();
        let size_text = size_line.split(';').next().unwrap().trim();
        let size = usize::from_str_radix(size_text, 16).unwrap();
        body = &body[line_end + 2..];
        if size == 0 {
            return decoded;
        }
        decoded.extend_from_slice(&body[..size]);
        body = &body[size + 2..];
    }
}

fn form_encode(text: &str) -> String {
    let mut encoded = String::new();
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char);
            }
            b' ' => encoded.push('+'),
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}
