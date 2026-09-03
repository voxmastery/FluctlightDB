use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Duration;

use fluctlightdb::test_env::EnvGuard;
use fluctlightdb::{request_shutdown, reset_shutdown_for_tests, BrainServer};
use tempfile::tempdir;

const SERVER_ENV: &[&str] = &[
    "FLUCTLIGHT_API_KEYS",
    "FLUCTLIGHT_REQUIRE_AUTH",
    "FLUCTLIGHT_SERVER_MODE",
    "FLUCTLIGHT_FOVEA_INGESTION",
    "FLUCTLIGHT_REQUEST_TIMEOUT_MS",
    "FLUCTLIGHT_MAX_CONNECTIONS",
];

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn start_server(server: BrainServer, port: u16) -> thread::JoinHandle<()> {
    reset_shutdown_for_tests();
    let handle = thread::spawn(move || {
        server.serve(&format!("127.0.0.1:{port}")).unwrap();
    });
    for _ in 0..100 {
        if TcpStream::connect(("127.0.0.1", port)).is_ok() {
            return handle;
        }
        thread::sleep(Duration::from_millis(10));
    }
    panic!("server failed to start");
}

fn raw_request(port: u16, request: &[u8]) -> Vec<u8> {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    stream.write_all(request).unwrap();
    let mut response = Vec::new();
    stream.read_to_end(&mut response).unwrap();
    response
}

fn raw_malformed_request(port: u16, request: &[u8]) -> Vec<u8> {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_millis(10)))
        .unwrap();
    stream.write_all(request).unwrap();
    let _ = stream.shutdown(std::net::Shutdown::Write);
    let mut response = Vec::new();
    let _ = stream.read_to_end(&mut response);
    response
}

fn response_status(response: &[u8]) -> u16 {
    String::from_utf8_lossy(response)
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|status| status.parse().ok())
        .unwrap_or_default()
}

#[test]
fn production_mode_rejects_missing_auth_even_on_loopback() {
    let env = EnvGuard::acquire(SERVER_ENV);
    env.set("FLUCTLIGHT_SERVER_MODE", "production");
    env.remove("FLUCTLIGHT_API_KEYS");
    env.remove("FLUCTLIGHT_REQUIRE_AUTH");

    let dir = tempdir().unwrap();
    let server = BrainServer::open(dir.path().join("brain")).unwrap();
    let error = server
        .validate_serve_config("127.0.0.1:0")
        .expect_err("production must require explicit auth");

    assert!(error
        .to_string()
        .contains("production mode requires authentication"));
}

#[test]
fn development_open_mode_must_be_explicit_and_is_allowed_on_loopback() {
    let env = EnvGuard::acquire(SERVER_ENV);
    env.set("FLUCTLIGHT_SERVER_MODE", "development");
    env.remove("FLUCTLIGHT_API_KEYS");
    env.set("FLUCTLIGHT_REQUIRE_AUTH", "false");

    let dir = tempdir().unwrap();
    let server = BrainServer::open(dir.path().join("brain")).unwrap();

    server
        .validate_serve_config("127.0.0.1:0")
        .expect("explicit development open mode should be allowed");
}

#[test]
fn hyper_rejects_content_length_with_transfer_encoding() {
    let env = EnvGuard::acquire(SERVER_ENV);
    env.set("FLUCTLIGHT_SERVER_MODE", "development");
    env.remove("FLUCTLIGHT_API_KEYS");
    env.set("FLUCTLIGHT_REQUIRE_AUTH", "false");

    let dir = tempdir().unwrap();
    let port = free_port();
    let handle = start_server(BrainServer::open(dir.path().join("brain")).unwrap(), port);
    let request = b"POST /api/v1/status HTTP/1.1\r\n\
Host: localhost\r\n\
Content-Type: application/json\r\n\
Content-Length: 2\r\n\
Transfer-Encoding: chunked\r\n\
Connection: close\r\n\r\n\
{}";
    let response = raw_request(port, request);

    request_shutdown();
    handle.join().unwrap();
    let status = String::from_utf8_lossy(&response);
    assert!(
        status.starts_with("HTTP/1.1 400"),
        "ambiguous framing must be rejected by Hyper: {status}"
    );
}

