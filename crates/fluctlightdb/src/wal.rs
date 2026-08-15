//! Write-ahead log v2 — segmented rotation + checkpoint watermark replay.

use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::brain::FluctlightBrain;
use crate::error::{Error, Result};
use crate::placement::DurabilityPolicy;
use crate::sleep_trigger::SleepTrigger;
use crate::types::Episode;

const MAX_SEGMENT_BYTES: u64 = 64 * 1024 * 1024;
const WAL_ENVELOPE_VERSION: u32 = 2;
const CANONICAL_MUTATION_SCHEMA_VERSION: u32 = 1;
const MUTATION_POLICY_VERSION: u32 = 1;

pub const WAL_COVERED_DURABLE_MUTATIONS: &[&str] = &[
    "FluctlightBrain::compact",
    "FluctlightBrain::death",
    "FluctlightBrain::experience",
    "FluctlightBrain::fovea_ingest",
    "FluctlightBrain::mark_core",
    "FluctlightBrain::reward",
    "FluctlightBrain::sleep",
    "FluctlightBrain::tick",
    "FluctlightBrain::tick_n",
];

pub const WAL_DISTRIBUTED_DISABLED_MUTATIONS: &[&str] = &[
    "FluctlightBrain::api_inhibit",
    "FluctlightBrain::api_set_goal",
    "FluctlightBrain::apply_retention",
    "FluctlightBrain::chorus_sleep",
    "FluctlightBrain::consolidate",
    "FluctlightBrain::delete_by_agent_id",
    "FluctlightBrain::delete_by_subject",
    "FluctlightBrain::flush_wm",
    "FluctlightBrain::forget_before_tick",
    "FluctlightBrain::muon_imprint",
    "FluctlightBrain::muon_imprint_batch",
    "FluctlightBrain::neurogenesis_pulse",
    "FluctlightBrain::observe_tool",
    "FluctlightBrain::reconsolidate",
    "FluctlightBrain::scrub_pii",
    "FluctlightBrain::turn_end",
    "FluctlightBrain::verify_fact",
    "brain_snapshot::import_snapshot",
    "brain_snapshot::import_snapshot_json",
    "query::execute_mut",
    "query::forget_before",
    "query::forget_engram",
    "raw_export::import_raw",
    "raw_export::import_raw_json",
];

pub const WAL_UNCOVERED_DURABLE_MUTATIONS: &[&str] = &[];

fn legacy_envelope_version() -> u32 {
    1
}

fn legacy_mutation_version() -> u32 {
    0
}

