#![cfg(feature = "distributed")]

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::time::Duration;

use fluctlightdb::control::network::{
    certificate_fingerprint, MtlsRpcServer, PeerIdentityRegistry, TlsIdentity,
};
use fluctlightdb::control::types::{ControlState, NodeMetadata};
use fluctlightdb::manifest::save_v4_dir;
use fluctlightdb::placement::{DurabilityPolicy, Placement};
use fluctlightdb::replicate::{
    CheckpointFile, CheckpointTransfer, ReplicaStore, ReplicationService, TenantReplicationClient,
};
use fluctlightdb::wal::{self, WalIdentity};
use fluctlightdb::{BrainServer, Episode, Error, FluctlightBrain};
use rcgen::{
    BasicConstraints, Certificate, CertificateParams, ExtendedKeyUsagePurpose, IsCa, KeyPair,
    KeyUsagePurpose,
};
use sha2::{Digest, Sha256};

fn ca() -> (Certificate, KeyPair) {
    let mut params = CertificateParams::new(Vec::<String>::new()).unwrap();
    params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    params.key_usages = vec![
        KeyUsagePurpose::DigitalSignature,
        KeyUsagePurpose::KeyCertSign,
    ];
    let key = KeyPair::generate().unwrap();
    (params.self_signed(&key).unwrap(), key)
}

fn identity(node_id: u64, ca: &Certificate, ca_key: &KeyPair) -> TlsIdentity {
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

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mtls_three_node_snapshot_bootstrap_wal_catchup_and_zero_loss_ack() {
    let root = tempfile::tempdir().unwrap();
    let (ca, ca_key) = ca();
    let identities = [
        identity(1, &ca, &ca_key),
        identity(2, &ca, &ca_key),
        identity(3, &ca, &ca_key),
    ];
    let wal_identity = WalIdentity {
        tenant_uuid: uuid::Uuid::from_u128(700),
        writer_epoch: 5,
        fence_generation: 5,
        durability: DurabilityPolicy::Quorum,
    };
    let registry = PeerIdentityRegistry::new();
    registry.register(NodeMetadata {
        node_id: 1,
        certificate_sha256: certificate_fingerprint(&identities[0].certificate_chain_der[0]),
        ..NodeMetadata::default()
    });
    let follower2 = root.path().join("node2");
    let follower3 = root.path().join("node3");
    let server2 = MtlsRpcServer::start(
        "127.0.0.1:0",
        identities[1].clone(),
        registry.clone(),
        Arc::new(ReplicationService::new(&follower2, wal_identity)),
    )
    .await
    .unwrap();
    let server3 = MtlsRpcServer::start(
        "127.0.0.1:0",
        identities[2].clone(),
        registry,
        Arc::new(ReplicationService::new(&follower3, wal_identity)),
    )
    .await
    .unwrap();
    let targets = [
        NodeMetadata {
            node_id: 2,
            raft_addr: server2.local_addr().to_string(),
            certificate_sha256: certificate_fingerprint(&identities[1].certificate_chain_der[0]),
            ..NodeMetadata::default()
        },
        NodeMetadata {
            node_id: 3,
            raft_addr: server3.local_addr().to_string(),
            certificate_sha256: certificate_fingerprint(&identities[2].certificate_chain_der[0]),
            ..NodeMetadata::default()
        },
    ];
    let primary = root.path().join("node1");
    let mut brain = FluctlightBrain::new();
    brain.set_wal_identity(Some(wal_identity));
    save_v4_dir(&brain, &primary).unwrap();
    let mut checkpoint = CheckpointTransfer::from_active(&primary, wal_identity).unwrap();
    let padding = vec![0x5a; 17 * 1024 * 1024];
    checkpoint.files.push(CheckpointFile {
        name: "large-snapshot-padding.bin".into(),
        length: padding.len() as u64,
        sha256: Sha256::digest(&padding).into(),
        bytes: padding,
    });
    let client = TenantReplicationClient::new(identities[0].clone()).unwrap();
    for target in &targets {
        client
            .install_checkpoint(target, checkpoint.clone())
            .await
            .unwrap();
    }

    brain.attach_store_path(primary.clone());
    brain
        .experience(Episode::new("acked canonical mutation", "phase4", 0.9))
        .unwrap();
    let frames = wal::replication_frames(&primary, 0, 1, &wal_identity).unwrap();
    let mut durable_nodes = vec![1];
    for target in &targets {
        let ack = client.apply_wal(target, frames.clone()).await.unwrap();
        assert_eq!(ack.durable_watermark, 1);
        durable_nodes.push(target.node_id);
    }
    assert_eq!(durable_nodes, vec![1, 2, 3]);

    for follower in [&follower2, &follower3] {
        let replica = ReplicaStore::new(follower, wal_identity);
        assert_eq!(replica.durable_watermark().unwrap(), 1);
        let loaded = FluctlightBrain::open_readonly(follower).unwrap();
        assert!(loaded
            .activate("acked canonical mutation")
            .recalls
            .iter()
            .any(|item| item.episode.content == "acked canonical mutation"));
        assert!(
            loaded.checkpoint().is_err(),
            "replicas must remain read-only"
        );
    }

    drop(brain);
    let placement = Placement {
        tenant_uuid: wal_identity.tenant_uuid,
        generation: wal_identity.fence_generation,
        primary: Some(1),
        members: BTreeSet::from([1, 2, 3]),
        draining: BTreeSet::new(),
        durable_watermarks: BTreeMap::from([(1, 1), (2, 1), (3, 1)]),
        committed_watermark: 1,
        durability: DurabilityPolicy::Quorum,
    };
    let mut state = ControlState::default();
    state.placements.insert("default".into(), placement);
    state.nodes = targets
        .iter()
        .cloned()
        .chain([NodeMetadata {
            node_id: 1,
            ..NodeMetadata::default()
        }])
        .map(|node| (node.node_id, node))
        .collect();
    let server = BrainServer::open(primary.clone())
        .unwrap()
        .with_applied_control_state(1, state)
        .with_tenant_replication(
            client,
            targets
                .iter()
                .cloned()
                .map(|node| (node.node_id, node))
                .collect(),
            Duration::from_secs(2),
        );
    server
        .with_brain_write("default", |brain| {
            brain.experience(Episode::new("server quorum mutation", "phase4", 0.9))
        })
        .unwrap();
    assert_eq!(
        ReplicaStore::new(&follower2, wal_identity)
            .durable_watermark()
            .unwrap(),
        2
    );
    assert_eq!(
        ReplicaStore::new(&follower3, wal_identity)
            .durable_watermark()
            .unwrap(),
        2
    );

    server2.shutdown().await.unwrap();
    server3.shutdown().await.unwrap();
    let error = server
        .with_brain_write("default", |brain| {
            brain.experience(Episode::new("must not claim success", "phase4", 0.9))
        })
        .unwrap_err();
    assert!(
        matches!(error, Error::DurabilityUnavailable { .. }),
        "{error}"
    );
}
