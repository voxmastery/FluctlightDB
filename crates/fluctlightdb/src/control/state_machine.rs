use std::collections::BTreeMap;
use std::sync::Arc;

use hmac::{Hmac, Mac};
use rand::RngCore;
use sha2::Sha256;

use super::types::{
    ControlCommand, ControlResponse, ControlRole, ControlState, KeyMetadata, KeyVerifier,
    TenantLifecycle, TenantRecord,
};

const KEY_VERIFIER_VERSION: u16 = 1;
type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssuedKey {
    pub secret: String,
    pub metadata: KeyMetadata,
}

#[derive(Clone)]
pub struct KeyIssuer {
    pepper: [u8; 32],
}

impl KeyIssuer {
    pub fn new(pepper: &[u8]) -> Result<Self, String> {
        let pepper: [u8; 32] = pepper
            .try_into()
            .map_err(|_| "cluster pepper must be exactly 32 bytes".to_string())?;
        Ok(Self { pepper })
    }

    pub fn issue(
        &self,
        key_id: impl Into<String>,
        tenant_id: impl Into<String>,
        role: ControlRole,
        created_at_unix_ms: u64,
        expires_at_unix_ms: Option<u64>,
    ) -> Result<IssuedKey, String> {
        let mut raw = [0_u8; 32];
        rand::rngs::OsRng.fill_bytes(&mut raw);
        let secret = hex(&raw);
        let metadata = self.metadata_for_secret(
            key_id,
            tenant_id,
            role,
            created_at_unix_ms,
            expires_at_unix_ms,
            &secret,
        )?;
        Ok(IssuedKey { secret, metadata })
    }

