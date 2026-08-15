//! CAB adversarial security tests (issue #1).
//! Exercises the real BrainServer over raw HTTP.
//! Run: cargo test -p fluctlightdb --test zz_security_review -- --test-threads=1

use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Duration;

use fluctlightdb::test_env::EnvGuard;
use fluctlightdb::{reset_shutdown_for_tests, BrainServer};
use tempfile::tempdir;

const AUTH_ENV: &[&str] = &[
    "FLUCTLIGHT_API_KEYS",
    "FLUCTLIGHT_REQUIRE_AUTH",
    "HOME",
    "USERPROFILE",
];

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
    let n = stream.read(&mut buf).unwrap_or(0);
    let resp = String::from_utf8_lossy(&buf[..n]).to_string();
    let status = resp
        .lines()
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let bs = resp.find("\r\n\r\n").map(|i| i + 4).unwrap_or(0);
    (status, resp[bs..].to_string())
}

fn start(brain: std::path::PathBuf, port: u16) -> thread::JoinHandle<()> {
    reset_shutdown_for_tests();
    let server = BrainServer::open(brain).unwrap();
    let addr = format!("127.0.0.1:{port}");
    let barrier = Arc::new(Barrier::new(2));
    let b = barrier.clone();
    let h = thread::spawn(move || {
        b.wait();
        let _ = server.serve(&addr);
    });
    barrier.wait();
    thread::sleep(Duration::from_millis(400));
    h
}

#[test]
fn h1_path_traversal_does_not_write_outside_tenant_root() {
    let env = EnvGuard::acquire(AUTH_ENV);
    env.remove("FLUCTLIGHT_API_KEYS");
    env.set("FLUCTLIGHT_REQUIRE_AUTH", "false");

    let tmp = tempdir().unwrap();
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&home).unwrap();
    env.set("HOME", home.to_str().unwrap());
    env.set("USERPROFILE", home.to_str().unwrap());

    let sentinel = tmp.path().join("PWNED_OUTSIDE_ROOT");
    let sentinel_str = sentinel.to_str().unwrap().replace('\\', "/");

    let brain = tmp.path().join("default_brain");
    let _h = start(brain, 39141);

    let body = format!(
        r#"{{"tenant_id":"{sentinel_str}","content":"traversal-poc","context":"x","salience_hint":0.9}}"#
    );
    let (status, resp) = post(39141, "/api/v1/experience", &body, None);
    println!(
        "H1: status={status} resp={} sentinel={}",
        resp.trim(),
        sentinel.exists()
    );
    assert!(
        !sentinel.exists(),
        "H1: brain must not be created outside tenant root"
    );
    assert_ne!(
        status, 200,
        "H1: open-mode must not honor attacker tenant_id"
    );
}

#[test]
fn h2_admin_key_cannot_cross_tenant() {
    let env = EnvGuard::acquire(AUTH_ENV);
    env.set("FLUCTLIGHT_API_KEYS", "tenant_a:adminA:admin");
    env.set("FLUCTLIGHT_REQUIRE_AUTH", "true");
    let tmp = tempdir().unwrap();
    env.set("HOME", tmp.path().to_str().unwrap());
    env.set("USERPROFILE", tmp.path().to_str().unwrap());

    let _h = start(tmp.path().join("default_brain"), 39142);

    let body = r#"{"content":"cross-tenant","context":"x","salience_hint":0.5}"#;
    let (status, resp) = post(
        39142,
        "/api/v1/tenants/tenant_b/experience",
        body,
        Some("adminA"),
    );
    println!("H2: status={status} resp={}", resp.trim());
    assert_eq!(
        status, 403,
        "H2: per-tenant admin must not write other tenants"
    );
}