pub fn wal_enabled() -> bool {
    std::env::var("FLUCTLIGHT_WAL")
        .map(|v| v != "0" && !v.eq_ignore_ascii_case("false"))
        .unwrap_or(true)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
#[allow(clippy::large_enum_variant)]
pub enum WalEntry {
    Experience {
        episode: Episode,
        #[serde(default)]
        assigned_engram_id: Option<uuid::Uuid>,
    },
    Sleep,
    Tick {
        n: u64,
    },
    Reward {
        magnitude: f32,
    },
    MarkCore {
        engram_id: uuid::Uuid,
        key: String,
    },
    Death {
        cause: String,
    },
    Compact,
    SwarmTransaction {
        transaction: crate::swarm::SwarmTransaction,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanonicalMutation {
    pub schema_version: u32,
    pub assigned_ids: Vec<uuid::Uuid>,
    pub deterministic_seed: [u8; 32],
    pub policy_version: u32,
    pub mutation: WalEntry,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WalIdentity {
    pub tenant_uuid: uuid::Uuid,
    pub writer_epoch: u64,
    pub fence_generation: u64,
    pub durability: DurabilityPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WalReplicationFrame {
    pub tenant_uuid: uuid::Uuid,
    pub writer_epoch: u64,
    pub fence_generation: u64,
    pub seq: u64,
    pub operation_id: String,
    pub sha256: [u8; 32],
    pub payload: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WalRecord {
    #[serde(default = "legacy_envelope_version")]
    envelope_version: u32,
    #[serde(default = "legacy_mutation_version")]
    mutation_schema_version: u32,
    #[serde(default)]
    assigned_ids: Vec<uuid::Uuid>,
    #[serde(default)]
    deterministic_seed: [u8; 32],
    #[serde(default = "legacy_mutation_version")]
    policy_version: u32,
    seq: u64,
    #[serde(default)]
    operation_id: Option<String>,
    #[serde(default)]
    writer_epoch: u64,
    #[serde(default)]
    tenant_uuid: Option<uuid::Uuid>,
    #[serde(default)]
    fence_generation: u64,
    #[serde(default, rename = "durability_policy")]
    durability: Option<DurabilityPolicy>,
    #[serde(default)]
    fence_token: u64,
    #[serde(default)]
    idempotency_key: Option<String>,
    #[serde(flatten)]
    entry: WalEntry,
}

impl WalRecord {
    fn new(seq: u64, entry: WalEntry) -> Self {
        let assigned_ids = match &entry {
            WalEntry::Experience {
                assigned_engram_id: Some(id),
                ..
            } => vec![*id],
            _ => Vec::new(),
        };
        let mut seed = Sha256::new();
        seed.update(seq.to_le_bytes());
        seed.update(serde_json::to_vec(&entry).unwrap_or_default());
        Self {
            envelope_version: WAL_ENVELOPE_VERSION,
            mutation_schema_version: CANONICAL_MUTATION_SCHEMA_VERSION,
            assigned_ids,
            deterministic_seed: seed.finalize().into(),
            policy_version: MUTATION_POLICY_VERSION,
            seq,
            operation_id: Some(uuid::Uuid::new_v4().to_string()),
            writer_epoch: 0,
            tenant_uuid: None,
            fence_generation: 0,
            durability: None,
            fence_token: seq,
            idempotency_key: None,
            entry,
        }
    }

    fn new_fenced(seq: u64, entry: WalEntry, identity: &WalIdentity) -> Self {
        let mut record = Self::new(seq, entry);
        record.writer_epoch = identity.writer_epoch;
        record.tenant_uuid = Some(identity.tenant_uuid);
        record.fence_generation = identity.fence_generation;
        record.durability = Some(identity.durability);
        record
    }

    fn identity(&self) -> Option<WalIdentity> {
        Some(WalIdentity {
            tenant_uuid: self.tenant_uuid?,
            writer_epoch: self.writer_epoch,
            fence_generation: self.fence_generation,
            durability: self.durability?,
        })
    }

    #[allow(dead_code)]
    fn canonical_mutation(&self) -> CanonicalMutation {
        CanonicalMutation {
            schema_version: self.mutation_schema_version,
            assigned_ids: self.assigned_ids.clone(),
            deterministic_seed: self.deterministic_seed,
            policy_version: self.policy_version,
            mutation: self.entry.clone(),
        }
    }

    fn validate_envelope(&self) -> Result<()> {
        match self.envelope_version {
            1 => Ok(()),
            WAL_ENVELOPE_VERSION
                if self.operation_id.is_some()
                    && self.fence_token == self.seq
                    && self.mutation_schema_version == CANONICAL_MUTATION_SCHEMA_VERSION
                    && self.policy_version == MUTATION_POLICY_VERSION =>
            {
                Ok(())
            }
            WAL_ENVELOPE_VERSION => Err(Error::Store("invalid WAL v2 envelope".into())),
            version => Err(Error::Store(format!(
                "unsupported WAL envelope version {version}"
            ))),
        }
    }
}

pub fn wal_base(brain_path: &Path) -> PathBuf {
    if crate::storage::is_v4_path(brain_path) {
        brain_path.join("wal").join("brain.wal")
    } else {
        brain_path.with_extension("flct.wal")
    }
}

pub fn wal_path(brain_path: &Path) -> PathBuf {
    active_segment(brain_path)
}

fn active_segment(brain_path: &Path) -> PathBuf {
    let base = wal_base(brain_path);
    let mut idx = 1u32;
    loop {
        let path = segment_path(&base, idx);
        if !path.exists() {
            return path;
        }
        let len = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        if len < MAX_SEGMENT_BYTES {
            return path;
        }
        idx += 1;
    }
}

fn segment_path(base: &Path, idx: u32) -> PathBuf {
    PathBuf::from(format!("{}.{:03}", base.display(), idx))
}

fn open_append_private(path: &Path) -> Result<File> {
    let mut options = OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        options.mode(0o600);
        let file = options.open(path)?;
        file.set_permissions(fs::Permissions::from_mode(0o600))?;
        Ok(file)
    }
    #[cfg(not(unix))]
    Ok(options.open(path)?)
}

pub fn list_segments(brain_path: &Path) -> Vec<PathBuf> {
    list_segments_inner(brain_path)
}

/// Public alias used by replicate/DR paths.
pub fn list_segments_public(brain_path: &Path) -> Vec<PathBuf> {
    list_segments(brain_path)
}

fn list_segments_inner(brain_path: &Path) -> Vec<PathBuf> {
    let base = wal_base(brain_path);
    let parent = base.parent().unwrap_or(Path::new("."));
    let stem = base
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("brain.flct.wal");
    let prefix = format!("{stem}.");
    let mut numbered = Vec::new();
    if let Ok(read) = fs::read_dir(parent) {
        for entry in read.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if let Some(suffix) = name.strip_prefix(&prefix) {
                if !suffix.is_empty() && suffix.bytes().all(|b| b.is_ascii_digit()) {
                    if let Ok(index) = suffix.parse::<u64>() {
                        numbered.push((index, entry.path()));
                    }
                }
            }
        }
    }
    numbered.sort_by_key(|(index, _)| *index);
    let mut out: Vec<_> = numbered.into_iter().map(|(_, path)| path).collect();
    if out.is_empty() && base.exists() {
        out.push(base);
    }
    out
}

pub fn append(brain_path: &Path, seq: u64, entry: &WalEntry) -> Result<()> {
    append_record(brain_path, WalRecord::new(seq, entry.clone()))
}

pub fn append_fenced(
    brain_path: &Path,
    seq: u64,
    entry: &WalEntry,
    identity: &WalIdentity,
) -> Result<()> {
    validate_existing_identity(brain_path, identity)?;
    append_record(
        brain_path,
        WalRecord::new_fenced(seq, entry.clone(), identity),
    )
}

pub fn replication_frames(
    brain_path: &Path,
    after_seq: u64,
    through_seq: u64,
    identity: &WalIdentity,
) -> Result<Vec<WalReplicationFrame>> {
    if through_seq < after_seq {
        return Err(Error::Store("invalid WAL replication range".into()));
    }
    let mut expected = after_seq.saturating_add(1);
    let mut frames = Vec::new();
    for path in list_segments(brain_path) {
        let mut reader = BufReader::new(File::open(path)?);
        let mut line = String::new();
        loop {
            line.clear();
            if reader.read_line(&mut line)? == 0 {
                break;
            }
            if line.trim().is_empty() {
                continue;
            }
            if !line.ends_with('\n') {
                return Err(Error::Store("torn WAL record cannot be replicated".into()));
            }
            let payload = line.trim_end_matches('\n').as_bytes().to_vec();
            let record: WalRecord = serde_json::from_slice(&payload)
                .map_err(|error| Error::Serde(error.to_string()))?;
            record.validate_envelope()?;
            if record.seq <= after_seq {
                continue;
            }
            if record.seq > through_seq {
                break;
            }
            if record.seq != expected {
                return Err(Error::Store(format!(
                    "WAL replication range is not contiguous: expected {expected}, found {}",
                    record.seq
                )));
            }
            if record.identity().as_ref() != Some(identity) {
                return Err(Error::Store("stale or mixed WAL fence generation".into()));
            }
            frames.push(WalReplicationFrame {
                tenant_uuid: identity.tenant_uuid,
                writer_epoch: identity.writer_epoch,
                fence_generation: identity.fence_generation,
                seq: record.seq,
                operation_id: record
                    .operation_id
                    .clone()
                    .ok_or_else(|| Error::Store("WAL record lacks operation id".into()))?,
                sha256: Sha256::digest(&payload).into(),
                payload,
            });
            expected = expected.saturating_add(1);
        }
    }
    if expected != through_seq.saturating_add(1) {
        return Err(Error::Store(format!(
            "WAL replication range ended before sequence {through_seq}"
        )));
    }
    Ok(frames)
}

pub fn append_replication_frames(
    brain_path: &Path,
    expected_start: u64,
    frames: &[WalReplicationFrame],
    identity: &WalIdentity,
) -> Result<u64> {
    let mut expected = expected_start;
    for frame in frames {
        if frame.tenant_uuid != identity.tenant_uuid
            || frame.writer_epoch != identity.writer_epoch
            || frame.fence_generation != identity.fence_generation
        {
            return Err(Error::Store("stale or mixed WAL fence generation".into()));
        }
        if frame.seq != expected {
            return Err(Error::Store(format!(
                "WAL frames must be contiguous: expected {expected}, found {}",
                frame.seq
            )));
        }
        if <[u8; 32]>::from(Sha256::digest(&frame.payload)) != frame.sha256 {
            return Err(Error::Store(format!(
                "WAL frame SHA-256 mismatch at sequence {}",
                frame.seq
            )));
        }
        let record: WalRecord = serde_json::from_slice(&frame.payload)
            .map_err(|error| Error::Serde(error.to_string()))?;
        record.validate_envelope()?;
        if record.seq != frame.seq
            || record.operation_id.as_deref() != Some(frame.operation_id.as_str())
            || record.identity().as_ref() != Some(identity)
        {
            return Err(Error::Store(
                "WAL frame payload has stale or mixed identity".into(),
            ));
        }
        expected = expected.saturating_add(1);
    }
    if frames.is_empty() {
        return Ok(expected_start.saturating_sub(1));
    }
    validate_existing_identity(brain_path, identity)?;
    let path = active_segment(brain_path);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = open_append_private(&path)?;
    for frame in frames {
        file.write_all(&frame.payload)?;
        file.write_all(b"\n")?;
    }
    file.sync_all()?;
    Ok(frames.last().map(|frame| frame.seq).unwrap_or_default())
}

fn append_record(brain_path: &Path, record: WalRecord) -> Result<()> {
    let path = active_segment(brain_path);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    repair_torn_tail(&path)?;
    let line = serde_json::to_string(&record).map_err(|e| Error::Serde(e.to_string()))?;
    let mut file = open_append_private(&path)?;
    writeln!(file, "{line}")?;
    crate::wal_sync::append_and_sync(brain_path, &mut file, line.len() + 1)?;
    Ok(())
}

fn repair_torn_tail(path: &Path) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    let bytes = fs::read(path)?;
    if bytes.is_empty() || bytes.ends_with(b"\n") {
        return Ok(());
    }
    let complete_len = bytes
        .iter()
        .rposition(|byte| *byte == b'\n')
        .map(|index| index + 1)
        .unwrap_or(0);
    let file = OpenOptions::new().write(true).open(path)?;
    file.set_len(complete_len as u64)?;
    file.sync_all()?;
    Ok(())
}

fn validate_existing_identity(brain_path: &Path, expected: &WalIdentity) -> Result<()> {
    for path in list_segments(brain_path) {
        let reader = BufReader::new(File::open(path)?);
        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            let record: WalRecord =
                serde_json::from_str(&line).map_err(|error| Error::Serde(error.to_string()))?;
            if record.identity().as_ref() != Some(expected) {
                return Err(Error::Store(format!(
                    "mixed-generation WAL: expected tenant {} writer epoch {} fence generation {}",
                    expected.tenant_uuid, expected.writer_epoch, expected.fence_generation
                )));
            }
        }
    }
    Ok(())
}

pub fn validate_replay_identity(brain_path: &Path, expected: &WalIdentity) -> Result<()> {
    for path in list_segments(brain_path) {
        let reader = BufReader::new(File::open(path)?);
        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            let record: WalRecord =
                serde_json::from_str(&line).map_err(|error| Error::Serde(error.to_string()))?;
            record.validate_envelope()?;
            if record.identity().as_ref() != Some(expected) {
                return Err(Error::Store(format!(
                    "stale or mixed-generation WAL: expected tenant {} writer epoch {} fence generation {}",
                    expected.tenant_uuid, expected.writer_epoch, expected.fence_generation
                )));
            }
        }
    }
    Ok(())
}

pub fn flush(brain_path: &Path) -> Result<()> {
    let path = active_segment(brain_path);
    if !path.exists() {
        return Ok(());
    }
    let mut file = open_append_private(&path)?;
    crate::wal_sync::flush_path(brain_path, &mut file)
}

pub fn truncate(brain_path: &Path) -> Result<()> {
    let mut deleted = Vec::new();
    for path in list_segments(brain_path) {
        if path.exists() {
            crate::checkpoint_fault::hit("wal.before_delete");
            fs::remove_file(&path)?;
            crate::checkpoint_fault::hit("wal.after_delete");
            deleted.push(path);
        }
    }
    deleted.sort_by(|a, b| a.parent().cmp(&b.parent()));
    deleted.dedup_by(|a, b| a.parent() == b.parent());
    for path in deleted {
        crate::segment::sync_parent_dir(&path)?;
        crate::checkpoint_fault::hit("wal.after_dir_fsync");
    }
    Ok(())
}

pub fn replay(brain: &mut FluctlightBrain, brain_path: &Path, after_seq: u64) -> Result<u64> {
    let mut count = 0u64;
    for path in list_segments(brain_path) {
        if !path.exists() {
            continue;
        }
        let file = File::open(&path)?;
        let reader = BufReader::new(file);
        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            let record: WalRecord =
                serde_json::from_str(&line).map_err(|e| Error::Serde(e.to_string()))?;
            record.validate_envelope()?;
            validate_record_for_brain(&record, brain)?;
            if record.seq <= after_seq {
                continue;
            }
            apply_entry(brain, record.entry)?;
            count += 1;
        }
    }
    Ok(count)
}