#[test]
fn fovea_read_is_disabled_without_separate_ingestion_capability() {
    let env = EnvGuard::acquire(SERVER_ENV);
    env.set("FLUCTLIGHT_SERVER_MODE", "development");
    env.remove("FLUCTLIGHT_API_KEYS");
    env.set("FLUCTLIGHT_REQUIRE_AUTH", "false");
    env.remove("FLUCTLIGHT_FOVEA_INGESTION");

    let dir = tempdir().unwrap();
    let source = dir.path().join("source.txt");
    std::fs::write(&source, "private source material").unwrap();
    let body = serde_json::json!({"file_path": source}).to_string();
    let request = format!(
        "POST /api/v1/fovea-read HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    let port = free_port();
    let handle = start_server(BrainServer::open(dir.path().join("brain")).unwrap(), port);
    let response = raw_request(port, request.as_bytes());

    request_shutdown();
    handle.join().unwrap();
    assert_eq!(
        response_status(&response),
        403,
        "{:?}",
        String::from_utf8_lossy(&response)
    );
    assert!(String::from_utf8_lossy(&response).contains("fovea ingestion disabled"));
}

#[test]
fn oversized_request_body_is_rejected() {
    let env = EnvGuard::acquire(SERVER_ENV);
    env.set("FLUCTLIGHT_SERVER_MODE", "development");
    env.remove("FLUCTLIGHT_API_KEYS");
    env.set("FLUCTLIGHT_REQUIRE_AUTH", "false");

    let dir = tempdir().unwrap();
    let body = "x".repeat(1_048_577);
    let request = format!(
        "POST /api/v1/status HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    let port = free_port();
    let handle = start_server(BrainServer::open(dir.path().join("brain")).unwrap(), port);
    let response = raw_request(port, request.as_bytes());

    request_shutdown();
    handle.join().unwrap();
    assert_eq!(
        response_status(&response),
        413,
        "{:?}",
        String::from_utf8_lossy(&response)
    );
}

#[test]
fn utf8_codepoint_split_across_tcp_writes_is_preserved() {
    let env = EnvGuard::acquire(SERVER_ENV);
    env.set("FLUCTLIGHT_SERVER_MODE", "development");
    env.remove("FLUCTLIGHT_API_KEYS");
    env.set("FLUCTLIGHT_REQUIRE_AUTH", "false");

    let dir = tempdir().unwrap();
    let port = free_port();
    let handle = start_server(BrainServer::open(dir.path().join("brain")).unwrap(), port);
    let body = r#"{"content":"boundary 🧠 café","context":"utf8"}"#;
    let body_bytes = body.as_bytes();
    let emoji = body_bytes
        .windows(4)
        .position(|bytes| bytes == "🧠".as_bytes())
        .unwrap();
    let split = emoji + 2;
    let headers = format!(
        "POST /api/v1/experience HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body_bytes.len()
    );
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream.write_all(headers.as_bytes()).unwrap();
    stream.write_all(&body_bytes[..split]).unwrap();
    thread::sleep(Duration::from_millis(10));
    stream.write_all(&body_bytes[split..]).unwrap();
    let mut response = Vec::new();
    stream.read_to_end(&mut response).unwrap();

    request_shutdown();
    handle.join().unwrap();
    assert_eq!(
        response_status(&response),
        200,
        "{:?}",
        String::from_utf8_lossy(&response)
    );
}

#[test]
fn excessive_header_count_is_rejected() {
    let env = EnvGuard::acquire(SERVER_ENV);
    env.set("FLUCTLIGHT_SERVER_MODE", "development");
    env.remove("FLUCTLIGHT_API_KEYS");
    env.set("FLUCTLIGHT_REQUIRE_AUTH", "false");

    let dir = tempdir().unwrap();
    let mut request = String::from("POST /api/v1/status HTTP/1.1\r\nHost: localhost\r\n");
    for index in 0..65 {
        request.push_str(&format!("X-Test-{index}: value\r\n"));
    }
    request.push_str("Content-Length: 2\r\nConnection: close\r\n\r\n{}");
    let port = free_port();
    let handle = start_server(BrainServer::open(dir.path().join("brain")).unwrap(), port);
    let response = raw_request(port, request.as_bytes());

    request_shutdown();
    handle.join().unwrap();
    assert_eq!(
        response_status(&response),
        431,
        "{:?}",
        String::from_utf8_lossy(&response)
    );
}

#[test]
fn absolute_timeout_covers_slow_request_body() {
    let env = EnvGuard::acquire(SERVER_ENV);
    env.set("FLUCTLIGHT_SERVER_MODE", "development");
    env.remove("FLUCTLIGHT_API_KEYS");
    env.set("FLUCTLIGHT_REQUIRE_AUTH", "false");
    env.set("FLUCTLIGHT_REQUEST_TIMEOUT_MS", "50");

    let dir = tempdir().unwrap();
    let port = free_port();
    let handle = start_server(BrainServer::open(dir.path().join("brain")).unwrap(), port);
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_millis(500)))
        .unwrap();
    stream
        .write_all(
            b"POST /api/v1/status HTTP/1.1\r\nHost: localhost\r\nContent-Length: 10\r\nConnection: close\r\n\r\n",
        )
        .unwrap();
    let mut response = Vec::new();
    stream
        .read_to_end(&mut response)
        .expect("server must terminate a slow request with a response");

    request_shutdown();
    handle.join().unwrap();
    assert_eq!(
        response_status(&response),
        504,
        "{:?}",
        String::from_utf8_lossy(&response)
    );
}

