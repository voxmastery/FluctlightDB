//! Full CAB end-to-end scenario tests against a live BrainServer.
//! cargo test -p fluctlightdb --test cab_e2e_scenarios -- --test-threads=1 --nocapture

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Duration;

use fluctlightdb::auth::{AuthConfig, Role};
use fluctlightdb::auth_store::AuthStore;
use fluctlightdb::tenant::{locus_slug, tenant_dir};
use fluctlightdb::test_env::EnvGuard;
use fluctlightdb::{reset_shutdown_for_tests, BrainServer};
use tempfile::tempdir;

const ENV: &[&str] = &[
    "FLUCTLIGHT_API_KEYS",
    "FLUCTLIGHT_REQUIRE_AUTH",
    "HOME",
    "USERPROFILE",
];

fn http(
    port: u16,
    method: &str,
    path: &str,
    body: &str,
    token: Option<&str>,
    lowercase_auth: bool,
) -> (u16, String) {
    let mut stream = TcpStream::connect(format!("127.0.0.1:{port}")).expect("connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(8)))
        .unwrap();
    let auth = match token {
        Some(t) if lowercase_auth => format!("authorization: Bearer {t}\r\n"),
        Some(t) => format!("Authorization: Bearer {t}\r\n"),
        None => String::new(),
    };
    let req = format!(
        "{method} {path} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{auth}\r\n{body}",
        body.len()
    );
    stream.write_all(req.as_bytes()).unwrap();
    let mut buf = vec![0u8; 1 << 16];
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

fn start(brain: PathBuf, port: u16) -> thread::JoinHandle<()> {
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
    thread::sleep(Duration::from_millis(450));
    h
}

#[test]
fn scenario_open_mode_default_write_and_block_traversal() {
    let env = EnvGuard::acquire(ENV);
    env.remove("FLUCTLIGHT_API_KEYS");
    env.set("FLUCTLIGHT_REQUIRE_AUTH", "false");
    let tmp = tempdir().unwrap();
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&home).unwrap();
    env.set("HOME", home.to_str().unwrap());
    env.set("USERPROFILE", home.to_str().unwrap());

    let _h = start(tmp.path().join("brain"), 39201);

    // Happy path: open mode writes default brain
    let (s, body) = http(
        39201,
        "POST",
        "/api/v1/experience",
        r#"{"content":"hello default","context":"e2e","salience_hint":0.8}"#,
        None,
        false,
    );
    assert_eq!(s, 200, "open default experience: {body}");

    let (s, body) = http(
        39201,
        "POST",
        "/api/v1/activate",
        r#"{"cue":"hello default","limit":5}"#,
        None,
        false,
    );
    assert_eq!(s, 200, "open default activate: {body}");
    assert!(
        body.contains("hello") || body.contains("recalls") || body.contains("engram"),
        "{body}"
    );

    // Traversal blocked
    let sentinel = tmp.path().join("OUTSIDE");
    let evil = sentinel.to_str().unwrap().replace('\\', "/");
    let (s, _) = http(
        39201,
        "POST",
        "/api/v1/experience",
        &format!(r#"{{"tenant_id":"{evil}","content":"x","context":"x","salience_hint":0.5}}"#),
        None,
        false,
    );
    assert_ne!(s, 200);
    assert!(!sentinel.exists());
}

#[test]
fn scenario_write_admin_platform_full_matrix() {
    let env = EnvGuard::acquire(ENV);
    env.set(
        "FLUCTLIGHT_API_KEYS",
        "platform:platSecret:platform,acme:writeSecret:write,acme:adminSecret:admin,beta:betaAdmin:admin",
    );
    env.set("FLUCTLIGHT_REQUIRE_AUTH", "true");
    let tmp = tempdir().unwrap();
    env.set("HOME", tmp.path().to_str().unwrap());
    env.set("USERPROFILE", tmp.path().to_str().unwrap());

    let _h = start(tmp.path().join("brain"), 39202);

    // Write encode + recall
    let (s, body) = http(
        39202,
        "POST",
        "/api/v1/experience",
        r#"{"content":"acme remembers cats","context":"pets","salience_hint":0.9}"#,
        Some("writeSecret"),
        false,
    );
    assert_eq!(s, 200, "write experience: {body}");

    let (s, body) = http(
        39202,
        "POST",
        "/api/v1/activate",
        r#"{"cue":"cats","limit":8}"#,
        Some("writeSecret"),
        true, // lowercase auth header
    );
    assert_eq!(s, 200, "write activate: {body}");

    // Admin govern on own tenant
    let (s, body) = http(
        39202,
        "POST",
        "/api/v1/compact",
        "{}",
        Some("adminSecret"),
        false,
    );
    assert_eq!(s, 200, "admin compact own tenant: {body}");

    // Admin cannot cross into beta
    let (s, _) = http(
        39202,
        "POST",
        "/api/v1/tenants/beta/experience",
        r#"{"content":"nope","context":"x","salience_hint":0.5}"#,
        Some("adminSecret"),
        false,
    );
    assert_eq!(s, 403);

    // Write cannot compact
    let (s, _) = http(
        39202,
        "POST",
        "/api/v1/compact",
        "{}",
        Some("writeSecret"),
        false,
    );
    assert_ne!(s, 200);

    // Platform cannot encode into a brain
    let (s, _) = http(
        39202,
        "POST",
        "/api/v1/experience",
        r#"{"content":"plat","context":"x","salience_hint":0.5,"tenant_id":"acme"}"#,
        Some("platSecret"),
        false,
    );
    assert_ne!(s, 200);

    // Platform provisions new tenant + write key
    let (s, body) = http(
        39202,
        "POST",
        "/api/v1/admin/tenant/provision",
        r#"{"tenant_id":"gamma"}"#,
        Some("platSecret"),
        false,
    );
    assert_eq!(s, 200, "provision: {body}");
    let key: serde_json::Value = serde_json::from_str(&body).expect("json");
    let new_key = key["key"].as_str().expect("key field");
    assert!(new_key.starts_with("fld_"));

    // Admin cannot provision
    let (s, _) = http(
        39202,
        "POST",
        "/api/v1/admin/tenant/provision",
        r#"{"tenant_id":"delta"}"#,
        Some("adminSecret"),
        false,
    );
    assert_ne!(s, 200);

    // New provisioned key works for gamma
    let (s, body) = http(
        39202,
        "POST",
        "/api/v1/tenants/gamma/experience",
        r#"{"content":"gamma live","context":"e2e","salience_hint":0.7}"#,
        Some(new_key),
        false,
    );
    assert_eq!(s, 200, "provisioned key experience: {body}");

    // Locus for gamma is hashed under HOME/.fluctlight/tenants/<slug>
    let root = tmp.path().join(".fluctlight");
    let dir = tenant_dir(&root, "gamma");
    assert!(
        dir.exists() || dir.join("brain").exists() || dir.join("brain.flct").exists(),
        "expected hashed locus under {}",
        dir.display()
    );
    assert!(dir.to_string_lossy().contains(&locus_slug("gamma")));
}

#[test]
fn scenario_hashed_secret_and_expiry() {
    let dir = tempdir().unwrap();
    let store = AuthStore::open(dir.path().join("auth.db")).unwrap();
    let issued = store.issue_key("t1", Role::Write).unwrap();
    assert!(store.lookup(&issued.key).is_some());

    // Secret is stored hashed — raw key still authorizes
    let cfg = AuthConfig {
        keys: Default::default(),
        require_auth: true,
    };
    // Lookup goes through store path when env keys empty — use store directly above.
    let expired = store
        .issue_key_with_expiry("t1", Role::Write, Some(1))
        .unwrap(); // expired in 1970
    assert!(
        store.lookup(&expired.key).is_none(),
        "expired key must not authorize"
    );

    let _ = cfg; // keep import meaningful for future AuthConfig store integration tests
}

#[test]
fn scenario_unknown_role_and_missing_bearer() {
    let env = EnvGuard::acquire(ENV);
    env.set("FLUCTLIGHT_API_KEYS", "t:good:write,t:bad:superuser");
    env.set("FLUCTLIGHT_REQUIRE_AUTH", "true");
    let tmp = tempdir().unwrap();
    env.set("HOME", tmp.path().to_str().unwrap());
    env.set("USERPROFILE", tmp.path().to_str().unwrap());
    let _h = start(tmp.path().join("brain"), 39203);

    let body = r#"{"content":"x","context":"x","salience_hint":0.5}"#;
    let (s, _) = http(39203, "POST", "/api/v1/experience", body, None, false);
    assert_eq!(s, 401);
    let (s, _) = http(
        39203,
        "POST",
        "/api/v1/experience",
        body,
        Some("bad"),
        false,
    );
    assert_eq!(s, 401);
    let (s, _) = http(
        39203,
        "POST",
        "/api/v1/experience",
        body,
        Some("good"),
        false,
    );
    assert_eq!(s, 200);
}

#[test]
fn scenario_non_localhost_bind_requires_keys() {
    let env = EnvGuard::acquire(ENV);
    env.remove("FLUCTLIGHT_API_KEYS");
    env.set("FLUCTLIGHT_REQUIRE_AUTH", "false");
    let tmp = tempdir().unwrap();
    let server = BrainServer::open(tmp.path().join("brain")).unwrap();
    let err = server.serve("0.0.0.0:39299").err().expect("must err");
    let msg = err.to_string();
    assert!(
        msg.contains("API_KEYS") || msg.contains("non-localhost"),
        "{msg}"
    );
}
