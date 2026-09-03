#![cfg(feature = "distributed")]

use std::collections::{BTreeMap, BTreeSet};
use std::net::TcpListener;
use std::path::Path;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use fluctlightdb::control::network::TlsIdentity;
use fluctlightdb::control::service::{ControlNode, ControlNodeConfig};
use fluctlightdb::control::types::{ControlCommand, ControlRole, NodeId, TenantControlConfig};
use fluctlightdb::placement::{DurabilityPolicy, Placement};
use fluctlightdb::{request_shutdown, reset_shutdown_for_tests, BrainServer};
use rcgen::{
    BasicConstraints, Certificate, CertificateParams, ExtendedKeyUsagePurpose, IsCa, KeyPair,
    KeyUsagePurpose,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const PLATFORM_TOKEN: &str = "phase3-platform-bootstrap-token-0000000000000000";
static API_TOKEN: OnceLock<String> = OnceLock::new();

fn test_ca() -> (Certificate, KeyPair) {
    let mut params = CertificateParams::new(Vec::<String>::new()).unwrap();
    params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    params.key_usages = vec![
        KeyUsagePurpose::DigitalSignature,
        KeyUsagePurpose::KeyCertSign,
    ];
    let key = KeyPair::generate().unwrap();
    (params.self_signed(&key).unwrap(), key)
}

fn identity(node_id: NodeId, ca: &Certificate, ca_key: &KeyPair) -> TlsIdentity {
    let mut params = CertificateParams::new(vec!["localhost".into()]).unwrap();
    params.extended_key_usages = vec![
        ExtendedKeyUsagePurpose::ServerAuth,
        ExtendedKeyUsagePurpose::ClientAuth,
    ];
    let key = KeyPair::generate().unwrap();
    let certificate = params.signed_by(&key, ca, ca_key).unwrap();
    TlsIdentity {
        node_id,
        certificate_chain_der: vec![certificate.der().to_vec()],
        private_key_der: key.serialize_der(),
        ca_certificate_der: ca.der().to_vec(),
        server_name: "localhost".into(),
    }
}

async fn start_node(
    node_id: NodeId,
    data_dir: &Path,
    pepper: &[u8; 32],
    identity: TlsIdentity,
) -> ControlNode {
    ControlNode::start(ControlNodeConfig {
        node_id,
        bind_addr: "127.0.0.1:0".into(),
        data_dir: data_dir.to_path_buf(),
        cluster_pepper: pepper.to_vec(),
        tls_identity: identity,
        cluster_name: "phase-3-placement-routing".into(),
    })
    .await
    .unwrap()
}

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

async fn post(port: u16, path: &str, body: &str) -> (u16, String) {
    let api_token = API_TOKEN.get().expect("test API token initialized");
    let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .unwrap();
    let request = format!(
        "POST {path} HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {api_token}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut response = Vec::new();
    stream.read_to_end(&mut response).await.unwrap();
    let response = String::from_utf8(response).unwrap();
    let status = response
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|status| status.parse().ok())
        .unwrap_or_default();
    let body = response
        .split_once("\r\n\r\n")
        .map(|(_, body)| body.to_string())
        .unwrap_or_default();
    (status, body)
}

async fn get(port: u16, path: &str) -> (u16, String) {
    let api_token = API_TOKEN.get().expect("test API token initialized");
    let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .unwrap();
    let request = format!(
        "GET {path} HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {api_token}\r\nConnection: close\r\n\r\n"
    );
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut response = Vec::new();
    stream.read_to_end(&mut response).await.unwrap();
    let response = String::from_utf8(response).unwrap();
    let status = response
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|status| status.parse().ok())
        .unwrap_or_default();
    let body = response
        .split_once("\r\n\r\n")
        .map(|(_, body)| body.to_string())
        .unwrap_or_default();
    (status, body)
}