#[test]
fn saturated_server_load_sheds_instead_of_queueing() {
    let env = EnvGuard::acquire(SERVER_ENV);
    env.set("FLUCTLIGHT_SERVER_MODE", "development");
    env.remove("FLUCTLIGHT_API_KEYS");
    env.set("FLUCTLIGHT_REQUIRE_AUTH", "false");
    env.set("FLUCTLIGHT_MAX_CONNECTIONS", "1");
    env.set("FLUCTLIGHT_REQUEST_TIMEOUT_MS", "1000");

    let dir = tempdir().unwrap();
    let port = free_port();
    let handle = start_server(BrainServer::open(dir.path().join("brain")).unwrap(), port);
    let mut blocker = TcpStream::connect(("127.0.0.1", port)).unwrap();
    blocker
        .write_all(
            b"POST /api/v1/status HTTP/1.1\r\nHost: localhost\r\nContent-Length: 10\r\nConnection: close\r\n\r\n",
        )
        .unwrap();
    thread::sleep(Duration::from_millis(30));
    let response = raw_request(
        port,
        b"POST /api/v1/status HTTP/1.1\r\nHost: localhost\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}",
    );
    drop(blocker);

    request_shutdown();
    handle.join().unwrap();
    assert_eq!(
        response_status(&response),
        503,
        "{:?}",
        String::from_utf8_lossy(&response)
    );
}

#[test]
fn responses_include_a_request_id() {
    let env = EnvGuard::acquire(SERVER_ENV);
    env.set("FLUCTLIGHT_SERVER_MODE", "development");
    env.remove("FLUCTLIGHT_API_KEYS");
    env.set("FLUCTLIGHT_REQUIRE_AUTH", "false");

    let dir = tempdir().unwrap();
    let port = free_port();
    let handle = start_server(BrainServer::open(dir.path().join("brain")).unwrap(), port);
    let response = raw_request(
        port,
        b"GET /health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
    );

    request_shutdown();
    handle.join().unwrap();
    let response = String::from_utf8_lossy(&response).to_ascii_lowercase();
    assert!(response.contains("\r\nx-request-id:"), "{response}");
}