#[test]
fn m1_unknown_role_does_not_grant_write() {
    let env = EnvGuard::acquire(AUTH_ENV);
    env.set("FLUCTLIGHT_API_KEYS", "tenant_a:keyX:superuser");
    env.set("FLUCTLIGHT_REQUIRE_AUTH", "true");
    let tmp = tempdir().unwrap();
    env.set("HOME", tmp.path().to_str().unwrap());
    env.set("USERPROFILE", tmp.path().to_str().unwrap());

    let _h = start(tmp.path().join("default_brain"), 39143);

    let body = r#"{"content":"x","context":"x","salience_hint":0.5}"#;
    let (status, resp) = post(39143, "/api/v1/experience", &body, Some("keyX"));
    println!("M1: status={status} resp={}", resp.trim());
    assert_eq!(status, 401, "M1: unknown role must not authorize");
}

#[test]
fn control_read_role_cannot_write() {
    let env = EnvGuard::acquire(AUTH_ENV);
    env.set("FLUCTLIGHT_API_KEYS", "tenant_a:readK:read");
    env.set("FLUCTLIGHT_REQUIRE_AUTH", "true");
    let tmp = tempdir().unwrap();
    env.set("HOME", tmp.path().to_str().unwrap());
    env.set("USERPROFILE", tmp.path().to_str().unwrap());

    let _h = start(tmp.path().join("default_brain"), 39144);

    let body = r#"{"content":"x","context":"x","salience_hint":0.5}"#;
    let (status, resp) = post(39144, "/api/v1/experience", &body, Some("readK"));
    println!("CONTROL: status={status} resp={}", resp.trim());
    assert_ne!(status, 200, "CONTROL: read key must be denied write");
}

#[test]
fn platform_can_provision_admin_cannot() {
    let env = EnvGuard::acquire(AUTH_ENV);
    env.set(
        "FLUCTLIGHT_API_KEYS",
        "platform:platK:platform,tenant_a:adminA:admin",
    );
    env.set("FLUCTLIGHT_REQUIRE_AUTH", "true");
    let tmp = tempdir().unwrap();
    env.set("HOME", tmp.path().to_str().unwrap());
    env.set("USERPROFILE", tmp.path().to_str().unwrap());

    let _h = start(tmp.path().join("default_brain"), 39145);

    let body = r#"{"tenant_id":"new_agent"}"#;
    let (st_admin, _) = post(
        39145,
        "/api/v1/admin/tenant/provision",
        body,
        Some("adminA"),
    );
    assert_ne!(st_admin, 200, "tenant admin must not provision");

    let (st_plat, resp) = post(39145, "/api/v1/admin/tenant/provision", body, Some("platK"));
    println!("PLATFORM: status={st_plat} resp={}", resp.trim());
    assert_eq!(st_plat, 200, "platform key must provision");
}

#[test]
fn lowercase_authorization_header_accepted() {
    let env = EnvGuard::acquire(AUTH_ENV);
    env.set("FLUCTLIGHT_API_KEYS", "tenant_a:writeK:write");
    env.set("FLUCTLIGHT_REQUIRE_AUTH", "true");
    let tmp = tempdir().unwrap();
    env.set("HOME", tmp.path().to_str().unwrap());
    env.set("USERPROFILE", tmp.path().to_str().unwrap());

    let _h = start(tmp.path().join("default_brain"), 39146);

    let mut stream = TcpStream::connect("127.0.0.1:39146").unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let body = r#"{"content":"hi","context":"x","salience_hint":0.5}"#;
    let req = format!(
        "POST /api/v1/experience HTTP/1.1\r\nhost: localhost\r\ncontent-type: application/json\r\ncontent-length: {}\r\nauthorization: Bearer writeK\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(req.as_bytes()).unwrap();
    let mut buf = vec![0u8; 8192];
    let n = stream.read(&mut buf).unwrap();
    let resp = String::from_utf8_lossy(&buf[..n]);
    let status = resp
        .lines()
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .unwrap_or("0");
    println!("M4: {resp}");
    assert_eq!(status, "200", "lowercase authorization must work");
}
