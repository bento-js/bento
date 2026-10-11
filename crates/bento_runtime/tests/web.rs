mod support;

use std::collections::BTreeSet;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::time::{Duration, Instant};

use tempfile::tempdir;

use bento_host::{NetTarget, Scope};
use support::{completed, config, run};

#[test]
fn fetch_performs_a_granted_http_request() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut stream = loop {
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(Instant::now() < deadline, "guest never fetched");
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(error) => panic!("HTTP fixture failed: {error}"),
            }
        };
        stream.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        let mut request = [0; 4096];
        let read = stream.read(&mut request).unwrap();
        assert!(std::str::from_utf8(&request[..read]).unwrap().starts_with("GET /guest HTTP/1.1"));
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\nConnection: close\r\n\r\nBento")
            .unwrap();
    });
    let root = tempdir().unwrap();
    let mut settings = config(root.path());
    settings.grants.net = Scope::Only(BTreeSet::from([NetTarget::new("127.0.0.1", Some(port))]));
    let outcome = run(
        settings,
        "fetch.mjs",
        &format!(
            r#"
import assert from 'node:assert/strict';
const response = await fetch(new Request('http://127.0.0.1:{port}/guest'));
assert.equal(response.status, 200);
assert.equal(await response.text(), 'Bento');
"#
        ),
    );
    server.join().unwrap();
    completed(outcome);
}