#[test]
fn metrics_are_not_public_in_development_open_mode() {
    let env = EnvGuard::acquire(SERVER_ENV);
    env.set("FLUCTLIGHT_SERVER_MODE", "development");
    env.remove("FLUCTLIGHT_API_KEYS");
    env.set("FLUCTLIGHT_REQUIRE_AUTH", "false");

    let dir = tempdir().unwrap();
    let port = free_port();
    let handle = start_server(BrainServer::open(dir.path().join("brain")).unwrap(), port);
    let response = raw_request(
        port,
        b"GET /metrics HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
    );

    request_shutdown();
    handle.join().unwrap();
    assert_eq!(
        response_status(&response),
        403,
        "{:?}",
        String::from_utf8_lossy(&response)
    );
}

#[test]
fn unknown_server_mode_is_rejected_at_startup() {
    let env = EnvGuard::acquire(SERVER_ENV);
    env.set("FLUCTLIGHT_SERVER_MODE", "prodution");
    env.set("FLUCTLIGHT_REQUIRE_AUTH", "true");
    env.set("FLUCTLIGHT_API_KEYS", "default:key:admin");

    let dir = tempdir().unwrap();
    let server = BrainServer::open(dir.path().join("brain")).unwrap();
    let error = server
        .validate_serve_config("127.0.0.1:0")
        .expect_err("unknown mode must not silently select a policy");

    assert!(error.to_string().contains("FLUCTLIGHT_SERVER_MODE"));
}

#[test]
fn hyper_rejects_conflicting_duplicate_content_length() {
    let env = EnvGuard::acquire(SERVER_ENV);
    env.set("FLUCTLIGHT_SERVER_MODE", "development");
    env.remove("FLUCTLIGHT_API_KEYS");
    env.set("FLUCTLIGHT_REQUIRE_AUTH", "false");

    let dir = tempdir().unwrap();
    let port = free_port();
    let handle = start_server(BrainServer::open(dir.path().join("brain")).unwrap(), port);
    let response = raw_request(
        port,
        b"POST /api/v1/status HTTP/1.1\r\nHost: localhost\r\nContent-Length: 2\r\nContent-Length: 3\r\nConnection: close\r\n\r\n{}",
    );

    request_shutdown();
    handle.join().unwrap();
    assert_eq!(
        response_status(&response),
        400,
        "{:?}",
        String::from_utf8_lossy(&response)
    );
}

#[test]
fn duplicate_transfer_encoding_is_rejected() {
    let env = EnvGuard::acquire(SERVER_ENV);
    env.set("FLUCTLIGHT_SERVER_MODE", "development");
    env.remove("FLUCTLIGHT_API_KEYS");
    env.set("FLUCTLIGHT_REQUIRE_AUTH", "false");

    let dir = tempdir().unwrap();
    let port = free_port();
    let handle = start_server(BrainServer::open(dir.path().join("brain")).unwrap(), port);
    let response = raw_request(
        port,
        b"POST /api/v1/status HTTP/1.1\r\nHost: localhost\r\nTransfer-Encoding: chunked\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n2\r\n{}\r\n0\r\n\r\n",
    );

    request_shutdown();
    handle.join().unwrap();
    assert_eq!(
        response_status(&response),
        400,
        "{:?}",
        String::from_utf8_lossy(&response)
    );
}

#[test]
fn production_mode_accepts_explicit_auth_configuration() {
    let env = EnvGuard::acquire(SERVER_ENV);
    env.set("FLUCTLIGHT_SERVER_MODE", "production");
    env.set("FLUCTLIGHT_REQUIRE_AUTH", "true");
    env.set("FLUCTLIGHT_API_KEYS", "default:secret:admin");

    let dir = tempdir().unwrap();
    let server = BrainServer::open(dir.path().join("brain")).unwrap();

    server.validate_serve_config("127.0.0.1:0").unwrap();
}

