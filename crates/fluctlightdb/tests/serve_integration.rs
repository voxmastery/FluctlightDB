//! HTTP integration tests — auth, tenant binding, consolidate.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Duration;

use fluctlightdb::{BrainServer, request_shutdown};
use tempfile::tempdir;

fn post(port: u16, path: &str, body: &str, token: Option<&str>) -> (u16, String) {
    let mut stream = TcpStream::connect(format!("127.0.0.1:{port}")).expect("connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let auth = token
        .map(|t| format!("Authorization: Bearer {t}\r\n"))
        .unwrap_or_default();
    let req = format!(
        "POST {path} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{auth}\r\n{body}",
        body.len()
    );
    stream.write_all(req.as_bytes()).unwrap();
    let mut buf = vec![0u8; 65536];
    let n = stream.read(&mut buf).unwrap();
    let resp = String::from_utf8_lossy(&buf[..n]).to_string();
    let status = resp
        .lines()
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let body_start = resp.find("\r\n\r\n").map(|i| i + 4).unwrap_or(0);
    (status, resp[body_start..].to_string())
}

#[test]
fn serve_auth_and_consolidate() {
    let prev_keys = std::env::var("FLUCTLIGHT_API_KEYS").ok();
    let prev_req = std::env::var("FLUCTLIGHT_REQUIRE_AUTH").ok();
    std::env::set_var("FLUCTLIGHT_API_KEYS", "default:testkey:admin");
    std::env::set_var("FLUCTLIGHT_REQUIRE_AUTH", "true");
    std::env::set_var("FLUCTLIGHT_WAL_FSYNC", "always");

    let dir = tempdir().unwrap();
    let brain = dir.path().join("brain");
    let server = BrainServer::open(brain).unwrap();
    let port = 18792u16;
    let addr = format!("127.0.0.1:{port}");
    let barrier = Arc::new(Barrier::new(2));
    let b = barrier.clone();
    let handle = thread::spawn(move || {
        b.wait();
        let _ = server.serve(&addr);
    });
    barrier.wait();
    thread::sleep(Duration::from_millis(300));

    let (s0, _) = post(port, "/api/v1/status", "{}", None);
    assert_eq!(s0, 401);

    let (s1, body1) = post(port, "/api/v1/status", "{}", Some("testkey"));
    assert_eq!(s1, 200);
    assert!(body1.contains("engrams"));

    let exp = r#"{"content":"integration test memory","context":"test","salience":0.8}"#;
    let (s2, _) = post(port, "/api/v1/experience", exp, Some("testkey"));
    assert_eq!(s2, 200);

    let (s3, body3) = post(
        port,
        "/api/v1/consolidate",
        r#"{"min_salience":0.5,"limit":5}"#,
        Some("testkey"),
    );
    assert_eq!(s3, 200);
    assert!(body3.contains("memories"));

    request_shutdown();
    let _ = handle.join();

    match prev_keys {
        Some(v) => std::env::set_var("FLUCTLIGHT_API_KEYS", v),
        None => std::env::remove_var("FLUCTLIGHT_API_KEYS"),
    }
    match prev_req {
        Some(v) => std::env::set_var("FLUCTLIGHT_REQUIRE_AUTH", v),
        None => std::env::remove_var("FLUCTLIGHT_REQUIRE_AUTH"),
    }
}
