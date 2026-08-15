//! HTTP integration tests — auth, tenant binding, consolidate.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Duration;

use fluctlightdb::test_env::EnvGuard;
use fluctlightdb::{
    request_shutdown, reset_shutdown_for_tests, BeginSwarm, BrainServer, CiteMemories, ClaimSlot,
    EvidenceReceipt, EvidenceResult, MemoryBundle, MemoryExposure, RecordEvidence, ReportAttempt,
    SwarmTransaction, WorkerSlot, WorkerStatus,
};
use tempfile::tempdir;

const AUTH_ENV: &[&str] = &[
    "FLUCTLIGHT_API_KEYS",
    "FLUCTLIGHT_REQUIRE_AUTH",
    "FLUCTLIGHT_WAL_FSYNC",
    "FLUCTLIGHT_TENANT_ROOT",
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

fn start_server(brain: std::path::PathBuf, keys: &str, port: u16) -> std::thread::JoinHandle<()> {
    reset_shutdown_for_tests();
    // Caller must hold EnvGuard for AUTH_ENV.
    std::env::set_var("FLUCTLIGHT_API_KEYS", keys);
    std::env::set_var("FLUCTLIGHT_REQUIRE_AUTH", "true");
    let tenant_root = brain
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."))
        .join("tenant-root");
    std::env::set_var("FLUCTLIGHT_TENANT_ROOT", tenant_root);
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
    handle
}

#[test]
fn serve_auth_and_consolidate() {
    let env = EnvGuard::acquire(AUTH_ENV);
    env.set("FLUCTLIGHT_API_KEYS", "default:testkey:admin");
    env.set("FLUCTLIGHT_REQUIRE_AUTH", "true");
    env.set("FLUCTLIGHT_WAL_FSYNC", "always");
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
    drop(env);
}

#[test]
fn serve_cross_tenant_path_forbidden() {
    let _env = EnvGuard::acquire(AUTH_ENV);
    let dir = tempdir().unwrap();
    let port = 18793u16;
    let handle = start_server(
        dir.path().join("brain"),
        "tenant_a:key_a:write,tenant_b:key_b:write",
        port,
    );

    let exp = r#"{"content":"tenant a secret","context":"iso","salience":0.8}"#;
    let (s_write, write_body) = post(port, "/api/v1/experience", exp, Some("key_a"));
    assert_eq!(s_write, 200, "tenant write failed: {write_body}");

    let (s_forbidden, body) = post(port, "/api/v1/tenants/tenant_a/status", "{}", Some("key_b"));
    assert_eq!(s_forbidden, 403, "tenant_b must not read tenant_a: {body}");

    request_shutdown();
    let _ = handle.join();
}

#[test]
fn serve_read_role_cannot_write() {
    let _env = EnvGuard::acquire(AUTH_ENV);
    let dir = tempdir().unwrap();
    let port = 18794u16;
    let handle = start_server(dir.path().join("brain"), "tenant_a:read_only:read", port);

    let (s_status, status_body) = post(port, "/api/v1/status", "{}", Some("read_only"));
    assert_eq!(s_status, 200, "status failed: {status_body}");

    let exp = r#"{"content":"should fail","context":"rbac","salience":0.5}"#;
    let (s_write, body) = post(port, "/api/v1/experience", exp, Some("read_only"));
    assert_eq!(s_write, 403, "read role must not write: {body}");

    request_shutdown();
    let _ = handle.join();
}

#[test]
fn serve_swarm_lifecycle_enforces_verifier_role() {
    let _env = EnvGuard::acquire(AUTH_ENV);
    let dir = tempdir().unwrap();
    let port = 18795u16;
    let handle = start_server(
        dir.path().join("brain"),
        "default:admin_key:admin,default:worker_key:write",
        port,
    );
    let swarm_id = uuid::Uuid::new_v4();
    let memory_id = uuid::Uuid::new_v4();
    let mut allocations = std::collections::HashMap::new();
    allocations.insert(
        "slot-a".into(),
        MemoryBundle {
            episodic_memories: vec![MemoryExposure {
                engram_id: memory_id,
                content: "use the actor strategy".into(),
                score: 0.9,
                strategy_tags: vec!["actor".into()],
            }],
            strict_id_disjoint: true,
            ..MemoryBundle::default()
        },
    );
    let begin = SwarmTransaction::Begin(BeginSwarm {
        transaction_id: uuid::Uuid::new_v4(),
        swarm_id,
        project_id: "fluctlight".into(),
        objective_digest: "sha256:objective".into(),
        repository_identity: "repo".into(),
        base_commit: "abc123".into(),
        policy_version: "v1".into(),
        roster: vec![WorkerSlot {
            slot_id: "slot-a".into(),
            role: "worker".into(),
            agent_id: None,
            worktree: None,
            status: WorkerStatus::Declared,
        }],
        allocations,
    });
    let body = serde_json::json!({"transaction": begin}).to_string();
    let (status, response) = post(port, "/api/v1/swarm/begin", &body, Some("admin_key"));
    assert_eq!(status, 200, "begin failed: {response}");

    let claim = SwarmTransaction::Claim(ClaimSlot {
        transaction_id: uuid::Uuid::new_v4(),
        swarm_id,
        slot_id: "slot-a".into(),
        agent_id: "agent-7".into(),
        worktree: "/tmp/worktree-7".into(),
    });
    let body = serde_json::json!({"transaction": claim}).to_string();
    let (status, response) = post(port, "/api/v1/swarm/claim", &body, Some("worker_key"));
    assert_eq!(status, 200, "claim failed: {response}");
    assert!(response.contains(&memory_id.to_string()));

    let cite = SwarmTransaction::Cite(CiteMemories {
        transaction_id: uuid::Uuid::new_v4(),
        swarm_id,
        slot_id: "slot-a".into(),
        memory_ids: vec![memory_id],
    });
    let body = serde_json::json!({"transaction": cite}).to_string();
    assert_eq!(
        post(port, "/api/v1/swarm/cite", &body, Some("worker_key")).0,
        200
    );

    let report = SwarmTransaction::Report(ReportAttempt {
        transaction_id: uuid::Uuid::new_v4(),
        swarm_id,
        slot_id: "slot-a".into(),
        result_tree: "tree-123".into(),
        summary: "tests pass".into(),
    });
    let body = serde_json::json!({"transaction": report}).to_string();
    assert_eq!(
        post(port, "/api/v1/swarm/attempt", &body, Some("worker_key")).0,
        200
    );

    let evidence = SwarmTransaction::Evidence(RecordEvidence {
        transaction_id: uuid::Uuid::new_v4(),
        swarm_id,
        slot_id: "slot-a".into(),
        receipt: EvidenceReceipt {
            result: EvidenceResult::Success,
            source_uri: "test://cargo-test".into(),
            command_digest: "sha256:command".into(),
        },
    });
    let body = serde_json::json!({"transaction": evidence}).to_string();
    assert_eq!(
        post(port, "/api/v1/swarm/evidence", &body, Some("worker_key")).0,
        403
    );
    let (status, response) = post(
        port,
        "/api/v1/swarm/evidence",
        &body,
        Some("admin_key"),
    );
    assert_eq!(status, 200, "evidence failed: {response}");
    assert!(response.contains(&memory_id.to_string()));

    request_shutdown();
    let _ = handle.join();
}