#[test]
fn liveness_and_readiness_report_distinct_meaningful_states() {
    let env = EnvGuard::acquire(SERVER_ENV);
    env.set("FLUCTLIGHT_SERVER_MODE", "development");
    env.remove("FLUCTLIGHT_API_KEYS");
    env.set("FLUCTLIGHT_REQUIRE_AUTH", "false");

    let dir = tempdir().unwrap();
    let port = free_port();
    let handle = start_server(BrainServer::open(dir.path().join("brain")).unwrap(), port);
    let live = raw_request(
        port,
        b"GET /live HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
    );
    let ready = raw_request(
        port,
        b"GET /ready HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
    );

    request_shutdown();
    handle.join().unwrap();
    assert_eq!(response_status(&live), 200);
    assert!(String::from_utf8_lossy(&live).contains("\"status\":\"live\""));
    assert_eq!(response_status(&ready), 200);
    assert!(String::from_utf8_lossy(&ready).contains("\"ready\":true"));
}

#[test]
fn platform_capability_can_read_metrics() {
    let env = EnvGuard::acquire(SERVER_ENV);
    env.set("FLUCTLIGHT_SERVER_MODE", "production");
    env.set("FLUCTLIGHT_REQUIRE_AUTH", "true");
    env.set("FLUCTLIGHT_API_KEYS", "platform:metrics-key:platform");

    let dir = tempdir().unwrap();
    let port = free_port();
    let handle = start_server(BrainServer::open(dir.path().join("brain")).unwrap(), port);
    let response = raw_request(
        port,
        b"GET /metrics HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer metrics-key\r\nConnection: close\r\n\r\n",
    );

    request_shutdown();
    handle.join().unwrap();
    assert_eq!(
        response_status(&response),
        200,
        "{:?}",
        String::from_utf8_lossy(&response)
    );
    assert!(String::from_utf8_lossy(&response).contains("fluctlight_experiences_total"));
}

