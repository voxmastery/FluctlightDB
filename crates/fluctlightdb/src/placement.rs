//! Applied-control-state tenant placement, fencing, promotion and read policy.

use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

pub type NodeId = u64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum DurabilityPolicy {
    Local,
    #[default]
    Quorum,
    All,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlacementState {
    Absent,
    Staging,
    Follower,
    Primary,
    Draining,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Placement {
    pub tenant_uuid: uuid::Uuid,
    pub generation: u64,
    pub primary: Option<NodeId>,
    pub members: BTreeSet<NodeId>,
    pub draining: BTreeSet<NodeId>,
    pub durable_watermarks: BTreeMap<NodeId, u64>,
    pub committed_watermark: u64,
    pub durability: DurabilityPolicy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WriteFence {
    pub tenant_uuid: uuid::Uuid,
    pub node_id: NodeId,
    pub generation: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PlacementError {
    #[error("tenant identity does not match applied placement")]
    TenantMismatch,
    #[error("node is not primary; current primary is {primary:?} at generation {generation}")]
    NotPrimary {
        primary: Option<NodeId>,
        generation: u64,
    },
    #[error("stale placement generation {presented}; expected {expected}")]
    StaleGeneration { expected: u64, presented: u64 },
    #[error("placement generation changed; current generation is {current}")]
    GenerationChanged { current: u64 },
    #[error("new placement generation {proposed} must exceed {current}")]
    GenerationNotHigher { current: u64, proposed: u64 },
    #[error(
        "candidate durable watermark {candidate_watermark} is below required {required_watermark}"
    )]
    InsufficientDurability {
        candidate_watermark: u64,
        required_watermark: u64,
    },
    #[error("candidate node is not a placement member")]
    CandidateAbsent,
    #[error("primary did not durably store the exact canonical mutation")]
    PrimaryNotDurable,
    #[error("durable acknowledgements for the exact canonical mutation did not reach {required}")]
    InsufficientAcks { required: &'static str },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecoveryPointObjective {
    Zero,
    NonZero,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplicaDurableAck {
    pub node_id: NodeId,
    pub watermark: u64,
    pub mutation_sha256: [u8; 32],
}

impl ReplicaDurableAck {
    pub fn exact(node_id: NodeId, watermark: u64, mutation_sha256: [u8; 32]) -> Self {
        Self {
            node_id,
            watermark,
            mutation_sha256,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DurableWriteOutcome {
    pub committed_watermark: u64,
    pub durable_copies: usize,
    pub rpo: RecoveryPointObjective,
}

pub fn evaluate_durable_write(
    policy: DurabilityPolicy,
    assigned: &BTreeSet<NodeId>,
    primary: NodeId,
    watermark: u64,
    mutation_sha256: [u8; 32],
    acknowledgements: &[ReplicaDurableAck],
) -> Result<DurableWriteOutcome, PlacementError> {
    let exact_nodes: BTreeSet<_> = acknowledgements
        .iter()
        .filter(|ack| {
            assigned.contains(&ack.node_id)
                && ack.watermark == watermark
                && ack.mutation_sha256 == mutation_sha256
        })
        .map(|ack| ack.node_id)
        .collect();
    if !exact_nodes.contains(&primary) {
        return Err(PlacementError::PrimaryNotDurable);
    }
    let required = match policy {
        DurabilityPolicy::Local => 1,
        DurabilityPolicy::Quorum => assigned.len() / 2 + 1,
        DurabilityPolicy::All => assigned.len(),
    };
    if exact_nodes.len() < required {
        return Err(PlacementError::InsufficientAcks {
            required: match policy {
                DurabilityPolicy::Local => "primary",
                DurabilityPolicy::Quorum => "assigned-node majority",
                DurabilityPolicy::All => "all assigned nodes",
            },
        });
    }
    Ok(DurableWriteOutcome {
        committed_watermark: watermark,
        durable_copies: exact_nodes.len(),
        rpo: if policy == DurabilityPolicy::Local {
            RecoveryPointObjective::NonZero
        } else {
            RecoveryPointObjective::Zero
        },
    })
}

impl Placement {
    pub fn authorize_write(&self, fence: &WriteFence) -> Result<(), PlacementError> {
        if fence.tenant_uuid != self.tenant_uuid {
            return Err(PlacementError::TenantMismatch);
        }
        if self.primary != Some(fence.node_id) {
            return Err(PlacementError::NotPrimary {
                primary: self.primary,
                generation: self.generation,
            });
        }
        if fence.generation != self.generation {
            return Err(PlacementError::StaleGeneration {
                expected: self.generation,
                presented: fence.generation,
            });
        }
        Ok(())
    }

    pub fn promote(
        &self,
        candidate: NodeId,
        expected_generation: u64,
        new_generation: u64,
    ) -> Result<Self, PlacementError> {
        if expected_generation != self.generation {
            return Err(PlacementError::GenerationChanged {
                current: self.generation,
            });
        }
        if new_generation <= self.generation {
            return Err(PlacementError::GenerationNotHigher {
                current: self.generation,
                proposed: new_generation,
            });
        }
        if !self.members.contains(&candidate) || self.draining.contains(&candidate) {
            return Err(PlacementError::CandidateAbsent);
        }
        let candidate_watermark = self
            .durable_watermarks
            .get(&candidate)
            .copied()
            .unwrap_or_default();
        let required_watermark = self.committed_watermark;
        if candidate_watermark < required_watermark {
            return Err(PlacementError::InsufficientDurability {
                candidate_watermark,
                required_watermark,
            });
        }
        let mut promoted = self.clone();
        promoted.primary = Some(candidate);
        promoted.generation = new_generation;
        Ok(promoted)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalPlacement {
    pub state: PlacementState,
    pub generation: Option<u64>,
    pub tenant_uuid: Option<uuid::Uuid>,
    pub primary: Option<NodeId>,
    pub durability: Option<DurabilityPolicy>,
}

#[derive(Debug, Clone, Copy)]
pub struct PlacementReconciler {
    node_id: NodeId,
}

impl PlacementReconciler {
    pub fn new(node_id: NodeId) -> Self {
        Self { node_id }
    }

    pub fn reconcile(&self, applied: Option<&Placement>) -> LocalPlacement {
        let Some(applied) = applied else {
            return LocalPlacement {
                state: PlacementState::Absent,
                generation: None,
                tenant_uuid: None,
                primary: None,
                durability: None,
            };
        };
        let state = if applied.draining.contains(&self.node_id) {
            PlacementState::Draining
        } else if !applied.members.contains(&self.node_id) {
            PlacementState::Absent
        } else if applied.primary == Some(self.node_id) {
            PlacementState::Primary
        } else if applied
            .durable_watermarks
            .get(&self.node_id)
            .copied()
            .unwrap_or_default()
            < applied.committed_watermark
        {
            PlacementState::Staging
        } else {
            PlacementState::Follower
        };
        LocalPlacement {
            state,
            generation: Some(applied.generation),
            tenant_uuid: Some(applied.tenant_uuid),
            primary: applied.primary,
            durability: Some(applied.durability),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct FollowerWatermark {
    pub durable: u64,
    pub observed_at: SystemTime,
}

#[derive(Debug, Clone, Copy)]
pub enum ReadConsistency {
    Primary,
    BoundedStale {
        minimum_watermark: u64,
        maximum_age: Duration,
    },
    Eventual,
}

impl ReadConsistency {
    pub fn allows(
        self,
        local_is_primary: bool,
        follower: Option<FollowerWatermark>,
        now: SystemTime,
    ) -> bool {
        match self {
            Self::Primary => local_is_primary,
            Self::Eventual => true,
            Self::BoundedStale {
                minimum_watermark,
                maximum_age,
            } => {
                local_is_primary
                    || follower.is_some_and(|watermark| {
                        watermark.durable >= minimum_watermark
                            && now
                                .duration_since(watermark.observed_at)
                                .is_ok_and(|age| age <= maximum_age)
                    })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};
    use std::time::{Duration, SystemTime};

    use super::*;

    fn placement(primary: u64, generation: u64) -> Placement {
        Placement {
            tenant_uuid: uuid::Uuid::from_u128(7),
            generation,
            primary: Some(primary),
            members: BTreeSet::from([1, 2, 3]),
            draining: BTreeSet::new(),
            durable_watermarks: BTreeMap::from([(1, 10), (2, 10), (3, 9)]),
            committed_watermark: 10,
            durability: DurabilityPolicy::Quorum,
        }
    }

    #[test]
    fn applied_placement_reconciles_all_local_states() {
        let absent = PlacementReconciler::new(4).reconcile(None);
        assert_eq!(absent.state, PlacementState::Absent);

        let mut desired = placement(1, 4);
        assert_eq!(
            PlacementReconciler::new(1).reconcile(Some(&desired)).state,
            PlacementState::Primary
        );
        assert_eq!(
            PlacementReconciler::new(2).reconcile(Some(&desired)).state,
            PlacementState::Follower
        );
        desired.durable_watermarks.insert(2, 9);
        assert_eq!(
            PlacementReconciler::new(2).reconcile(Some(&desired)).state,
            PlacementState::Staging
        );
        desired.draining.insert(3);
        assert_eq!(
            PlacementReconciler::new(3).reconcile(Some(&desired)).state,
            PlacementState::Draining
        );
    }

    #[test]
    fn write_fence_rejects_stale_primary_and_generation() {
        let current = placement(2, 8);
        let stale_primary = WriteFence {
            tenant_uuid: current.tenant_uuid,
            node_id: 1,
            generation: 7,
        };
        assert_eq!(
            current.authorize_write(&stale_primary),
            Err(PlacementError::NotPrimary {
                primary: Some(2),
                generation: 8,
            })
        );

        let stale_generation = WriteFence {
            tenant_uuid: current.tenant_uuid,
            node_id: 2,
            generation: 7,
        };
        assert_eq!(
            current.authorize_write(&stale_generation),
            Err(PlacementError::StaleGeneration {
                expected: 8,
                presented: 7,
            })
        );
    }

    #[test]
    fn promotion_is_cas_and_requires_policy_watermark() {
        let current = placement(1, 5);
        assert!(matches!(
            current.promote(2, 4, 6),
            Err(PlacementError::GenerationChanged { current: 5 })
        ));
        assert!(matches!(
            current.promote(3, 5, 6),
            Err(PlacementError::InsufficientDurability { .. })
        ));
        let promoted = current.promote(2, 5, 6).unwrap();
        assert_eq!(promoted.primary, Some(2));
        assert_eq!(promoted.generation, 6);
    }

    #[test]
    fn follower_reads_enforce_watermark_and_age() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(100);
        let follower = FollowerWatermark {
            durable: 10,
            observed_at: now - Duration::from_secs(3),
        };
        assert!(ReadConsistency::BoundedStale {
            minimum_watermark: 9,
            maximum_age: Duration::from_secs(5),
        }
        .allows(false, Some(follower), now));
        assert!(!ReadConsistency::BoundedStale {
            minimum_watermark: 11,
            maximum_age: Duration::from_secs(5),
        }
        .allows(false, Some(follower), now));
        assert!(!ReadConsistency::Primary.allows(false, Some(follower), now));
        assert!(ReadConsistency::Eventual.allows(false, None, now));
    }

    #[test]
    fn quorum_ack_requires_primary_and_assigned_majority_for_exact_mutation() {
        let assigned = BTreeSet::from([1, 2, 3]);
        let mutation = [7; 32];
        let successful = evaluate_durable_write(
            DurabilityPolicy::Quorum,
            &assigned,
            1,
            22,
            mutation,
            &[
                ReplicaDurableAck::exact(1, 22, mutation),
                ReplicaDurableAck::exact(2, 22, mutation),
            ],
        )
        .unwrap();
        assert_eq!(successful.rpo, RecoveryPointObjective::Zero);

        let outage = evaluate_durable_write(
            DurabilityPolicy::Quorum,
            &assigned,
            1,
            23,
            mutation,
            &[ReplicaDurableAck::exact(1, 23, mutation)],
        )
        .unwrap_err();
        assert!(outage.to_string().contains("majority"), "{outage}");

        let wrong_mutation = evaluate_durable_write(
            DurabilityPolicy::Quorum,
            &assigned,
            1,
            24,
            mutation,
            &[
                ReplicaDurableAck::exact(1, 24, mutation),
                ReplicaDurableAck::exact(2, 24, [8; 32]),
            ],
        )
        .unwrap_err();
        assert!(
            wrong_mutation.to_string().contains("majority"),
            "{wrong_mutation}"
        );
    }

    #[test]
    fn local_durability_explicitly_reports_nonzero_rpo() {
        let assigned = BTreeSet::from([1, 2, 3]);
        let mutation = [9; 32];
        let outcome = evaluate_durable_write(
            DurabilityPolicy::Local,
            &assigned,
            1,
            31,
            mutation,
            &[ReplicaDurableAck::exact(1, 31, mutation)],
        )
        .unwrap();
        assert_eq!(outcome.rpo, RecoveryPointObjective::NonZero);
    }
}
