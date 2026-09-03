#![cfg(feature = "distributed")]

use std::collections::BTreeSet;
use std::path::Path;
use std::time::Duration;

use fluctlightdb::control::network::{certificate_fingerprint, MtlsRpcClient, TlsIdentity};
use fluctlightdb::control::service::{ControlNode, ControlNodeConfig};
use fluctlightdb::control::state_machine::KeyIssuer;
use fluctlightdb::control::types::{ControlCommand, ControlRole, NodeId, TenantControlConfig};
use rcgen::{
    BasicConstraints, Certificate, CertificateParams, ExtendedKeyUsagePurpose, IsCa, KeyPair,
    KeyUsagePurpose,
};

fn test_ca() -> (Certificate, KeyPair) {
    let mut params = CertificateParams::new(Vec::<String>::new()).unwrap();
    params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    params.key_usages = vec![
        KeyUsagePurpose::DigitalSignature,
        KeyUsagePurpose::KeyCertSign,
        KeyUsagePurpose::CrlSign,
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
    params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
    let key = KeyPair::generate().unwrap();
    let cert = params.signed_by(&key, ca, ca_key).unwrap();
    TlsIdentity {
        node_id,
        certificate_chain_der: vec![cert.der().to_vec()],
        private_key_der: key.serialize_der(),
        ca_certificate_der: ca.der().to_vec(),
        server_name: "localhost".into(),
    }
}

async fn start_node(
    node_id: NodeId,
    bind_addr: &str,
    data_dir: &Path,
    pepper: &[u8; 32],
    identity: TlsIdentity,
) -> ControlNode {
    ControlNode::start(ControlNodeConfig {
        node_id,
        bind_addr: bind_addr.into(),
        data_dir: data_dir.to_path_buf(),
        cluster_pepper: pepper.to_vec(),
        tls_identity: identity,
        cluster_name: "phase-2-network-test".into(),
    })
    .await
    .unwrap()
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn platform_bootstrap_requires_mode_0600_removes_secret_and_cannot_be_reused() {
    use std::os::unix::fs::PermissionsExt;

    let root = tempfile::tempdir().unwrap();
    let pepper = [41; 32];
    let (ca, ca_key) = test_ca();
    let node = start_node(
        1,
        "127.0.0.1:0",
        &root.path().join("node"),
        &pepper,
        identity(1, &ca, &ca_key),
    )
    .await;
    node.bootstrap_single().await.unwrap();

    let insecure = root.path().join("insecure-bootstrap");
    std::fs::write(&insecure, b"insecure-secret\n").unwrap();
    std::fs::set_permissions(&insecure, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(node
        .bootstrap_platform_credential_from_file(&insecure)
        .await
        .unwrap_err()
        .contains("0600"));
    assert!(insecure.exists());

    let secret_file = root.path().join("bootstrap");
    std::fs::write(&secret_file, b"one-time-platform-secret\n").unwrap();
    std::fs::set_permissions(&secret_file, std::fs::Permissions::from_mode(0o600)).unwrap();
    node.bootstrap_platform_credential_from_file(&secret_file)
        .await
        .unwrap();
    assert!(!secret_file.exists());
    let authorized = node
        .authorize_linearizable("one-time-platform-secret", u64::MAX - 1)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(authorized.role, ControlRole::Platform);
    for name in ["control.sqlite", "control.sqlite-wal", "control.sqlite-shm"] {
        let path = root.path().join("node").join(name);
        if let Ok(sqlite_bytes) = std::fs::read(path) {
            assert!(
                !sqlite_bytes
                    .windows(b"one-time-platform-secret".len())
                    .any(|window| window == b"one-time-platform-secret"),
                "bootstrap plaintext leaked into SQLite or the Raft log"
            );
        }
    }

    let reused = root.path().join("reused-bootstrap");
    std::fs::write(&reused, b"different-secret\n").unwrap();
    std::fs::set_permissions(&reused, std::fs::Permissions::from_mode(0o600)).unwrap();
    assert!(node
        .bootstrap_platform_credential_from_file(&reused)
        .await
        .unwrap_err()
        .contains("already completed"));
    assert!(!reused.exists());
    assert!(node
        .authorize_linearizable("different-secret", u64::MAX - 1)
        .await
        .unwrap()
        .is_none());
    node.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 6)]
async fn networked_three_node_cluster_forwards_restarts_and_rejects_identity_mismatch() {
    let root = tempfile::tempdir().unwrap();
    let pepper = [42; 32];
    let (ca, ca_key) = test_ca();
    let identities = [
        identity(1, &ca, &ca_key),
        identity(2, &ca, &ca_key),
        identity(3, &ca, &ca_key),
    ];
    let node1 = start_node(
        1,
        "127.0.0.1:0",
        &root.path().join("node1"),
        &pepper,
        identities[0].clone(),
    )
    .await;
    let node2 = start_node(
        2,
        "127.0.0.1:0",
        &root.path().join("node2"),
        &pepper,
        identities[1].clone(),
    )
    .await;
    let mut node3 = start_node(
        3,
        "127.0.0.1:0",
        &root.path().join("node3"),
        &pepper,
        identities[2].clone(),
    )
    .await;
    let metadata = vec![node1.metadata(), node2.metadata(), node3.metadata()];
    for node in [&node1, &node2, &node3] {
        node.register_peers(metadata.clone());
    }

    node1.bootstrap_single().await.unwrap();
    node2.join_cluster(node1.metadata()).await.unwrap();
    node3.join_cluster(node1.metadata()).await.unwrap();
    node1
        .change_membership(BTreeSet::from([1, 2, 3]), 0)
        .await
        .unwrap();
    assert_eq!(node2.current_leader(), Some(1));
    assert!(node2
        .change_membership(BTreeSet::from([1, 2, 3]), 0)
        .await
        .unwrap_err()
        .contains("membership epoch"));

    node2
        .propose(ControlCommand::CreateTenant {
            tenant_id: "tenant-a".into(),
            request_id: "create-tenant-a".into(),
            config: TenantControlConfig::default(),
        })
        .await
        .unwrap();
    let issued = KeyIssuer::new(&pepper)
        .unwrap()
        .issue("key-a", "tenant-a", ControlRole::Admin, 10, None)
        .unwrap();
    node2
        .propose(ControlCommand::IssueKey {
            request_id: "issue-key-a".into(),
            metadata: issued.metadata,
        })
        .await
        .unwrap();
    for node in [&node1, &node2, &node3] {
        let authorized = node
            .authorize_linearizable(&issued.secret, 11)
            .await
            .unwrap()
            .expect("issued credential must authorize on every ready node");
        assert_eq!(authorized.tenant_id, "tenant-a");
        assert_eq!(authorized.role, ControlRole::Admin);
    }
    node2
        .propose(ControlCommand::RevokeKey {
            key_id: "key-a".into(),
            request_id: "revoke-key-a".into(),
            revoked_at_unix_ms: 20,
        })
        .await
        .unwrap();

    let state = node3.linearizable_read().await.unwrap();
    assert!(state.tenants.contains_key("tenant-a"));
    assert!(state.keys["key-a"].revoked_at_unix_ms.is_some());
    for node in [&node1, &node2, &node3] {
        assert!(
            node.wait_for_revision(state.revision, Duration::from_secs(2))
                .await,
            "Raft revoke did not apply to every ready node within 2s"
        );
        assert!(
            node.authorize_linearizable(&issued.secret, 21)
                .await
                .unwrap()
                .is_none(),
            "revoked credential still authorized"
        );
    }

    let node3_addr = node3.metadata().raft_addr;
    node3.shutdown().await.unwrap();
    node3 = start_node(
        3,
        &node3_addr,
        &root.path().join("node3"),
        &pepper,
        identities[2].clone(),
    )
    .await;
    assert_eq!(node3.local_state().unwrap().revision, state.revision);
    node3.register_peers(metadata.clone());
    let restarted = node3.linearizable_read().await.unwrap();
    assert_eq!(restarted.revision, state.revision);
    assert!(restarted.tenants.contains_key("tenant-a"));

    let attacker_identity = identity(2, &ca, &ca_key);
    assert_ne!(
        certificate_fingerprint(&attacker_identity.certificate_chain_der[0]),
        node2.metadata().certificate_sha256
    );
    let attacker = MtlsRpcClient::new(attacker_identity).unwrap();
    assert!(attacker
        .request(&node1.metadata(), b"forged-node-two".to_vec())
        .await
        .is_err());

    node3.shutdown().await.unwrap();
    node2.shutdown().await.unwrap();
    node1.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 6)]
#[ignore = "Phase 5 release gate: run explicitly; leader failover must pass before production"]
async fn leader_kill_surviving_quorum_elects_and_commits() {
    let root = tempfile::tempdir().unwrap();
    let pepper = [43; 32];
    let (ca, ca_key) = test_ca();
    let identities = [
        identity(1, &ca, &ca_key),
        identity(2, &ca, &ca_key),
        identity(3, &ca, &ca_key),
    ];
    let node1 = start_node(
        1,
        "127.0.0.1:0",
        &root.path().join("node1"),
        &pepper,
        identities[0].clone(),
    )
    .await;
    let node2 = start_node(
        2,
        "127.0.0.1:0",
        &root.path().join("node2"),
        &pepper,
        identities[1].clone(),
    )
    .await;
    let node3 = start_node(
        3,
        "127.0.0.1:0",
        &root.path().join("node3"),
        &pepper,
        identities[2].clone(),
    )
    .await;
    let metadata = vec![node1.metadata(), node2.metadata(), node3.metadata()];
    for node in [&node1, &node2, &node3] {
        node.register_peers(metadata.clone());
    }
    node1.bootstrap_single().await.unwrap();
    node2.join_cluster(node1.metadata()).await.unwrap();
    node3.join_cluster(node1.metadata()).await.unwrap();
    node1
        .change_membership(BTreeSet::from([1, 2, 3]), 0)
        .await
        .unwrap();
    node1
        .propose(ControlCommand::CreateTenant {
            tenant_id: "leader-kill".into(),
            request_id: "leader-kill-create".into(),
            config: TenantControlConfig::default(),
        })
        .await
        .unwrap();

    node1.shutdown().await.unwrap();
    let mut last_error = String::new();
    let mut committed = false;
    for _ in 0..200 {
        for node in [&node2, &node3] {
            match node
                .propose(ControlCommand::ConfigureTenant {
                    tenant_id: "leader-kill".into(),
                    request_id: "leader-kill-commit".into(),
                    config: TenantControlConfig {
                        max_requests_per_second: 77,
                        ..TenantControlConfig::default()
                    },
                })
                .await
            {
                Ok(_) => {
                    committed = true;
                    break;
                }
                Err(error) => last_error = error,
            }
        }
        if committed {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(
        committed,
        "no post-kill commit; node2 leader={:?}, node3 leader={:?}, error={last_error}",
        node2.current_leader(),
        node3.current_leader()
    );
    node3.shutdown().await.unwrap();
    node2.shutdown().await.unwrap();
}