    pub fn metadata_for_secret(
        &self,
        key_id: impl Into<String>,
        tenant_id: impl Into<String>,
        role: ControlRole,
        created_at_unix_ms: u64,
        expires_at_unix_ms: Option<u64>,
        secret: &str,
    ) -> Result<KeyMetadata, String> {
        Ok(KeyMetadata {
            key_id: key_id.into(),
            tenant_id: tenant_id.into(),
            role,
            verifier: verifier(&self.pepper, secret.as_bytes())?,
            created_at_unix_ms,
            expires_at_unix_ms,
            revoked_at_unix_ms: None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizedKey {
    pub key_id: String,
    pub tenant_id: String,
    pub role: ControlRole,
}

#[derive(Clone)]
pub struct ControlStateMachine {
    pepper: [u8; 32],
    state: ControlState,
    auth_index: Arc<BTreeMap<[u8; 32], KeyMetadata>>,
}

impl ControlStateMachine {
    pub fn new(pepper: &[u8]) -> Result<Self, String> {
        Self::from_state(pepper, ControlState::default())
    }

    pub fn from_state(pepper: &[u8], state: ControlState) -> Result<Self, String> {
        let pepper: [u8; 32] = pepper
            .try_into()
            .map_err(|_| "cluster pepper must be exactly 32 bytes".to_string())?;
        let auth_index = Arc::new(
            state
                .keys
                .values()
                .map(|key| (key.verifier.digest, key.clone()))
                .collect(),
        );
        Ok(Self {
            pepper,
            state,
            auth_index,
        })
    }

    pub fn state(&self) -> &ControlState {
        &self.state
    }

    pub fn auth_index(&self) -> Arc<BTreeMap<[u8; 32], KeyMetadata>> {
        Arc::clone(&self.auth_index)
    }

    pub fn authorize(&self, secret: &str, now_unix_ms: u64) -> Option<AuthorizedKey> {
        let digest = verifier(&self.pepper, secret.as_bytes()).ok()?.digest;
        let key = self.auth_index.get(&digest)?;
        if key.verifier.version != KEY_VERIFIER_VERSION
            || key.revoked_at_unix_ms.is_some()
            || key
                .expires_at_unix_ms
                .is_some_and(|expiry| expiry <= now_unix_ms)
            || !verify(&self.pepper, secret.as_bytes(), &key.verifier.digest)
        {
            return None;
        }
        Some(AuthorizedKey {
            key_id: key.key_id.clone(),
            tenant_id: key.tenant_id.clone(),
            role: key.role,
        })
    }

    pub fn apply(&mut self, command: ControlCommand) -> Result<ControlResponse, String> {
        let request_id = request_id(&command).to_string();
        if let Some(previous) = self.state.applied_requests.get(&request_id) {
            let revision = match previous {
                ControlResponse::Applied { revision }
                | ControlResponse::AlreadyApplied { revision } => *revision,
                ControlResponse::Rejected { .. } => self.state.revision,
            };
            return Ok(ControlResponse::AlreadyApplied { revision });
        }

        let result = self.apply_once(command);
        if matches!(result, ControlResponse::Applied { .. }) {
            self.auth_index = Arc::new(
                self.state
                    .keys
                    .values()
                    .map(|key| (key.verifier.digest, key.clone()))
                    .collect(),
            );
        }
        self.state
            .applied_requests
            .insert(request_id, result.clone());
        Ok(result)
    }

    fn apply_once(&mut self, command: ControlCommand) -> ControlResponse {
        let rejected = |reason: &str| ControlResponse::Rejected {
            reason: reason.to_string(),
        };
        match command {
            ControlCommand::CreateTenant {
                tenant_id, config, ..
            } => {
                if tenant_id.is_empty() || self.state.tenants.contains_key(&tenant_id) {
                    return rejected("tenant id is empty or already exists");
                }
                self.state.tenants.insert(
                    tenant_id,
                    TenantRecord {
                        config,
                        lifecycle: TenantLifecycle::Active,
                        placements: Default::default(),
                    },
                );
            }
            ControlCommand::ConfigureTenant {
                tenant_id, config, ..
            } => match self.state.tenants.get_mut(&tenant_id) {
                Some(tenant) => tenant.config = config,
                None => return rejected("tenant is not registered"),
            },
            ControlCommand::SetTenantLifecycle {
                tenant_id,
                lifecycle,
                ..
            } => match self.state.tenants.get_mut(&tenant_id) {
                Some(tenant) => tenant.lifecycle = lifecycle,
                None => return rejected("tenant is not registered"),
            },
            ControlCommand::IssueKey { metadata, .. } => {
                if metadata.verifier.version != KEY_VERIFIER_VERSION {
                    return rejected("unsupported key verifier version");
                }
                if self.state.keys.contains_key(&metadata.key_id) {
                    return rejected("key id already exists");
                }
                self.state.keys.insert(metadata.key_id.clone(), metadata);
            }
            ControlCommand::BootstrapPlatformKey { metadata, .. } => {
                if self.state.credential_bootstrap_completed {
                    return rejected("platform credential bootstrap is already completed");
                }
                if metadata.verifier.version != KEY_VERIFIER_VERSION
                    || metadata.role != ControlRole::Platform
                    || metadata.revoked_at_unix_ms.is_some()
                    || self.state.keys.contains_key(&metadata.key_id)
                {
                    return rejected("invalid platform bootstrap credential");
                }
                self.state.keys.insert(metadata.key_id.clone(), metadata);
                self.state.credential_bootstrap_completed = true;
            }
            ControlCommand::RevokeKey {
                key_id,
                revoked_at_unix_ms,
                ..
            } => match self.state.keys.get_mut(&key_id) {
                Some(key) => key.revoked_at_unix_ms = Some(revoked_at_unix_ms),
                None => return rejected("key id is not registered"),
            },
            ControlCommand::RegisterNode { node, .. } => {
                if node.node_id == 0 || node.raft_addr.is_empty() {
                    return rejected("node id and raft address are required");
                }
                self.state.nodes.insert(node.node_id, node);
            }
            ControlCommand::SetPlacement {
                tenant_id, nodes, ..
            } => {
                if !nodes.iter().all(|node| self.state.nodes.contains_key(node)) {
                    return rejected("placement contains an unregistered node");
                }
                match self.state.tenants.get_mut(&tenant_id) {
                    Some(tenant) => tenant.placements = nodes,
                    None => return rejected("tenant is not registered"),
                }
            }
            ControlCommand::ReconcilePlacement {
                tenant_id,
                expected_generation,
                placement,
                ..
            } => {
                if !self.state.tenants.contains_key(&tenant_id) {
                    return rejected("tenant is not registered");
                }
                if !placement
                    .members
                    .iter()
                    .chain(placement.draining.iter())
                    .all(|node| self.state.nodes.contains_key(node))
                {
                    return rejected("placement contains an unregistered node");
                }
                if placement
                    .primary
                    .is_some_and(|node| !placement.members.contains(&node))
                {
                    return rejected("placement primary must be an active member");
                }
                let current_generation = self
                    .state
                    .placements
                    .get(&tenant_id)
                    .map(|current| current.generation)
                    .unwrap_or_default();
                if current_generation != expected_generation {
                    return rejected("placement generation changed");
                }
                if placement.generation <= current_generation {
                    return rejected("placement generation must increase");
                }
                if self
                    .state
                    .placements
                    .get(&tenant_id)
                    .is_some_and(|current| current.tenant_uuid != placement.tenant_uuid)
                {
                    return rejected("tenant UUID cannot change");
                }
                if let Some(tenant) = self.state.tenants.get_mut(&tenant_id) {
                    tenant.placements = placement.members.clone();
                }
                self.state.placements.insert(tenant_id, placement);
            }
            ControlCommand::PromotePlacement {
                tenant_id,
                candidate,
                expected_generation,
                new_generation,
                ..
            } => {
                let Some(current) = self.state.placements.get(&tenant_id) else {
                    return rejected("tenant has no applied placement");
                };
                let promoted = match current.promote(candidate, expected_generation, new_generation)
                {
                    Ok(promoted) => promoted,
                    Err(error) => {
                        return ControlResponse::Rejected {
                            reason: error.to_string(),
                        }
                    }
                };
                self.state.placements.insert(tenant_id, promoted);
            }
            ControlCommand::ReportDurableWatermark {
                tenant_id,
                node_id,
                generation,
                durable_watermark,
                ..
            } => {
                let Some(placement) = self.state.placements.get_mut(&tenant_id) else {
                    return rejected("tenant has no applied placement");
                };
                if generation != placement.generation {
                    return rejected("stale durable watermark generation");
                }
                if !placement.members.contains(&node_id) || placement.draining.contains(&node_id) {
                    return rejected("durable watermark reporter is not an active member");
                }
                let previous = placement
                    .durable_watermarks
                    .get(&node_id)
                    .copied()
                    .unwrap_or_default();
                if durable_watermark < previous {
                    return rejected("durable watermark cannot move backwards");
                }
                placement
                    .durable_watermarks
                    .insert(node_id, durable_watermark);
                let mut watermarks: Vec<_> = placement
                    .members
                    .iter()
                    .filter(|member| !placement.draining.contains(member))
                    .map(|member| {
                        placement
                            .durable_watermarks
                            .get(member)
                            .copied()
                            .unwrap_or_default()
                    })
                    .collect();
                watermarks.sort_unstable_by(|left, right| right.cmp(left));
                let policy_watermark = match placement.durability {
                    crate::placement::DurabilityPolicy::Local => placement
                        .primary
                        .and_then(|primary| placement.durable_watermarks.get(&primary).copied())
                        .unwrap_or_default(),
                    crate::placement::DurabilityPolicy::Quorum => {
                        let majority = watermarks.len() / 2 + 1;
                        watermarks
                            .get(majority.saturating_sub(1))
                            .copied()
                            .unwrap_or_default()
                    }
                    crate::placement::DurabilityPolicy::All => {
                        watermarks.last().copied().unwrap_or_default()
                    }
                };
                placement.committed_watermark = placement.committed_watermark.max(policy_watermark);
            }
            ControlCommand::SetVoters {
                expected_membership_epoch,
                voters,
                ..
            } => {
                if expected_membership_epoch != self.state.membership_epoch {
                    return rejected("membership epoch changed");
                }
                if voters.is_empty()
                    || !voters
                        .iter()
                        .all(|node| self.state.nodes.contains_key(node))
                {
                    return rejected("voters must be non-empty registered nodes");
                }
                self.state.voters = voters;
                self.state.membership_epoch += 1;
            }
        }
        self.state.revision += 1;
        ControlResponse::Applied {
            revision: self.state.revision,
        }
    }
}

fn request_id(command: &ControlCommand) -> &str {
    match command {
        ControlCommand::CreateTenant { request_id, .. }
        | ControlCommand::ConfigureTenant { request_id, .. }
        | ControlCommand::SetTenantLifecycle { request_id, .. }
        | ControlCommand::IssueKey { request_id, .. }
        | ControlCommand::BootstrapPlatformKey { request_id, .. }
        | ControlCommand::RevokeKey { request_id, .. }
        | ControlCommand::RegisterNode { request_id, .. }
        | ControlCommand::SetPlacement { request_id, .. }
        | ControlCommand::ReconcilePlacement { request_id, .. }
        | ControlCommand::PromotePlacement { request_id, .. }
        | ControlCommand::ReportDurableWatermark { request_id, .. }
        | ControlCommand::SetVoters { request_id, .. } => request_id,
    }
}

fn verifier(pepper: &[u8; 32], secret: &[u8]) -> Result<KeyVerifier, String> {
    let mut mac = HmacSha256::new_from_slice(pepper).map_err(|error| error.to_string())?;
    mac.update(secret);
    Ok(KeyVerifier {
        version: KEY_VERIFIER_VERSION,
        digest: mac.finalize().into_bytes().into(),
    })
}

fn verify(pepper: &[u8; 32], secret: &[u8], expected: &[u8; 32]) -> bool {
    let Ok(mut mac) = HmacSha256::new_from_slice(pepper) else {
        return false;
    };
    mac.update(secret);
    mac.verify_slice(expected).is_ok()
}

fn hex(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write;
        let _ = write!(output, "{byte:02x}");
    }
    output
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;
    use crate::control::types::{
        ControlCommand, ControlResponse, ControlRole, NodeMetadata, TenantControlConfig,
    };

    fn pepper() -> [u8; 32] {
        [7; 32]
    }

    #[test]
    fn key_issuer_generates_256_bit_secret_and_state_machine_authorizes_digest_only() {
        let issuer = KeyIssuer::new(&pepper()).unwrap();
        let issued = issuer
            .issue("key-1", "tenant-a", ControlRole::Write, 10, Some(12))
            .unwrap();
        assert_eq!(issued.secret.len(), 64);
        assert!(!serde_json::to_string(&issued.metadata)
            .unwrap()
            .contains(&issued.secret));

        let mut machine = ControlStateMachine::new(&pepper()).unwrap();
        machine
            .apply(ControlCommand::IssueKey {
                request_id: "issue-1".into(),
                metadata: issued.metadata.clone(),
            })
            .unwrap();

        let auth = machine.authorize(&issued.secret, 11).unwrap();
        assert_eq!(auth.tenant_id, "tenant-a");
        assert_eq!(auth.role, ControlRole::Write);
        assert!(machine.authorize("wrong", 11).is_none());
        assert!(machine.authorize(&issued.secret, 12).is_none());
    }

    #[test]
    fn platform_bootstrap_is_committed_once_and_cannot_be_reused() {
        let issuer = KeyIssuer::new(&pepper()).unwrap();
        let first = issuer
            .metadata_for_secret(
                "bootstrap-platform",
                "platform",
                ControlRole::Platform,
                10,
                None,
                "first-secret",
            )
            .unwrap();
        let second = issuer
            .metadata_for_secret(
                "bootstrap-platform-2",
                "platform",
                ControlRole::Platform,
                11,
                None,
                "second-secret",
            )
            .unwrap();
        let mut machine = ControlStateMachine::new(&pepper()).unwrap();

        assert!(matches!(
            machine
                .apply(ControlCommand::BootstrapPlatformKey {
                    request_id: "bootstrap-1".into(),
                    metadata: first,
                })
                .unwrap(),
            ControlResponse::Applied { .. }
        ));
        assert!(machine.state().credential_bootstrap_completed);
        assert!(machine.authorize("first-secret", 12).is_some());

        let reused = machine
            .apply(ControlCommand::BootstrapPlatformKey {
                request_id: "bootstrap-2".into(),
                metadata: second,
            })
            .unwrap();
        assert!(matches!(reused, ControlResponse::Rejected { .. }));
        assert!(machine.authorize("second-secret", 12).is_none());
    }

    #[test]
    fn membership_change_is_compare_and_swap_and_requires_registered_nodes() {
        let mut machine = ControlStateMachine::new(&pepper()).unwrap();
        machine
            .apply(ControlCommand::RegisterNode {
                request_id: "node-1".into(),
                node: NodeMetadata {
                    node_id: 1,
                    raft_addr: "127.0.0.1:9101".into(),
                    api_addr: "127.0.0.1:9201".into(),
                    certificate_sha256: [1; 32],
                },
            })
            .unwrap();

        let rejected = machine
            .apply(ControlCommand::SetVoters {
                request_id: "members-1".into(),
                expected_membership_epoch: 0,
                voters: BTreeSet::from([1, 2]),
            })
            .unwrap();
        assert!(matches!(rejected, ControlResponse::Rejected { .. }));

        let applied = machine
            .apply(ControlCommand::SetVoters {
                request_id: "members-2".into(),
                expected_membership_epoch: 0,
                voters: BTreeSet::from([1]),
            })
            .unwrap();
        assert!(matches!(applied, ControlResponse::Applied { .. }));
        assert_eq!(machine.state().membership_epoch, 1);

        let stale = machine
            .apply(ControlCommand::SetVoters {
                request_id: "members-3".into(),
                expected_membership_epoch: 0,
                voters: BTreeSet::from([1]),
            })
            .unwrap();
        assert!(matches!(stale, ControlResponse::Rejected { .. }));
    }

    #[test]
    fn duplicate_request_returns_original_response_without_advancing_revision() {
        let mut machine = ControlStateMachine::new(&pepper()).unwrap();
        let command = ControlCommand::CreateTenant {
            tenant_id: "tenant-a".into(),
            request_id: "create-1".into(),
            config: TenantControlConfig::default(),
        };
        let first = machine.apply(command.clone()).unwrap();
        let revision = machine.state().revision;
        let second = machine.apply(command).unwrap();

        assert!(matches!(first, ControlResponse::Applied { .. }));
        assert_eq!(second, ControlResponse::AlreadyApplied { revision });
        assert_eq!(machine.state().revision, revision);
    }

    #[test]
    fn placement_promotion_is_applied_cas_and_rejects_lagging_candidate() {
        use crate::placement::{DurabilityPolicy, Placement};

        let mut machine = ControlStateMachine::new(&pepper()).unwrap();
        for node_id in 1..=3 {
            machine
                .apply(ControlCommand::RegisterNode {
                    request_id: format!("node-{node_id}"),
                    node: NodeMetadata {
                        node_id,
                        raft_addr: format!("127.0.0.1:91{node_id}"),
                        api_addr: format!("127.0.0.1:92{node_id}"),
                        certificate_sha256: [node_id as u8; 32],
                    },
                })
                .unwrap();
        }
        machine
            .apply(ControlCommand::CreateTenant {
                tenant_id: "tenant-p".into(),
                request_id: "tenant-p".into(),
                config: TenantControlConfig::default(),
            })
            .unwrap();
        let placement = Placement {
            tenant_uuid: uuid::Uuid::from_u128(9),
            generation: 1,
            primary: Some(1),
            members: BTreeSet::from([1, 2, 3]),
            draining: BTreeSet::new(),
            durable_watermarks: BTreeMap::from([(1, 20), (2, 20), (3, 19)]),
            committed_watermark: 20,
            durability: DurabilityPolicy::Quorum,
        };
        machine
            .apply(ControlCommand::ReconcilePlacement {
                tenant_id: "tenant-p".into(),
                request_id: "place-p".into(),
                expected_generation: 0,
                placement,
            })
            .unwrap();

        let lagging = machine
            .apply(ControlCommand::PromotePlacement {
                tenant_id: "tenant-p".into(),
                request_id: "promote-lagging".into(),
                candidate: 3,
                expected_generation: 1,
                new_generation: 2,
            })
            .unwrap();
        assert!(matches!(lagging, ControlResponse::Rejected { .. }));
        machine
            .apply(ControlCommand::PromotePlacement {
                tenant_id: "tenant-p".into(),
                request_id: "promote-ready".into(),
                candidate: 2,
                expected_generation: 1,
                new_generation: 2,
            })
            .unwrap();
        let applied = &machine.state().placements["tenant-p"];
        assert_eq!(applied.primary, Some(2));
        assert_eq!(applied.generation, 2);
    }

    #[test]
    fn durable_node_watermarks_advance_control_commit_only_at_policy_threshold() {
        use crate::placement::{DurabilityPolicy, Placement};

        let mut machine = ControlStateMachine::new(&pepper()).unwrap();
        for node_id in 1..=3 {
            machine
                .apply(ControlCommand::RegisterNode {
                    request_id: format!("watermark-node-{node_id}"),
                    node: NodeMetadata {
                        node_id,
                        raft_addr: format!("127.0.0.1:93{node_id}"),
                        api_addr: format!("127.0.0.1:94{node_id}"),
                        certificate_sha256: [node_id as u8; 32],
                    },
                })
                .unwrap();
        }
        machine
            .apply(ControlCommand::CreateTenant {
                tenant_id: "watermarked".into(),
                request_id: "watermarked-tenant".into(),
                config: TenantControlConfig::default(),
            })
            .unwrap();
        machine
            .apply(ControlCommand::ReconcilePlacement {
                tenant_id: "watermarked".into(),
                request_id: "watermarked-placement".into(),
                expected_generation: 0,
                placement: Placement {
                    tenant_uuid: uuid::Uuid::from_u128(88),
                    generation: 4,
                    primary: Some(1),
                    members: BTreeSet::from([1, 2, 3]),
                    draining: BTreeSet::new(),
                    durable_watermarks: BTreeMap::new(),
                    committed_watermark: 0,
                    durability: DurabilityPolicy::Quorum,
                },
            })
            .unwrap();

        for node_id in [1, 2] {
            machine
                .apply(ControlCommand::ReportDurableWatermark {
                    tenant_id: "watermarked".into(),
                    request_id: format!("watermark-{node_id}"),
                    node_id,
                    generation: 4,
                    durable_watermark: 17,
                })
                .unwrap();
        }
        let placement = &machine.state().placements["watermarked"];
        assert_eq!(placement.durable_watermarks[&1], 17);
        assert_eq!(placement.durable_watermarks[&2], 17);
        assert_eq!(placement.committed_watermark, 17);

        let stale = machine
            .apply(ControlCommand::ReportDurableWatermark {
                tenant_id: "watermarked".into(),
                request_id: "stale-watermark".into(),
                node_id: 3,
                generation: 3,
                durable_watermark: 99,
            })
            .unwrap();
        assert!(matches!(stale, ControlResponse::Rejected { .. }));
    }
}
