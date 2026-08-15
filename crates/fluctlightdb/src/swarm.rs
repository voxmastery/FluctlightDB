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
    #[serde(default)]
    pub strategy_tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MemoryCandidate {
    pub engram_id: Uuid,
    pub content: String,
    pub score: f32,
    #[serde(default)]
    pub strategy_tags: Vec<String>,
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

pub fn allocate_roster(
    roster: &[WorkerSlot],
    truth: &[MemoryCandidate],
    warnings: &[MemoryCandidate],
    episodes: &[MemoryCandidate],
    per_worker: usize,
) -> Result<HashMap<String, MemoryBundle>, SwarmError> {
    if roster.is_empty() {
        return Err(SwarmError::EmptyRoster);
    }
    let mut slot_ids = HashSet::with_capacity(roster.len());
    for slot in roster {
        if !slot_ids.insert(slot.slot_id.clone()) {
            return Err(SwarmError::DuplicateSlot(slot.slot_id.clone()));
        }
    }

    let verified_truth = sorted_exposures(truth);
    let mandatory_warnings = sorted_exposures(warnings);
    let diversity_degraded = episodes.len() < roster.len().saturating_mul(per_worker);
    let mut ordered_slots: Vec<_> = roster.iter().map(|slot| slot.slot_id.clone()).collect();
    ordered_slots.sort();
    let mut bundles: HashMap<String, MemoryBundle> = ordered_slots
        .iter()
        .map(|slot_id| {
            (
                slot_id.clone(),
                MemoryBundle {
                    verified_truth: verified_truth.clone(),
                    mandatory_warnings: mandatory_warnings.clone(),
                    episodic_memories: Vec::new(),
                    strict_id_disjoint: true,
                    diversity_degraded,
                },
            )
        })
        .collect();

    let mut remaining = episodes.to_vec();
    for _ in 0..per_worker {
        for slot_id in &ordered_slots {
            if remaining.is_empty() {
                break;
            }
            let assigned: Vec<&MemoryExposure> = bundles
                .values()
                .flat_map(|bundle| bundle.episodic_memories.iter())
                .collect();
            let selected = best_candidate_index(&remaining, &assigned);
            let candidate = remaining.remove(selected);
            bundles
                .get_mut(slot_id)
                .expect("bundle exists for every validated slot")
                .episodic_memories
                .push(candidate.into());
        }
    }

    Ok(bundles)
}

impl From<MemoryCandidate> for MemoryExposure {
    fn from(candidate: MemoryCandidate) -> Self {
        Self {
            engram_id: candidate.engram_id,
            content: candidate.content,
            score: candidate.score,
            strategy_tags: candidate.strategy_tags,
        }
    }
}

fn sorted_exposures(candidates: &[MemoryCandidate]) -> Vec<MemoryExposure> {
    let mut candidates = candidates.to_vec();
    candidates.sort_by(candidate_order);
    candidates.into_iter().map(Into::into).collect()
}

fn candidate_order(left: &MemoryCandidate, right: &MemoryCandidate) -> std::cmp::Ordering {
    right
        .score
        .total_cmp(&left.score)
        .then_with(|| left.engram_id.cmp(&right.engram_id))
}

fn best_candidate_index(candidates: &[MemoryCandidate], assigned: &[&MemoryExposure]) -> usize {
    let mut best_index = 0;
    let mut best_score = f32::NEG_INFINITY;
    for (index, candidate) in candidates.iter().enumerate() {
        let overlap = assigned
            .iter()
            .map(|memory| tag_overlap(&candidate.strategy_tags, &memory.strategy_tags))
            .fold(0.0_f32, f32::max);
        let adjusted = candidate.score - 0.25 * overlap;
        if adjusted > best_score
            || (adjusted == best_score && candidate.engram_id < candidates[best_index].engram_id)
        {
            best_index = index;
            best_score = adjusted;
        }
    }
    best_index
}

fn tag_overlap(left: &[String], right: &[String]) -> f32 {
    if left.is_empty() || right.is_empty() {
        return 0.0;
    }
    let left: HashSet<&str> = left.iter().map(String::as_str).collect();
    let right: HashSet<&str> = right.iter().map(String::as_str).collect();
    let union = left.union(&right).count();
    if union == 0 {
        0.0
    } else {
        left.intersection(&right).count() as f32 / union as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn worker(slot_id: &str) -> WorkerSlot {
        WorkerSlot {
            slot_id: slot_id.to_string(),
            role: "worker".to_string(),
            agent_id: None,
            worktree: None,
            status: WorkerStatus::Declared,
        }
    }

    fn candidate(id: u128, content: &str, score: f32, tags: &[&str]) -> MemoryCandidate {
        MemoryCandidate {
            engram_id: uuid::Uuid::from_u128(id),
            content: content.to_string(),
            score,
            strategy_tags: tags.iter().map(|tag| (*tag).to_string()).collect(),
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

    #[test]
    fn allocator_shares_truth_and_warnings_but_disjoins_episodes() {
        let roster = vec![worker("slot-b"), worker("slot-a")];
        let truth = vec![candidate(1, "Rust workspace uses Cargo", 1.0, &["fact"])];
        let warnings = vec![candidate(
            2,
            "Do not hold a lock across await",
            1.0,
            &["lock"],
        )];
        let episodes = vec![
            candidate(11, "use an actor", 0.95, &["coordination", "actor"]),
            candidate(12, "use a mutex", 0.90, &["coordination", "lock"]),
            candidate(13, "use channels", 0.85, &["channel"]),
            candidate(14, "use optimistic retries", 0.80, &["retry"]),
        ];

        let bundles = allocate_roster(&roster, &truth, &warnings, &episodes, 2).unwrap();
        let a = &bundles["slot-a"];
        let b = &bundles["slot-b"];

        assert_eq!(a.verified_truth, b.verified_truth);
        assert_eq!(a.mandatory_warnings, b.mandatory_warnings);
        let a_ids: HashSet<_> = a.episodic_memories.iter().map(|m| m.engram_id).collect();
        let b_ids: HashSet<_> = b.episodic_memories.iter().map(|m| m.engram_id).collect();
        assert!(a_ids.is_disjoint(&b_ids));
        assert!(a.strict_id_disjoint && b.strict_id_disjoint);
        assert!(!a.diversity_degraded && !b.diversity_degraded);
    }

    #[test]
    fn allocator_reports_shortage_without_duplicating_memory_ids() {
        let roster = vec![worker("slot-a"), worker("slot-b")];
        let episodes = vec![candidate(11, "only candidate", 0.9, &["one"])];

        let bundles = allocate_roster(&roster, &[], &[], &episodes, 1).unwrap();
        let assigned: Vec<_> = bundles
            .values()
            .flat_map(|bundle| bundle.episodic_memories.iter().map(|m| m.engram_id))
            .collect();
        let unique: HashSet<_> = assigned.iter().copied().collect();

        assert_eq!(assigned.len(), 1);
        assert_eq!(unique.len(), 1);
        assert!(bundles.values().all(|bundle| bundle.diversity_degraded));
    }

    #[test]
    fn allocator_is_deterministic_for_equal_inputs() {
        let roster = vec![worker("slot-b"), worker("slot-a")];
        let episodes = vec![
            candidate(12, "second", 0.8, &["same"]),
            candidate(11, "first", 0.8, &["same"]),
        ];

        let first = allocate_roster(&roster, &[], &[], &episodes, 1).unwrap();
        let second = allocate_roster(&roster, &[], &[], &episodes, 1).unwrap();

        assert_eq!(first, second);
        assert_eq!(
            first["slot-a"].episodic_memories[0].engram_id,
            uuid::Uuid::from_u128(11)
        );
    }

    #[test]
    fn warnings_never_enter_the_episodic_advice_lane() {
        let roster = vec![worker("slot-a")];
        let failed = candidate(50, "failed migration", 1.0, &["migration"]);

        let bundles = allocate_roster(&roster, &[], std::slice::from_ref(&failed), &[], 1).unwrap();
        let bundle = &bundles["slot-a"];

        assert_eq!(bundle.mandatory_warnings[0].engram_id, failed.engram_id);
        assert!(bundle.episodic_memories.is_empty());
    }
}