async fn wait_for_api(port: u16) {
    for _ in 0..100 {
        if tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .is_ok()
        {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("API did not start on port {port}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn partition_reassignment_fences_old_primary_and_routes_api_reads_and_writes() {
    let root = tempfile::tempdir().unwrap();
    let pepper = [55; 32];
    let (ca, ca_key) = test_ca();
    let node1 = Arc::new(
        start_node(
            1,
            &root.path().join("node1"),
            &pepper,
            identity(1, &ca, &ca_key),
        )
        .await,
    );
    let node2 = Arc::new(
        start_node(
            2,
            &root.path().join("node2"),
            &pepper,
            identity(2, &ca, &ca_key),
        )
        .await,
    );
    let node3 = Arc::new(
        start_node(
            3,
            &root.path().join("node3"),
            &pepper,
            identity(3, &ca, &ca_key),
        )
        .await,
    );
    let ports = [free_port(), free_port(), free_port()];
    let mut metadata = vec![node1.metadata(), node2.metadata(), node3.metadata()];
    for (index, node) in metadata.iter_mut().enumerate() {
        node.api_addr = format!("https://localhost:{}", ports[index]);
    }
    for node in [&node1, &node2, &node3] {
        node.register_peers(metadata.clone());
    }
    node1.bootstrap_single().await.unwrap();
    node1
        .bootstrap_platform_credential(PLATFORM_TOKEN)
        .await
        .unwrap();
    node1.add_learner(metadata[1].clone()).await.unwrap();
    node1.add_learner(metadata[2].clone()).await.unwrap();
    node1
        .change_membership(BTreeSet::from([1, 2, 3]), 0)
        .await
        .unwrap();
    for (index, node) in metadata.iter().cloned().enumerate() {
        node1
            .propose(ControlCommand::RegisterNode {
                request_id: format!("api-node-{}", index + 1),
                node,
            })
            .await
            .unwrap();
    }
    node1
        .propose(ControlCommand::CreateTenant {
            tenant_id: "default".into(),
            request_id: "create-default".into(),
            config: TenantControlConfig::default(),
        })
        .await
        .unwrap();
    let issued = node1
        .issue_credential("default", ControlRole::Write, None)
        .await
        .unwrap();
    API_TOKEN
        .set(issued.secret)
        .expect("test API token set once");
    node1
        .propose(ControlCommand::ReconcilePlacement {
            tenant_id: "default".into(),
            request_id: "placement-generation-1".into(),
            expected_generation: 0,
            placement: Placement {
                tenant_uuid: uuid::Uuid::from_u128(500),
                generation: 1,
                primary: Some(1),
                members: BTreeSet::from([1, 2, 3]),
                draining: BTreeSet::new(),
                durable_watermarks: BTreeMap::from([(1, 1), (2, 1), (3, 1)]),
                committed_watermark: 0,
                // This Phase 3 routing test has no tenant replication services.
                // Quorum durability is covered by tenant_replication_three_node.
                durability: DurabilityPolicy::Local,
            },
        })
        .await
        .unwrap();

    std::env::set_var("FLUCTLIGHT_SERVER_MODE", "development");
    std::env::set_var("FLUCTLIGHT_REQUIRE_AUTH", "false");
    std::env::remove_var("FLUCTLIGHT_DISTRIBUTED");
    reset_shutdown_for_tests();
    let servers = [
        BrainServer::open(root.path().join("brain1"))
            .unwrap()
            .attach_existing_control_node(Arc::clone(&node1))
            .await
            .unwrap(),
        BrainServer::open(root.path().join("brain2"))
            .unwrap()
            .attach_existing_control_node(Arc::clone(&node2))
            .await
            .unwrap(),
        BrainServer::open(root.path().join("brain3"))
            .unwrap()
            .attach_existing_control_node(Arc::clone(&node3))
            .await
            .unwrap(),
    ];
    let views = servers.clone();
    let tasks: Vec<_> = servers
        .into_iter()
        .zip(ports)
        .map(|(server, port)| {
            tokio::spawn(async move {
                server
                    .serve_async(&format!("127.0.0.1:{port}"))
                    .await
                    .unwrap();
            })
        })
        .collect();
    for port in ports {
        wait_for_api(port).await;
    }

    let (status, _) = post(
        ports[0],
        "/api/v1/experience",
        r#"{"content":"reflex: generation one write","context":"phase3"}"#,
    )
    .await;
    assert_eq!(status, 200);
    let before = views[0]
        .with_brain_read("default", |brain| Ok(brain.hippocampus.engrams.len()))
        .unwrap();

    node1.isolate().await.unwrap();
    let mut promoted = false;
    let mut last_promotion = String::new();
    for _ in 0..100 {
        let result = node2
            .propose(ControlCommand::PromotePlacement {
                tenant_id: "default".into(),
                request_id: "promote-node-2".into(),
                candidate: 2,
                expected_generation: 1,
                new_generation: 2,
            })
            .await;
        last_promotion = format!("{result:?}");
        if result.is_ok_and(|response| {
            matches!(
                response,
                fluctlightdb::control::types::ControlResponse::Applied { .. }
                    | fluctlightdb::control::types::ControlResponse::AlreadyApplied { .. }
            )
        }) {
            promoted = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(
        promoted,
        "caught-up node 2 must be promoted by CAS; last result: {last_promotion}; leaders: node2={:?} node3={:?}",
        node2.current_leader(),
        node3.current_leader()
    );

    let (ready_status, ready_body) = get(ports[0], "/ready").await;
    assert_eq!(ready_status, 503, "{ready_body}");
    assert!(ready_body.contains("\"ready\":false"));
    let (stale_status, stale_body) = post(
        ports[0],
        "/api/v1/experience",
        r#"{"content":"reflex: stale primary write","context":"phase3"}"#,
    )
    .await;
    assert_eq!(stale_status, 503, "{stale_body}");
    assert!(stale_body.contains("placement_unavailable"), "{stale_body}");
    let after = views[0]
        .with_brain_read("default", |brain| Ok(brain.hippocampus.engrams.len()))
        .unwrap();
    assert_eq!(
        after, before,
        "isolated old primary must not mutate locally"
    );

    let (primary_status, _) = post(
        ports[1],
        "/api/v1/experience",
        r#"{"content":"reflex: generation two write","context":"phase3"}"#,
    )
    .await;
    assert_eq!(primary_status, 200);
    let follower_before = views[2]
        .with_brain_read("default", |brain| Ok(brain.hippocampus.engrams.len()))
        .unwrap();
    let (redirect_status, redirect_body) = post(
        ports[2],
        "/api/v1/experience",
        r#"{"content":"reflex: follower must not write","context":"phase3"}"#,
    )
    .await;
    assert_eq!(redirect_status, 307, "{redirect_body}");
    assert!(redirect_body.contains("\"error\":\"not_primary\""));
    assert!(redirect_body.contains("\"placement_generation\":2"));
    let follower_after = views[2]
        .with_brain_read("default", |brain| Ok(brain.hippocampus.engrams.len()))
        .unwrap();
    assert_eq!(follower_after, follower_before);

    let (bounded_ok, _) = post(
        ports[2],
        "/api/v1/status",
        r#"{"read_consistency":"bounded_stale","minimum_watermark":1,"maximum_staleness_ms":5000}"#,
    )
    .await;
    assert_eq!(bounded_ok, 200);
    tokio::time::sleep(Duration::from_millis(20)).await;
    let (bounded_old, old_body) = post(
        ports[2],
        "/api/v1/status",
        r#"{"read_consistency":"bounded_stale","minimum_watermark":1,"maximum_staleness_ms":1}"#,
    )
    .await;
    assert_eq!(bounded_old, 503, "{old_body}");
    assert!(old_body.contains("read_consistency_unavailable"));
    let (activate_old, activate_body) = post(
        ports[2],
        "/api/v1/activate",
        r#"{"cue":"generation","read_consistency":"bounded_stale","minimum_watermark":1,"maximum_staleness_ms":1}"#,
    )
    .await;
    assert_eq!(activate_old, 503, "{activate_body}");
    assert!(activate_body.contains("read_consistency_unavailable"));
    let (bounded_lag, lag_body) = post(
        ports[2],
        "/api/v1/status",
        r#"{"read_consistency":"bounded_stale","minimum_watermark":11,"maximum_staleness_ms":5000}"#,
    )
    .await;
    assert_eq!(bounded_lag, 503, "{lag_body}");
    assert!(lag_body.contains("read_consistency_unavailable"));
    let (linearizable_follower, _) = post(
        ports[2],
        "/api/v1/status",
        r#"{"read_consistency":"primary"}"#,
    )
    .await;
    assert_eq!(linearizable_follower, 503);
    let (eventual, _) = post(
        ports[2],
        "/api/v1/status",
        r#"{"read_consistency":"eventual"}"#,
    )
    .await;
    assert_eq!(eventual, 200);

    request_shutdown();
    for task in tasks {
        task.await.unwrap();
    }
}