#[test]
fn explicit_fovea_ingestion_capability_enables_fovea_read() {
    let env = EnvGuard::acquire(SERVER_ENV);
    env.set("FLUCTLIGHT_SERVER_MODE", "development");
    env.remove("FLUCTLIGHT_API_KEYS");
    env.set("FLUCTLIGHT_REQUIRE_AUTH", "false");
    env.set("FLUCTLIGHT_FOVEA_INGESTION", "true");

    let dir = tempdir().unwrap();
    let source = dir.path().join("source.txt");
    std::fs::write(&source, "explicitly permitted source").unwrap();
    let body = serde_json::json!({"file_path": source, "dry_run": true}).to_string();
    let request = format!(
        "POST /api/v1/fovea-read HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    let port = free_port();
    let handle = start_server(BrainServer::open(dir.path().join("brain")).unwrap(), port);
    let response = raw_request(port, request.as_bytes());

    request_shutdown();
    handle.join().unwrap();
    assert_eq!(
        response_status(&response),
        200,
        "{:?}",
        String::from_utf8_lossy(&response)
    );
}

#[test]
fn graceful_shutdown_drains_an_in_flight_request() {
    let env = EnvGuard::acquire(SERVER_ENV);
    env.set("FLUCTLIGHT_SERVER_MODE", "development");
    env.remove("FLUCTLIGHT_API_KEYS");
    env.set("FLUCTLIGHT_REQUIRE_AUTH", "false");
    env.set("FLUCTLIGHT_REQUEST_TIMEOUT_MS", "1000");

    let dir = tempdir().unwrap();
    let port = free_port();
    let handle = start_server(BrainServer::open(dir.path().join("brain")).unwrap(), port);
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    stream
        .write_all(
            b"POST /api/v1/status HTTP/1.1\r\nHost: localhost\r\nContent-Length: 2\r\nConnection: close\r\n\r\n",
        )
        .unwrap();
    thread::sleep(Duration::from_millis(30));
    request_shutdown();
    stream.write_all(b"{}").unwrap();
    let mut response = Vec::new();
    stream.read_to_end(&mut response).unwrap();
    handle.join().unwrap();

    assert_eq!(
        response_status(&response),
        200,
        "{:?}",
        String::from_utf8_lossy(&response)
    );
}

#[test]
fn absolute_timeout_covers_blocked_business_dispatch() {
    let env = EnvGuard::acquire(SERVER_ENV);
    env.set("FLUCTLIGHT_SERVER_MODE", "development");
    env.remove("FLUCTLIGHT_API_KEYS");
    env.set("FLUCTLIGHT_REQUIRE_AUTH", "false");
    env.set("FLUCTLIGHT_REQUEST_TIMEOUT_MS", "50");

    let dir = tempdir().unwrap();
    let server = BrainServer::open(dir.path().join("brain")).unwrap();
    let blocker = server.clone();
    let locked = Arc::new(Barrier::new(2));
    let lock_signal = locked.clone();
    let lock_thread = thread::spawn(move || {
        blocker
            .with_brain_write("default", |_| {
                lock_signal.wait();
                thread::sleep(Duration::from_millis(250));
                Ok(())
            })
            .unwrap();
    });
    locked.wait();

    let port = free_port();
    let handle = start_server(server, port);
    let response = raw_request(
        port,
        b"POST /api/v1/status HTTP/1.1\r\nHost: localhost\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}",
    );

    request_shutdown();
    handle.join().unwrap();
    lock_thread.join().unwrap();
    assert_eq!(
        response_status(&response),
        504,
        "{:?}",
        String::from_utf8_lossy(&response)
    );
}

#[test]
#[ignore = "100k-case release gate; run serially in release mode"]
fn deterministic_hyper_malformed_request_gate_100k() {
    let env = EnvGuard::acquire(SERVER_ENV);
    env.set("FLUCTLIGHT_SERVER_MODE", "development");
    env.remove("FLUCTLIGHT_API_KEYS");
    env.set("FLUCTLIGHT_REQUIRE_AUTH", "false");
    let dir = tempdir().unwrap();
    let port = free_port();
    let handle = start_server(BrainServer::open(dir.path().join("brain")).unwrap(), port);
    let mut seed = 0x9e37_79b9_7f4a_7c15_u64;

    for case in 0..100_000_u64 {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        let request = match seed % 8 {
            0 => format!(
                "G{}T / HTTP/1.1\r\nHost: x\r\n\r\n",
                ((seed % 95) as u8 + 32) as char
            ),
            1 => format!("GET / HTTP/1.1\r\nBad Header-{case}: x\r\n\r\n"),
            2 => format!("POST / HTTP/1.1\r\nHost: x\r\nContent-Length: -{case}\r\n\r\n"),
            3 => format!(
                "POST / HTTP/1.1\r\nHost: x\r\nContent-Length: 1\r\nContent-Length: {}\r\n\r\nx",
                case % 9 + 2
            ),
            4 => "POST / HTTP/1.1\r\nHost: x\r\nTransfer-Encoding: chunked\r\n\r\nz\r\nx\r\n0\r\n\r\n"
                .to_string(),
            5 => format!("GET /\u{7f}{case} HTTP/1.1\r\nHost: x\r\n\r\n"),
            6 => format!("HTTP/1.1 GET /{case}\r\nHost: x\r\n\r\n"),
            _ => format!("GET /{case} HTTP/9.9\r\nHost: x\r\n\r\n"),
        };
        let _ = raw_malformed_request(port, request.as_bytes());
        if case % 10_000 == 0 {
            let health = raw_request(
                port,
                b"GET /live HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
            );
            assert_eq!(response_status(&health), 200, "server died at case {case}");
        }
    }

    request_shutdown();
    handle.join().unwrap();
}
