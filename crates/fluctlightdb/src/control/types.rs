use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::placement::Placement;

pub type NodeId = u64;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TenantControlConfig {
    pub max_brain_bytes: u64,
    pub max_requests_per_second: u32,
    pub max_concurrent_requests: u32,
}

impl Default for TenantControlConfig {
    fn default() -> Self {
        Self {
            max_brain_bytes: 1 << 30,
            max_requests_per_second: 100,
            max_concurrent_requests: 32,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TenantLifecycle {
    Active,
    Suspended,
    Deleted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ControlRole {
    Read,
    Write,
    Admin,
    Platform,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyVerifier {
    pub version: u16,
    pub digest: [u8; 32],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyMetadata {
    pub key_id: String,
    pub tenant_id: String,
    pub role: ControlRole,
    pub verifier: KeyVerifier,
    pub created_at_unix_ms: u64,
    pub expires_at_unix_ms: Option<u64>,
    pub revoked_at_unix_ms: Option<u64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeMetadata {
    pub node_id: NodeId,
    pub raft_addr: String,
    pub api_addr: String,
    pub certificate_sha256: [u8; 32],
}

impl std::fmt::Display for NodeMetadata {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}@{}", self.node_id, self.raft_addr)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TenantRecord {
    pub config: TenantControlConfig,
    pub lifecycle: TenantLifecycle,
    pub placements: BTreeSet<NodeId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ControlCommand {
    CreateTenant {
        tenant_id: String,
        request_id: String,
        config: TenantControlConfig,
    },
    ConfigureTenant {
        tenant_id: String,
        request_id: String,
        config: TenantControlConfig,
    },
    SetTenantLifecycle {
        tenant_id: String,
        request_id: String,
        lifecycle: TenantLifecycle,
    },
    IssueKey {
        request_id: String,
        metadata: KeyMetadata,
    },
    BootstrapPlatformKey {
        request_id: String,
        metadata: KeyMetadata,
    },
    RevokeKey {
        key_id: String,
        request_id: String,
        revoked_at_unix_ms: u64,
    },
    RegisterNode {
        request_id: String,
        node: NodeMetadata,
    },
    SetPlacement {
        tenant_id: String,
        request_id: String,
        nodes: BTreeSet<NodeId>,
    },
    ReconcilePlacement {
        tenant_id: String,
        request_id: String,
        expected_generation: u64,
        placement: Placement,
    },
    PromotePlacement {
        tenant_id: String,
        request_id: String,
        candidate: NodeId,
        expected_generation: u64,
        new_generation: u64,
    },
    ReportDurableWatermark {
        tenant_id: String,
        request_id: String,
        node_id: NodeId,
        generation: u64,
        durable_watermark: u64,
    },
    SetVoters {
        request_id: String,
        expected_membership_epoch: u64,
        voters: BTreeSet<NodeId>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ControlResponse {
    Applied { revision: u64 },
    AlreadyApplied { revision: u64 },
    Rejected { reason: String },
}

impl Default for ControlResponse {
    fn default() -> Self {
        Self::Applied { revision: 0 }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ControlState {
    pub revision: u64,
    pub membership_epoch: u64,
    pub tenants: BTreeMap<String, TenantRecord>,
    pub keys: BTreeMap<String, KeyMetadata>,
    pub nodes: BTreeMap<NodeId, NodeMetadata>,
    pub voters: BTreeSet<NodeId>,
    #[serde(default)]
    pub placements: BTreeMap<String, Placement>,
    #[serde(default)]
    pub credential_bootstrap_completed: bool,
    pub applied_requests: BTreeMap<String, ControlResponse>,
}

openraft::declare_raft_types!(
    pub ControlTypeConfig:
        D = ControlCommand,
        R = ControlResponse,
        NodeId = NodeId,
        Node = NodeMetadata,
        Entry = openraft::Entry<Self>,
        SnapshotData = std::io::Cursor<Vec<u8>>,
        AsyncRuntime = openraft::TokioRuntime,
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_commands_serialize_deterministically_without_brain_or_plaintext_key() {
        let command = ControlCommand::CreateTenant {
            tenant_id: "tenant-a".into(),
            request_id: "request-1".into(),
            config: TenantControlConfig::default(),
        };

        let first = serde_json::to_vec(&command).unwrap();
        let second = serde_json::to_vec(&command).unwrap();
        let encoded = String::from_utf8(first.clone()).unwrap();

        assert_eq!(first, second);
        assert!(!encoded.contains("brain_payload"));
        assert!(!encoded.contains("engram"));
        assert!(!encoded.contains("plaintext"));
        assert!(!encoded.contains("secret"));
    }
}