pub fn replay_fenced(
    brain: &mut FluctlightBrain,
    brain_path: &Path,
    after_seq: u64,
    expected: &WalIdentity,
) -> Result<u64> {
    validate_replay_identity(brain_path, expected)?;
    replay(brain, brain_path, after_seq)
}

pub fn replay_with_corruption_skip(
    brain: &mut FluctlightBrain,
    brain_path: &Path,
    after_seq: u64,
) -> Result<(u64, u64)> {
    let mut count = 0u64;
    let mut skipped = 0u64;
    let segments = list_segments(brain_path);
    let final_segment = segments.len().saturating_sub(1);
    let mut lines: Vec<(String, bool)> = Vec::new();
    for (segment_index, path) in segments.into_iter().enumerate() {
        let raw = fs::read_to_string(&path)?;
        let torn_tail = segment_index == final_segment && !raw.as_bytes().ends_with(b"\n");
        let start = lines.len();
        lines.extend(
            raw.lines()
                .filter(|line| !line.trim().is_empty())
                .map(|line| (line.to_owned(), false)),
        );
        if torn_tail && lines.len() > start {
            lines.last_mut().unwrap().1 = true;
        }
    }
    let mut expected_seq = after_seq.saturating_add(1);
    let last = lines.len().saturating_sub(1);
    for (index, (line, torn_tail)) in lines.into_iter().enumerate() {
        let record: WalRecord = match serde_json::from_str(&line) {
            Ok(record) => record,
            Err(_error) if index == last && torn_tail => {
                skipped += 1;
                continue;
            }
            Err(error) => {
                let location = if index == last { "final" } else { "interior" };
                return Err(Error::Store(format!(
                    "{location} WAL corruption at record {}: {error}",
                    index + 1
                )));
            }
        };
        record.validate_envelope()?;
        validate_record_for_brain(&record, brain)?;
        if record.seq <= after_seq {
            continue;
        }
        if record.seq != expected_seq {
            return Err(Error::Store(format!(
                "WAL sequence gap: expected {expected_seq}, found {}",
                record.seq
            )));
        }
        apply_entry(brain, record.entry)?;
        expected_seq = expected_seq.saturating_add(1);
        count += 1;
    }
    Ok((count, skipped))
}

