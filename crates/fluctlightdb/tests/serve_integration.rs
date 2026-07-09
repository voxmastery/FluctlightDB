//! HTTP integration tests — auth, tenant binding, consolidate.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::{Arc, Barrier, Mutex};
use std::thread;
use std::time::Duration;

use fluctlightdb::{request_shutdown, reset_shutdown_for_tests, BrainServer};
use tempfile::tempdir;

static SERVE_ITEST_LOCK: Mutex<()> = Mutex::new(());

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
    let _guard = SERVE_ITEST_LOCK.lock().unwrap();
    let prev_keys = std::env::var("FLUCTLIGHT_API_KEYS").ok();
    let prev_req = std::env::var("FLUCTLIGHT_REQUIRE_AUTH").ok();
    std::env::set_var("FLUCTLIGHT_API_KEYS", "default:testkey:admin");
    std::env::set_var("FLUCTLIGHT_REQUIRE_AUTH", "true");
    std::env::set_var("FLUCTLIGHT_WAL_FSYNC", "always");
    reset_shutdown_for_tests();

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

fn start_server(
    brain: std::path::PathBuf,
    keys: &str,
    port: u16,
) -> (std::thread::JoinHandle<()>, Arc<Barrier>) {
    reset_shutdown_for_tests();
    std::env::set_var("FLUCTLIGHT_API_KEYS", keys);
    std::env::set_var("FLUCTLIGHT_REQUIRE_AUTH", "true");
    let server = BrainServer::open(brain).unwrap();
    let addr = format!("127.0.0.1:{port}");
    let barrier = Arc::new(Barrier::new(2));
    let b = barrier.clone();
    let handle = thread::spawn(move || {
        b.wait();
        let _ = server.serve(&addr);
    });
    barrier.wait();
    thread::sleep(Duration::from_millis(300));
    (handle, barrier)
}

#[test]
fn serve_cross_tenant_path_forbidden() {
    let _guard = SERVE_ITEST_LOCK.lock().unwrap();
    let prev_keys = std::env::var("FLUCTLIGHT_API_KEYS").ok();
    let prev_req = std::env::var("FLUCTLIGHT_REQUIRE_AUTH").ok();
    let dir = tempdir().unwrap();
    let port = 18793u16;
    let handle = start_server(
        dir.path().join("brain"),
        "tenant_a:key_a:write,tenant_b:key_b:write",
        port,
    );

    let exp = r#"{"content":"tenant a secret","context":"iso","salience":0.8}"#;
    let (s_write, _) = post(port, "/api/v1/experience", exp, Some("key_a"));
    assert_eq!(s_write, 200);

    let (s_forbidden, body) = post(port, "/api/v1/tenants/tenant_a/status", "{}", Some("key_b"));
    assert_eq!(s_forbidden, 403, "tenant_b must not read tenant_a: {body}");

    request_shutdown();
    let _ = handle.0.join();

    match prev_keys {
        Some(v) => std::env::set_var("FLUCTLIGHT_API_KEYS", v),
        None => std::env::remove_var("FLUCTLIGHT_API_KEYS"),
    }
    match prev_req {
        Some(v) => std::env::set_var("FLUCTLIGHT_REQUIRE_AUTH", v),
        None => std::env::remove_var("FLUCTLIGHT_REQUIRE_AUTH"),
    }
}

#[test]
fn serve_read_role_cannot_write() {
    let _guard = SERVE_ITEST_LOCK.lock().unwrap();
    let prev_keys = std::env::var("FLUCTLIGHT_API_KEYS").ok();
    let prev_req = std::env::var("FLUCTLIGHT_REQUIRE_AUTH").ok();
    let dir = tempdir().unwrap();
    let port = 18794u16;
    let handle = start_server(dir.path().join("brain"), "tenant_a:read_only:read", port);

    let (s_status, _) = post(port, "/api/v1/status", "{}", Some("read_only"));
    assert_eq!(s_status, 200);

    let exp = r#"{"content":"should fail","context":"rbac","salience":0.5}"#;
    let (s_write, body) = post(port, "/api/v1/experience", exp, Some("read_only"));
    assert_eq!(s_write, 403, "read role must not write: {body}");

    request_shutdown();
    let _ = handle.0.join();

    match prev_keys {
        Some(v) => std::env::set_var("FLUCTLIGHT_API_KEYS", v),
        None => std::env::remove_var("FLUCTLIGHT_API_KEYS"),
    }
    match prev_req {
        Some(v) => std::env::set_var("FLUCTLIGHT_REQUIRE_AUTH", v),
        None => std::env::remove_var("FLUCTLIGHT_REQUIRE_AUTH"),
    }
}
