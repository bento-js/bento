use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

pub(crate) struct Response {
    pub status: u16,
    pub headers: BTreeMap<String, String>,
    pub body: String,
}

pub(crate) fn request(
    port: u16,
    method: &str,
    path: &str,
    body: &str,
) -> std::io::Result<Response> {
    let mut stream = TcpStream::connect_timeout(
        &format!("127.0.0.1:{port}").parse().unwrap(),
        Duration::from_secs(1),
    )?;
    stream.set_read_timeout(Some(Duration::from_secs(15)))?;
    write!(
        stream,
        "{method} {path} HTTP/1.1\r\nHost: localhost:{port}\r\nConnection: close\r\nContent-Type: application/x-www-form-urlencoded\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    )?;
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes)?;
    let boundary = bytes.windows(4).position(|part| part == b"\r\n\r\n").expect("HTTP headers");
    let head = std::str::from_utf8(&bytes[..boundary]).unwrap();
    let status = head.lines().next().unwrap().split_whitespace().nth(1).unwrap().parse().unwrap();
    let headers: BTreeMap<_, _> = head
        .lines()
        .skip(1)
        .map(|line| {
            let (name, value) = line.split_once(':').unwrap();
            (name.to_ascii_lowercase(), value.trim().to_owned())
        })
        .collect();
    let payload = &bytes[boundary + 4..];
    let body = if headers.get("transfer-encoding").is_some_and(|value| value == "chunked") {
        decode_chunks(payload)
    } else {
        payload.to_vec()
    };
    Ok(Response { status, headers, body: String::from_utf8(body).unwrap() })
}

fn decode_chunks(mut payload: &[u8]) -> Vec<u8> {
    let mut body = Vec::new();
    loop {
        let boundary = payload.windows(2).position(|part| part == b"\r\n").unwrap();
        let size = std::str::from_utf8(&payload[..boundary]).unwrap().split(';').next().unwrap();
        let size = usize::from_str_radix(size, 16).unwrap();
        if size == 0 {
            return body;
        }
        payload = &payload[boundary + 2..];
        body.extend_from_slice(&payload[..size]);
        payload = &payload[size + 2..];
    }
}