fn validate_record_for_brain(record: &WalRecord, brain: &FluctlightBrain) -> Result<()> {
    match (brain.wal_identity(), record.identity()) {
        (None, None) => Ok(()),
        (Some(expected), Some(actual)) if expected == actual => Ok(()),
        (Some(expected), _) => Err(Error::Store(format!(
            "stale or mixed-generation WAL on replay: expected tenant {} writer epoch {} fence generation {}",
            expected.tenant_uuid, expected.writer_epoch, expected.fence_generation
        ))),
        (None, Some(_)) => Err(Error::Store(
            "fenced WAL cannot replay without manifest placement identity".into(),
        )),
    }
}

fn apply_entry(brain: &mut FluctlightBrain, entry: WalEntry) -> Result<()> {
    match entry {
        WalEntry::Experience {
            episode,
            assigned_engram_id,
        } => {
            brain.experience_internal_assigned(episode, false, assigned_engram_id)?;
        }
        WalEntry::Sleep => {
            brain.sleep_internal(false, SleepTrigger::Manual)?;
        }
        WalEntry::Tick { n } => {
            for _ in 0..n {
                brain.tick_internal(false)?;
            }
        }
        WalEntry::Reward { magnitude } => {
            brain.reward_internal(magnitude);
        }
        WalEntry::MarkCore { engram_id, key } => {
            brain.mark_core_internal(engram_id, key);
        }
        WalEntry::Death { cause } => {
            brain.death_internal(&cause, false)?;
        }
        WalEntry::Compact => {
            brain.compact_internal(false)?;
        }
        WalEntry::SwarmTransaction { transaction } => {
            brain.apply_swarm_transaction_internal(transaction, false)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::swarm::{BeginSwarm, SwarmTransaction, WorkerSlot, WorkerStatus};
    use crate::types::Episode;
    use tempfile::tempdir;

    fn begin_transaction(transaction_id: uuid::Uuid, swarm_id: uuid::Uuid) -> SwarmTransaction {
        SwarmTransaction::Begin(BeginSwarm {
            transaction_id,
            swarm_id,
            project_id: "fluctlight".into(),
            objective_digest: "sha256:objective".into(),
            repository_identity: "repo".into(),
            base_commit: "abc123".into(),
            policy_version: "v1".into(),
            roster: vec![WorkerSlot {
                slot_id: "slot-a".into(),
                role: "worker".into(),
                agent_id: None,
                worktree: None,
                status: WorkerStatus::Declared,
            }],
            allocations: std::collections::HashMap::new(),
        })
    }

    #[test]
    #[cfg_attr(miri, ignore = "opens brain (sqlite3 FFI)")]
    fn wal_replays_experience_after_checkpoint_gap() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.flct");
        let brain = FluctlightBrain::open(&path).unwrap();
        brain.checkpoint().unwrap();
        drop(brain);

        append(
            &path,
            1,
            &WalEntry::Experience {
                episode: Episode {
                    content: "wal replay test".into(),
                    context: "test".into(),
                    outcome: None,
                    salience_hint: 0.6,
                    semantic_vector: None,
                    agent_id: None,
                    tenant_id: None,
                    rag: None,
                    provenance: None,
                },
                assigned_engram_id: None,
            },
        )
        .unwrap();

        let fresh = FluctlightBrain::open(&path).unwrap();
        assert!(!fresh.activate("wal replay").recalls.is_empty());
    }

    #[test]
    #[cfg_attr(miri, ignore = "opens brain (sqlite3 FFI)")]
    fn wal_rejects_sequence_gap() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("gap.flct");
        let brain = FluctlightBrain::open(&path).unwrap();
        brain.checkpoint().unwrap();
        drop(brain);
        append(&path, 2, &WalEntry::Tick { n: 1 }).unwrap();

        let err = FluctlightBrain::open(&path).unwrap_err();
        assert!(err.to_string().contains("WAL sequence gap"), "{err}");
    }

    #[test]
    fn wal_segments_use_exact_names_and_numeric_order() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("segments.flct");
        let base = wal_base(&path);
        fs::write(PathBuf::from(format!("{}.999", base.display())), b"").unwrap();
        fs::write(PathBuf::from(format!("{}.1000", base.display())), b"").unwrap();
        fs::write(PathBuf::from(format!("{}.001.tmp", base.display())), b"").unwrap();
        fs::write(PathBuf::from(format!("{}.backup", base.display())), b"").unwrap();

        let names: Vec<_> = list_segments(&path)
            .into_iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            names,
            vec!["segments.flct.wal.999", "segments.flct.wal.1000"]
        );
    }

    #[cfg(unix)]
    #[test]
    fn wal_segment_is_owner_only() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempdir().unwrap();
        let path = dir.path().join("private.flct");
        append(&path, 1, &WalEntry::Tick { n: 1 }).unwrap();
        let mode = fs::metadata(wal_path(&path)).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[test]
    #[cfg_attr(miri, ignore = "opens brain (sqlite3 FFI)")]
    fn wal_rejects_interior_corrupt_line() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.flct");
        let brain = FluctlightBrain::open(&path).unwrap();
        brain.checkpoint().unwrap();
        let wal = active_segment(&path);
        {
            let mut f = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&wal)
                .unwrap();
            writeln!(f, "{{not valid json").unwrap();
            let good = WalRecord::new(
                1,
                WalEntry::Experience {
                    episode: Episode {
                        content: "after corrupt".into(),
                        context: "t".into(),
                        outcome: None,
                        salience_hint: 0.5,
                        semantic_vector: None,
                        agent_id: None,
                        tenant_id: None,
                        rag: None,
                        provenance: None,
                    },
                    assigned_engram_id: None,
                },
            );
            writeln!(f, "{}", serde_json::to_string(&good).unwrap()).unwrap();
        }
        drop(brain);
        let err = FluctlightBrain::open(&path).unwrap_err();
        assert!(err.to_string().contains("interior WAL corruption"), "{err}");
    }

    #[test]
    #[cfg_attr(miri, ignore = "opens brain (sqlite3 FFI)")]
    fn wal_recovers_after_truncated_tail() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.flct");
        let brain = FluctlightBrain::open(&path).unwrap();
        brain.checkpoint().unwrap();
        let wal = active_segment(&path);
        {
            let mut f = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&wal)
                .unwrap();
            let good = WalRecord::new(
                1,
                WalEntry::Experience {
                    episode: Episode {
                        content: "before truncate".into(),
                        context: "t".into(),
                        outcome: None,
                        salience_hint: 0.5,
                        semantic_vector: None,
                        agent_id: None,
                        tenant_id: None,
                        rag: None,
                        provenance: None,
                    },
                    assigned_engram_id: None,
                },
            );
            let line = serde_json::to_string(&good).unwrap();
            writeln!(f, "{line}").unwrap();
            // Simulate torn write: partial JSON line at EOF (kill -9 mid-append).
            f.write_all(b"{\"seq\":2,\"entry\":{\"Experience\":")
                .unwrap();
            f.sync_all().unwrap();
        }
        drop(brain);
        let replay_brain = FluctlightBrain::open(&path).unwrap();
        assert!(!replay_brain.activate("before truncate").recalls.is_empty());
    }

    #[test]
    #[cfg_attr(miri, ignore = "opens brain (sqlite3 FFI)")]
    fn wal_rejects_newline_terminated_corrupt_final_record() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("complete-corrupt.flct");
        let brain = FluctlightBrain::open(&path).unwrap();
        brain.checkpoint().unwrap();
        drop(brain);
        fs::write(active_segment(&path), b"{not valid json}\n").unwrap();

        let err = FluctlightBrain::open(&path).unwrap_err();
        assert!(err.to_string().contains("WAL corruption"), "{err}");
    }

    #[test]
    #[cfg_attr(miri, ignore = "writes WAL files")]
    fn replaying_death_does_not_append_a_new_wal_record() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("death.flct");
        let mut brain = FluctlightBrain::new();
        brain.attach_store_path(path.clone());

        apply_entry(
            &mut brain,
            WalEntry::Death {
                cause: "replayed".into(),
            },
        )
        .unwrap();

        assert_eq!(brain.wal_seq, 0);
        assert!(list_segments(&path).is_empty());
    }

    /// Miri-safe: WAL JSON wire format (no sqlite / filesystem brain).
    #[test]
    fn miri_wal_record_json_roundtrip() {
        let mut record = WalRecord::new(
            42,
            WalEntry::Experience {
                episode: Episode {
                    content: "miri wal line".into(),
                    context: "test".into(),
                    outcome: None,
                    salience_hint: 0.5,
                    semantic_vector: None,
                    agent_id: None,
                    tenant_id: None,
                    rag: None,
                    provenance: None,
                },
                assigned_engram_id: None,
            },
        );
        record.idempotency_key = Some("idem-1".into());
        let line = serde_json::to_string(&record).unwrap();
        let back: WalRecord = serde_json::from_str(&line).unwrap();
        assert_eq!(back.seq, 42);
        assert!(matches!(back.entry, WalEntry::Experience { .. }));
    }

    #[test]
    #[cfg_attr(miri, ignore = "opens brain (sqlite3 FFI)")]
    fn wal_replays_swarm_transaction_after_checkpoint_gap() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("swarm-brain");
        let brain = FluctlightBrain::open(&path).unwrap();
        brain.checkpoint().unwrap();
        drop(brain);
        let swarm_id = uuid::Uuid::new_v4();
        let transaction = begin_transaction(uuid::Uuid::new_v4(), swarm_id);

        append(&path, 1, &WalEntry::SwarmTransaction { transaction }).unwrap();

        let loaded = FluctlightBrain::open(&path).unwrap();
        assert!(loaded.swarm.runs.contains_key(&swarm_id));
        drop(loaded);
        let reopened = FluctlightBrain::open(&path).unwrap();
        assert!(reopened.swarm.runs.contains_key(&swarm_id));
    }

    #[test]
    #[cfg_attr(miri, ignore = "opens brain (sqlite3 FFI)")]
    fn duplicate_swarm_transactions_replay_once() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("swarm-brain");
        let brain = FluctlightBrain::open(&path).unwrap();
        brain.checkpoint().unwrap();
        drop(brain);
        let swarm_id = uuid::Uuid::new_v4();
        let transaction = begin_transaction(uuid::Uuid::new_v4(), swarm_id);

        append(
            &path,
            1,
            &WalEntry::SwarmTransaction {
                transaction: transaction.clone(),
            },
        )
        .unwrap();
        append(&path, 2, &WalEntry::SwarmTransaction { transaction }).unwrap();

        let loaded = FluctlightBrain::open(&path).unwrap();
        assert_eq!(loaded.swarm.runs.len(), 1);
        assert_eq!(loaded.swarm.applied_transactions.len(), 1);
    }

    #[test]
    fn wal_v2_envelope_has_operation_identity_and_fencing() {
        let record = WalRecord::new(42, WalEntry::Tick { n: 1 });
        let value = serde_json::to_value(&record).unwrap();
        assert_eq!(value["envelope_version"], 2);
        assert_eq!(value["mutation_schema_version"], 1);
        assert_eq!(value["policy_version"], 1);
        assert_eq!(value["deterministic_seed"].as_array().unwrap().len(), 32);
        let canonical = record.canonical_mutation();
        assert_eq!(canonical.schema_version, 1);
        assert_eq!(value["fence_token"], 42);
        assert!(value["operation_id"]
            .as_str()
            .and_then(|id| uuid::Uuid::parse_str(id).ok())
            .is_some());

        let legacy: WalRecord = serde_json::from_str(r#"{"seq":1,"op":"tick","n":1}"#).unwrap();
        assert_eq!(legacy.envelope_version, 1);
        assert!(legacy.operation_id.is_none());
    }

    #[test]
    fn fenced_wal_persists_identity_policy_and_rejects_mixed_generation_append() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("fenced.flct");
        let identity = WalIdentity {
            tenant_uuid: uuid::Uuid::from_u128(99),
            writer_epoch: 4,
            fence_generation: 7,
            durability: crate::placement::DurabilityPolicy::Quorum,
        };
        append_fenced(&path, 1, &WalEntry::Tick { n: 1 }, &identity).unwrap();

        let line = fs::read_to_string(wal_path(&path)).unwrap();
        let value: serde_json::Value = serde_json::from_str(line.trim()).unwrap();
        assert_eq!(value["tenant_uuid"], identity.tenant_uuid.to_string());
        assert_eq!(value["writer_epoch"], 4);
        assert_eq!(value["fence_generation"], 7);
        assert_eq!(value["durability_policy"], "Quorum");

        let stale = WalIdentity {
            fence_generation: 8,
            ..identity
        };
        let error = append_fenced(&path, 2, &WalEntry::Tick { n: 1 }, &stale).unwrap_err();
        assert!(
            error.to_string().contains("mixed-generation WAL"),
            "{error}"
        );
    }

    #[test]
    fn fenced_wal_replay_validation_rejects_stale_or_local_records() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("replay-fenced.flct");
        let current = WalIdentity {
            tenant_uuid: uuid::Uuid::from_u128(100),
            writer_epoch: 5,
            fence_generation: 11,
            durability: crate::placement::DurabilityPolicy::All,
        };
        append_fenced(&path, 1, &WalEntry::Tick { n: 1 }, &current).unwrap();
        validate_replay_identity(&path, &current).unwrap();

        let stale = WalIdentity {
            fence_generation: 10,
            ..current
        };
        assert!(validate_replay_identity(&path, &stale)
            .unwrap_err()
            .to_string()
            .contains("stale or mixed-generation WAL"));
    }

    #[test]
    fn durable_mutation_wal_coverage_is_exact() {
        assert_eq!(
            WAL_COVERED_DURABLE_MUTATIONS,
            &[
                "FluctlightBrain::compact",
                "FluctlightBrain::death",
                "FluctlightBrain::experience",
                "FluctlightBrain::fovea_ingest",
                "FluctlightBrain::mark_core",
                "FluctlightBrain::reward",
                "FluctlightBrain::sleep",
                "FluctlightBrain::tick",
                "FluctlightBrain::tick_n",
            ]
        );
        assert!(WAL_UNCOVERED_DURABLE_MUTATIONS.is_empty());
        assert_eq!(WAL_DISTRIBUTED_DISABLED_MUTATIONS.len(), 24);
    }

    #[test]
    fn canonical_experience_replay_preserves_primary_assigned_engram_id() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("canonical-id.flct");
        let brain = FluctlightBrain::open(&path).unwrap();
        brain.checkpoint().unwrap();
        drop(brain);
        let assigned_engram_id = uuid::Uuid::from_u128(123_456);
        append(
            &path,
            1,
            &WalEntry::Experience {
                episode: Episode::new("deterministic replay", "phase4", 0.8),
                assigned_engram_id: Some(assigned_engram_id),
            },
        )
        .unwrap();

        let replayed = FluctlightBrain::open(&path).unwrap();
        assert_eq!(replayed.hippocampus.engrams[0].id, assigned_engram_id);
    }
}
