//! Durable coordination state for parallel-agent swarms.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const SWARM_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SwarmState {
    pub schema_version: u32,
    pub runs: HashMap<Uuid, SwarmRun>,
    pub feedback: HashMap<Uuid, EngramFeedback>,
    pub truth_revisions: HashMap<String, Vec<TruthRevision>>,
    pub applied_transactions: HashSet<Uuid>,
}

impl Default for SwarmState {
    fn default() -> Self {
        Self {
            schema_version: SWARM_SCHEMA_VERSION,
            runs: HashMap::new(),
            feedback: HashMap::new(),
            truth_revisions: HashMap::new(),
            applied_transactions: HashSet::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SwarmRun {
    pub id: Uuid,
    pub project_id: String,
    pub objective_digest: String,
    pub repository_identity: String,
    pub base_commit: String,
    pub policy_version: String,
    pub roster: Vec<WorkerSlot>,
    pub allocations: HashMap<String, MemoryBundle>,
    pub attempts: HashMap<String, Attempt>,
    pub status: SwarmStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SwarmStatus {
    Active,
    Finished,
    Aborted,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkerSlot {
    pub slot_id: String,
    pub role: String,
    pub agent_id: Option<String>,
    pub worktree: Option<String>,
    pub status: WorkerStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkerStatus {
    Declared,
    Claimed,
    Reported,
    Verified,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct MemoryBundle {
    pub verified_truth: Vec<MemoryExposure>,
    pub mandatory_warnings: Vec<MemoryExposure>,
    pub episodic_memories: Vec<MemoryExposure>,
    pub strict_id_disjoint: bool,
    pub diversity_degraded: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MemoryExposure {
    pub engram_id: Uuid,
    pub content: String,
    pub score: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Attempt {
    pub cited_memory_ids: Vec<Uuid>,
    pub result_tree: Option<String>,
    pub summary: Option<String>,
    pub evidence: Option<EvidenceReceipt>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EvidenceReceipt {
    pub result: EvidenceResult,
    pub source_uri: String,
    pub command_digest: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceResult {
    Success,
    Failure,
    Inconclusive,
    ReproducedFailure,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct EngramFeedback {
    pub successes: u64,
    pub failures: u64,
    pub inconclusive: u64,
    pub reproduced_failures: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TruthRevision {
    pub revision: u64,
    pub engram_id: Uuid,
    pub evidence: EvidenceReceipt,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BeginSwarm {
    pub transaction_id: Uuid,
    pub swarm_id: Uuid,
    pub project_id: String,
    pub objective_digest: String,
    pub repository_identity: String,
    pub base_commit: String,
    pub policy_version: String,
    pub roster: Vec<WorkerSlot>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SwarmError {
    #[error("worker roster must not be empty")]
    EmptyRoster,
    #[error("duplicate worker slot: {0}")]
    DuplicateSlot(String),
    #[error("swarm already exists: {0}")]
    SwarmExists(Uuid),
    #[error("transaction was already applied to a different swarm: {0}")]
    TransactionConflict(Uuid),
}

impl SwarmState {
    pub fn begin_run(&mut self, request: BeginSwarm) -> Result<SwarmRun, SwarmError> {
        if self.applied_transactions.contains(&request.transaction_id) {
            return self
                .runs
                .get(&request.swarm_id)
                .cloned()
                .ok_or(SwarmError::TransactionConflict(request.transaction_id));
        }
        if request.roster.is_empty() {
            return Err(SwarmError::EmptyRoster);
        }
        let mut slot_ids = HashSet::with_capacity(request.roster.len());
        for slot in &request.roster {
            if !slot_ids.insert(slot.slot_id.clone()) {
                return Err(SwarmError::DuplicateSlot(slot.slot_id.clone()));
            }
        }
        if self.runs.contains_key(&request.swarm_id) {
            return Err(SwarmError::SwarmExists(request.swarm_id));
        }

        let run = SwarmRun {
            id: request.swarm_id,
            project_id: request.project_id,
            objective_digest: request.objective_digest,
            repository_identity: request.repository_identity,
            base_commit: request.base_commit,
            policy_version: request.policy_version,
            roster: request.roster,
            allocations: HashMap::new(),
            attempts: HashMap::new(),
            status: SwarmStatus::Active,
        };
        self.runs.insert(run.id, run.clone());
        self.applied_transactions.insert(request.transaction_id);
        Ok(run)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn worker(slot_id: &str) -> WorkerSlot {
        WorkerSlot {
            slot_id: slot_id.to_string(),
            role: "worker".to_string(),
            agent_id: None,
            worktree: None,
            status: WorkerStatus::Declared,
        }
    }

    #[test]
    fn begin_run_requires_a_complete_unique_roster() {
        let mut state = SwarmState::default();
        let request = BeginSwarm {
            transaction_id: uuid::Uuid::new_v4(),
            swarm_id: uuid::Uuid::new_v4(),
            project_id: "fluctlight".to_string(),
            objective_digest: "sha256:objective".to_string(),
            repository_identity: "github.com/voxmastery/FluctlightDB".to_string(),
            base_commit: "abc123".to_string(),
            policy_version: "v1".to_string(),
            roster: vec![worker("slot-a"), worker("slot-b")],
        };

        let run = state.begin_run(request).expect("valid roster");

        assert_eq!(run.roster.len(), 2);
        assert_eq!(run.status, SwarmStatus::Active);
        assert_eq!(state.runs.len(), 1);
    }

    #[test]
    fn duplicate_slot_ids_are_rejected_without_mutating_state() {
        let mut state = SwarmState::default();
        let result = state.begin_run(BeginSwarm {
            transaction_id: uuid::Uuid::new_v4(),
            swarm_id: uuid::Uuid::new_v4(),
            project_id: "fluctlight".to_string(),
            objective_digest: "sha256:objective".to_string(),
            repository_identity: "repo".to_string(),
            base_commit: "abc123".to_string(),
            policy_version: "v1".to_string(),
            roster: vec![worker("slot-a"), worker("slot-a")],
        });

        assert_eq!(
            result.unwrap_err(),
            SwarmError::DuplicateSlot("slot-a".into())
        );
        assert!(state.runs.is_empty());
        assert!(state.applied_transactions.is_empty());
    }

    #[test]
    fn transaction_ids_make_begin_idempotent() {
        let mut state = SwarmState::default();
        let request = BeginSwarm {
            transaction_id: uuid::Uuid::new_v4(),
            swarm_id: uuid::Uuid::new_v4(),
            project_id: "fluctlight".to_string(),
            objective_digest: "sha256:objective".to_string(),
            repository_identity: "repo".to_string(),
            base_commit: "abc123".to_string(),
            policy_version: "v1".to_string(),
            roster: vec![worker("slot-a")],
        };

        state.begin_run(request.clone()).unwrap();
        let replayed = state.begin_run(request).unwrap();

        assert_eq!(replayed.roster.len(), 1);
        assert_eq!(state.runs.len(), 1);
        assert_eq!(state.applied_transactions.len(), 1);
    }

    #[test]
    fn state_roundtrips_with_stable_schema_version() {
        let state = SwarmState::default();
        let json = serde_json::to_string(&state).unwrap();
        let decoded: SwarmState = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded.schema_version, SWARM_SCHEMA_VERSION);
        assert_eq!(decoded, state);
    }
}
