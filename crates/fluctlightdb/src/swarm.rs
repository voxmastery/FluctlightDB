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
    #[serde(default)]
    pub allocations: HashMap<String, MemoryBundle>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClaimSlot {
    pub transaction_id: Uuid,
    pub swarm_id: Uuid,
    pub slot_id: String,
    pub agent_id: String,
    pub worktree: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CiteMemories {
    pub transaction_id: Uuid,
    pub swarm_id: Uuid,
    pub slot_id: String,
    pub memory_ids: Vec<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReportAttempt {
    pub transaction_id: Uuid,
    pub swarm_id: Uuid,
    pub slot_id: String,
    pub result_tree: String,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RecordEvidence {
    pub transaction_id: Uuid,
    pub swarm_id: Uuid,
    pub slot_id: String,
    pub receipt: EvidenceReceipt,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FinishSwarm {
    pub transaction_id: Uuid,
    pub swarm_id: Uuid,
    pub accepted_slot_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CitationReceipt {
    pub swarm_id: Uuid,
    pub slot_id: String,
    pub memory_ids: Vec<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PendingAttempt {
    pub swarm_id: Uuid,
    pub slot_id: String,
    pub attempt: Attempt,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VerifiedOutcome {
    pub swarm_id: Uuid,
    pub slot_id: String,
    pub receipt: EvidenceReceipt,
    pub credited_memory_ids: Vec<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SwarmSummary {
    pub swarm_id: Uuid,
    pub accepted_slot_id: String,
    pub status: SwarmStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "result", content = "value", rename_all = "snake_case")]
pub enum SwarmApplyResult {
    Began(SwarmRun),
    Claimed(MemoryBundle),
    Cited(CitationReceipt),
    Reported(PendingAttempt),
    Evidenced(VerifiedOutcome),
    Finished(SwarmSummary),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", content = "payload", rename_all = "snake_case")]
pub enum SwarmTransaction {
    Begin(BeginSwarm),
    Claim(ClaimSlot),
    Cite(CiteMemories),
    Report(ReportAttempt),
    Evidence(RecordEvidence),
    Finish(FinishSwarm),
}

impl SwarmTransaction {
    pub fn id(&self) -> Uuid {
        match self {
            Self::Begin(request) => request.transaction_id,
            Self::Claim(request) => request.transaction_id,
            Self::Cite(request) => request.transaction_id,
            Self::Report(request) => request.transaction_id,
            Self::Evidence(request) => request.transaction_id,
            Self::Finish(request) => request.transaction_id,
        }
    }
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
    #[error("swarm not found: {0}")]
    SwarmNotFound(Uuid),
    #[error("worker slot not found: {0}")]
    SlotNotFound(String),
    #[error("worker slot has no memory allocation: {0}")]
    AllocationNotFound(String),
    #[error("worker identity mismatch for slot {slot_id}")]
    IdentityMismatch { slot_id: String },
    #[error("worker slot has not been claimed: {0}")]
    SlotNotClaimed(String),
    #[error("memory was not exposed to this worker: {0}")]
    UnexposedMemory(Uuid),
    #[error("attempt has not been reported: {0}")]
    AttemptNotReported(String),
    #[error("attempt has not been verified: {0}")]
    AttemptNotVerified(String),
    #[error("swarm is not active: {0}")]
    SwarmNotActive(Uuid),
}

impl SwarmState {
    pub fn apply_transaction(
        &mut self,
        transaction: SwarmTransaction,
    ) -> Result<SwarmApplyResult, SwarmError> {
        if self.applied_transactions.contains(&transaction.id()) {
            return self.replay_result(&transaction);
        }
        let transaction_id = transaction.id();
        let result = match transaction {
            SwarmTransaction::Begin(request) => SwarmApplyResult::Began(self.begin_run(request)?),
            SwarmTransaction::Claim(request) => self.claim_slot(request)?,
            SwarmTransaction::Cite(request) => self.cite_memories(request)?,
            SwarmTransaction::Report(request) => self.report_attempt(request)?,
            SwarmTransaction::Evidence(request) => self.record_evidence(request)?,
            SwarmTransaction::Finish(request) => self.finish_swarm(request)?,
        };
        self.applied_transactions.insert(transaction_id);
        Ok(result)
    }

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
            allocations: request.allocations,
            attempts: HashMap::new(),
            status: SwarmStatus::Active,
        };
        self.runs.insert(run.id, run.clone());
        self.applied_transactions.insert(request.transaction_id);
        Ok(run)
    }

    pub fn route_candidates(
        &self,
        candidates: &[MemoryCandidate],
    ) -> (Vec<MemoryCandidate>, Vec<MemoryCandidate>) {
        let mut warnings = Vec::new();
        let mut advice = Vec::new();
        for candidate in candidates {
            let mut candidate = candidate.clone();
            if let Some(feedback) = self.feedback.get(&candidate.engram_id) {
                if feedback.reproduced_failures > 0 {
                    warnings.push(candidate);
                    continue;
                }
                candidate.score += (feedback.successes as f32 * 0.05).min(0.2);
            }
            advice.push(candidate);
        }
        warnings.sort_by(candidate_order);
        advice.sort_by(candidate_order);
        (warnings, advice)
    }

    fn replay_result(
        &self,
        transaction: &SwarmTransaction,
    ) -> Result<SwarmApplyResult, SwarmError> {
        match transaction {
            SwarmTransaction::Begin(request) => self
                .runs
                .get(&request.swarm_id)
                .cloned()
                .map(SwarmApplyResult::Began)
                .ok_or(SwarmError::TransactionConflict(request.transaction_id)),
            SwarmTransaction::Claim(request) => self
                .run(request.swarm_id)?
                .allocations
                .get(&request.slot_id)
                .cloned()
                .map(SwarmApplyResult::Claimed)
                .ok_or_else(|| SwarmError::AllocationNotFound(request.slot_id.clone())),
            SwarmTransaction::Cite(request) => {
                let attempt = self
                    .run(request.swarm_id)?
                    .attempts
                    .get(&request.slot_id)
                    .ok_or_else(|| SwarmError::SlotNotFound(request.slot_id.clone()))?;
                Ok(SwarmApplyResult::Cited(CitationReceipt {
                    swarm_id: request.swarm_id,
                    slot_id: request.slot_id.clone(),
                    memory_ids: attempt.cited_memory_ids.clone(),
                }))
            }
            SwarmTransaction::Report(request) => {
                let attempt = self
                    .run(request.swarm_id)?
                    .attempts
                    .get(&request.slot_id)
                    .cloned()
                    .ok_or_else(|| SwarmError::AttemptNotReported(request.slot_id.clone()))?;
                Ok(SwarmApplyResult::Reported(PendingAttempt {
                    swarm_id: request.swarm_id,
                    slot_id: request.slot_id.clone(),
                    attempt,
                }))
            }
            SwarmTransaction::Evidence(request) => {
                let attempt = self
                    .run(request.swarm_id)?
                    .attempts
                    .get(&request.slot_id)
                    .ok_or_else(|| SwarmError::AttemptNotReported(request.slot_id.clone()))?;
                let receipt = attempt
                    .evidence
                    .clone()
                    .ok_or_else(|| SwarmError::AttemptNotVerified(request.slot_id.clone()))?;
                Ok(SwarmApplyResult::Evidenced(VerifiedOutcome {
                    swarm_id: request.swarm_id,
                    slot_id: request.slot_id.clone(),
                    receipt,
                    credited_memory_ids: attempt.cited_memory_ids.clone(),
                }))
            }
            SwarmTransaction::Finish(request) => Ok(SwarmApplyResult::Finished(SwarmSummary {
                swarm_id: request.swarm_id,
                accepted_slot_id: request.accepted_slot_id.clone(),
                status: self.run(request.swarm_id)?.status.clone(),
            })),
        }
    }

    fn claim_slot(&mut self, request: ClaimSlot) -> Result<SwarmApplyResult, SwarmError> {
        let run = self.run_mut(request.swarm_id)?;
        if run.status != SwarmStatus::Active {
            return Err(SwarmError::SwarmNotActive(request.swarm_id));
        }
        let slot = run
            .roster
            .iter_mut()
            .find(|slot| slot.slot_id == request.slot_id)
            .ok_or_else(|| SwarmError::SlotNotFound(request.slot_id.clone()))?;
        match (&slot.agent_id, &slot.worktree) {
            (None, None) => {
                slot.agent_id = Some(request.agent_id);
                slot.worktree = Some(request.worktree);
                slot.status = WorkerStatus::Claimed;
            }
            (Some(agent_id), Some(worktree))
                if agent_id == &request.agent_id && worktree == &request.worktree => {}
            _ => {
                return Err(SwarmError::IdentityMismatch {
                    slot_id: request.slot_id,
                });
            }
        }
        let bundle = run
            .allocations
            .get(&slot.slot_id)
            .cloned()
            .ok_or_else(|| SwarmError::AllocationNotFound(slot.slot_id.clone()))?;
        Ok(SwarmApplyResult::Claimed(bundle))
    }

    fn cite_memories(&mut self, request: CiteMemories) -> Result<SwarmApplyResult, SwarmError> {
        let run = self.run_mut(request.swarm_id)?;
        ensure_claimed(run, &request.slot_id)?;
        let bundle = run
            .allocations
            .get(&request.slot_id)
            .ok_or_else(|| SwarmError::AllocationNotFound(request.slot_id.clone()))?;
        let exposed: HashSet<Uuid> = bundle
            .verified_truth
            .iter()
            .chain(&bundle.mandatory_warnings)
            .chain(&bundle.episodic_memories)
            .map(|memory| memory.engram_id)
            .collect();
        for memory_id in &request.memory_ids {
            if !exposed.contains(memory_id) {
                return Err(SwarmError::UnexposedMemory(*memory_id));
            }
        }
        let mut memory_ids = request.memory_ids;
        memory_ids.sort_unstable();
        memory_ids.dedup();
        run.attempts
            .entry(request.slot_id.clone())
            .or_default()
            .cited_memory_ids = memory_ids.clone();
        Ok(SwarmApplyResult::Cited(CitationReceipt {
            swarm_id: request.swarm_id,
            slot_id: request.slot_id,
            memory_ids,
        }))
    }

    fn report_attempt(&mut self, request: ReportAttempt) -> Result<SwarmApplyResult, SwarmError> {
        let run = self.run_mut(request.swarm_id)?;
        ensure_claimed(run, &request.slot_id)?;
        let attempt = run.attempts.entry(request.slot_id.clone()).or_default();
        attempt.result_tree = Some(request.result_tree);
        attempt.summary = Some(request.summary);
        let slot = run
            .roster
            .iter_mut()
            .find(|slot| slot.slot_id == request.slot_id)
            .expect("claimed slot still exists");
        slot.status = WorkerStatus::Reported;
        Ok(SwarmApplyResult::Reported(PendingAttempt {
            swarm_id: request.swarm_id,
            slot_id: request.slot_id,
            attempt: attempt.clone(),
        }))
    }

    fn record_evidence(&mut self, request: RecordEvidence) -> Result<SwarmApplyResult, SwarmError> {
        let (memory_ids, receipt) = {
            let run = self.run_mut(request.swarm_id)?;
            let attempt = run
                .attempts
                .get_mut(&request.slot_id)
                .ok_or_else(|| SwarmError::AttemptNotReported(request.slot_id.clone()))?;
            if attempt.result_tree.is_none() {
                return Err(SwarmError::AttemptNotReported(request.slot_id));
            }
            attempt.evidence = Some(request.receipt.clone());
            let slot = run
                .roster
                .iter_mut()
                .find(|slot| slot.slot_id == request.slot_id)
                .ok_or_else(|| SwarmError::SlotNotFound(request.slot_id.clone()))?;
            slot.status = WorkerStatus::Verified;
            (attempt.cited_memory_ids.clone(), request.receipt)
        };
        for memory_id in &memory_ids {
            self.feedback
                .entry(*memory_id)
                .or_default()
                .record(receipt.result);
        }
        Ok(SwarmApplyResult::Evidenced(VerifiedOutcome {
            swarm_id: request.swarm_id,
            slot_id: request.slot_id,
            receipt,
            credited_memory_ids: memory_ids,
        }))
    }

    fn finish_swarm(&mut self, request: FinishSwarm) -> Result<SwarmApplyResult, SwarmError> {
        let run = self.run_mut(request.swarm_id)?;
        let slot = run
            .roster
            .iter()
            .find(|slot| slot.slot_id == request.accepted_slot_id)
            .ok_or_else(|| SwarmError::SlotNotFound(request.accepted_slot_id.clone()))?;
        if slot.status != WorkerStatus::Verified {
            return Err(SwarmError::AttemptNotVerified(request.accepted_slot_id));
        }
        run.status = SwarmStatus::Finished;
        Ok(SwarmApplyResult::Finished(SwarmSummary {
            swarm_id: request.swarm_id,
            accepted_slot_id: request.accepted_slot_id,
            status: SwarmStatus::Finished,
        }))
    }

    fn run(&self, swarm_id: Uuid) -> Result<&SwarmRun, SwarmError> {
        self.runs
            .get(&swarm_id)
            .ok_or(SwarmError::SwarmNotFound(swarm_id))
    }

    fn run_mut(&mut self, swarm_id: Uuid) -> Result<&mut SwarmRun, SwarmError> {
        self.runs
            .get_mut(&swarm_id)
            .ok_or(SwarmError::SwarmNotFound(swarm_id))
    }
}

impl EngramFeedback {
    fn record(&mut self, result: EvidenceResult) {
        match result {
            EvidenceResult::Success => self.successes += 1,
            EvidenceResult::Failure => self.failures += 1,
            EvidenceResult::Inconclusive => self.inconclusive += 1,
            EvidenceResult::ReproducedFailure => self.reproduced_failures += 1,
        }
    }
}

fn ensure_claimed(run: &SwarmRun, slot_id: &str) -> Result<(), SwarmError> {
    let slot = run
        .roster
        .iter()
        .find(|slot| slot.slot_id == slot_id)
        .ok_or_else(|| SwarmError::SlotNotFound(slot_id.to_string()))?;
    if slot.agent_id.is_none() || slot.worktree.is_none() {
        return Err(SwarmError::SlotNotClaimed(slot_id.to_string()));
    }
    Ok(())
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
            allocations: HashMap::new(),
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
            allocations: HashMap::new(),
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
            allocations: HashMap::new(),
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

    fn allocated_state() -> (SwarmState, Uuid, Uuid) {
        let swarm_id = Uuid::new_v4();
        let memory_id = Uuid::new_v4();
        let mut allocations = HashMap::new();
        allocations.insert(
            "slot-a".into(),
            MemoryBundle {
                episodic_memories: vec![MemoryExposure {
                    engram_id: memory_id,
                    content: "try the actor strategy".into(),
                    score: 0.9,
                    strategy_tags: vec!["actor".into()],
                }],
                strict_id_disjoint: true,
                ..MemoryBundle::default()
            },
        );
        let mut state = SwarmState::default();
        state
            .apply_transaction(SwarmTransaction::Begin(BeginSwarm {
                transaction_id: Uuid::new_v4(),
                swarm_id,
                project_id: "fluctlight".into(),
                objective_digest: "sha256:objective".into(),
                repository_identity: "repo".into(),
                base_commit: "abc123".into(),
                policy_version: "v1".into(),
                roster: vec![worker("slot-a")],
                allocations,
            }))
            .unwrap();
        (state, swarm_id, memory_id)
    }

    #[test]
    fn claim_binds_real_agent_and_worktree_once() {
        let (mut state, swarm_id, _) = allocated_state();
        let result = state
            .apply_transaction(SwarmTransaction::Claim(ClaimSlot {
                transaction_id: Uuid::new_v4(),
                swarm_id,
                slot_id: "slot-a".into(),
                agent_id: "agent-7".into(),
                worktree: "/tmp/worktree-7".into(),
            }))
            .unwrap();

        assert!(matches!(result, SwarmApplyResult::Claimed(_)));
        let slot = &state.runs[&swarm_id].roster[0];
        assert_eq!(slot.agent_id.as_deref(), Some("agent-7"));
        assert_eq!(slot.worktree.as_deref(), Some("/tmp/worktree-7"));

        let spoof = state.apply_transaction(SwarmTransaction::Claim(ClaimSlot {
            transaction_id: Uuid::new_v4(),
            swarm_id,
            slot_id: "slot-a".into(),
            agent_id: "attacker".into(),
            worktree: "/tmp/other".into(),
        }));
        assert!(matches!(spoof, Err(SwarmError::IdentityMismatch { .. })));
    }

    #[test]
    fn citations_must_come_from_the_claimed_slots_exposure_set() {
        let (mut state, swarm_id, memory_id) = allocated_state();
        state
            .apply_transaction(SwarmTransaction::Claim(ClaimSlot {
                transaction_id: Uuid::new_v4(),
                swarm_id,
                slot_id: "slot-a".into(),
                agent_id: "agent-7".into(),
                worktree: "/tmp/worktree-7".into(),
            }))
            .unwrap();

        state
            .apply_transaction(SwarmTransaction::Cite(CiteMemories {
                transaction_id: Uuid::new_v4(),
                swarm_id,
                slot_id: "slot-a".into(),
                memory_ids: vec![memory_id],
            }))
            .unwrap();
        let unknown = state.apply_transaction(SwarmTransaction::Cite(CiteMemories {
            transaction_id: Uuid::new_v4(),
            swarm_id,
            slot_id: "slot-a".into(),
            memory_ids: vec![Uuid::new_v4()],
        }));

        assert!(matches!(unknown, Err(SwarmError::UnexposedMemory(_))));
    }

    #[test]
    fn verified_outcome_updates_only_cited_memory_feedback() {
        let (mut state, swarm_id, memory_id) = allocated_state();
        let uncited_id = Uuid::new_v4();
        state
            .apply_transaction(SwarmTransaction::Claim(ClaimSlot {
                transaction_id: Uuid::new_v4(),
                swarm_id,
                slot_id: "slot-a".into(),
                agent_id: "agent-7".into(),
                worktree: "/tmp/worktree-7".into(),
            }))
            .unwrap();
        state
            .apply_transaction(SwarmTransaction::Cite(CiteMemories {
                transaction_id: Uuid::new_v4(),
                swarm_id,
                slot_id: "slot-a".into(),
                memory_ids: vec![memory_id],
            }))
            .unwrap();
        state
            .apply_transaction(SwarmTransaction::Report(ReportAttempt {
                transaction_id: Uuid::new_v4(),
                swarm_id,
                slot_id: "slot-a".into(),
                result_tree: "tree-123".into(),
                summary: "tests pass".into(),
            }))
            .unwrap();
        state
            .apply_transaction(SwarmTransaction::Evidence(RecordEvidence {
                transaction_id: Uuid::new_v4(),
                swarm_id,
                slot_id: "slot-a".into(),
                receipt: EvidenceReceipt {
                    result: EvidenceResult::Success,
                    source_uri: "test://cargo-test".into(),
                    command_digest: "sha256:command".into(),
                },
            }))
            .unwrap();

        assert_eq!(state.feedback[&memory_id].successes, 1);
        assert!(!state.feedback.contains_key(&uncited_id));
    }

    #[test]
    fn reproduced_failure_feedback_routes_memory_to_warning_lane() {
        let memory_id = Uuid::new_v4();
        let candidate = MemoryCandidate {
            engram_id: memory_id,
            content: "unsafe migration".into(),
            score: 1.0,
            strategy_tags: vec!["migration".into()],
        };
        let mut state = SwarmState::default();
        state.feedback.insert(
            memory_id,
            EngramFeedback {
                reproduced_failures: 1,
                ..EngramFeedback::default()
            },
        );

        let (warnings, advice) = state.route_candidates(&[candidate]);

        assert_eq!(warnings.len(), 1);
        assert!(advice.is_empty());
    }
}
