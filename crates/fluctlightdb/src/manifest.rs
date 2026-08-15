//! FLCTLTDB v4 manifest + segmented brain layout.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::brain::FluctlightBrain;
use crate::error::{Error, Result};
use crate::segment;
use crate::store;

const V4_VERSION: u32 = 4;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrainManifest {
    pub format_version: u32,
    pub wal_seq: u64,
    pub wal_checkpoint_seq: u64,
    pub segments: Vec<String>,
    #[serde(default)]
    pub tenant_uuid: Option<uuid::Uuid>,
    #[serde(default)]
    pub writer_epoch: u64,
    #[serde(default)]
    pub fence_generation: u64,
    #[serde(default, rename = "durability_policy")]
    pub durability: Option<crate::placement::DurabilityPolicy>,
}

impl Default for BrainManifest {
    fn default() -> Self {
        Self {
            format_version: V4_VERSION,
            wal_seq: 0,
            wal_checkpoint_seq: 0,
            tenant_uuid: None,
            writer_epoch: 0,
            fence_generation: 0,
            durability: None,
            segments: vec![
                "life".into(),
                "development".into(),
                "neuromodulators".into(),
                "graph".into(),
                "hippocampus".into(),
                "cortex".into(),
                "amygdala".into(),
                "prefrontal".into(),
                "core_memories".into(),
                "autonomic".into(),
                "recent_separations".into(),
                "semantic".into(),
                "muon".into(),
                "tau".into(),
                "swarm".into(),
                "agent".into(),
                "governance".into(),
            ],
        }
    }
}

pub fn manifest_path(dir: &Path) -> PathBuf {
    dir.join("manifest.json")
}

pub fn save_v4_dir(brain: &FluctlightBrain, dir: &Path) -> Result<()> {
    segment::create_private_dir_all(dir)?;
    let generations = dir.join("generations");
    segment::create_private_dir_all(&generations)?;
    let generation_name = next_generation_name(&generations)?;
    let temporary = generations.join(format!(".{generation_name}.tmp-{}", std::process::id()));
    let generation = generations.join(&generation_name);
    if temporary.exists() {
        fs::remove_dir_all(&temporary)?;
    }
    write_checkpoint_dir(brain, &temporary)?;
    segment::sync_parent_dir(&temporary.join("checkpoint.complete"))?;
    crate::checkpoint_fault::hit("generation.before_rename");
    fs::rename(&temporary, &generation)?;
    crate::checkpoint_fault::hit("generation.after_rename");
    segment::sync_parent_dir(&generation)?;
    crate::checkpoint_fault::hit("generations.after_dir_fsync");

    let current = dir.join("CURRENT");
    let current_tmp = dir.join(format!(".CURRENT.tmp-{}", std::process::id()));
    let mut file = segment::create_private_file(&current_tmp)?;
    crate::checkpoint_fault::hit("current.before_write");
    writeln!(file, "{generation_name}")?;
    crate::checkpoint_fault::hit("current.after_write");
    file.sync_all()?;
    crate::checkpoint_fault::hit("current.after_fsync");
    drop(file);
    crate::checkpoint_fault::hit("current.before_rename");
    fs::rename(&current_tmp, &current)?;
    crate::checkpoint_fault::hit("current.after_rename");
    segment::sync_parent_dir(&current)?;
    crate::checkpoint_fault::hit("current.after_dir_fsync");
    Ok(())
}

/// Drop obsolete sealed generations, keeping `keep` newest (always retains CURRENT).
/// Brain-like trace decay after systems consolidation (Somnus).
pub fn prune_old_generations(dir: &Path, keep: usize) -> Result<usize> {
    let keep = keep.max(1);
    let generations = dir.join("generations");
    if !generations.is_dir() {
        return Ok(0);
    }
    let current_name = fs::read_to_string(dir.join("CURRENT"))
        .ok()
        .map(|raw| raw.trim().to_string())
        .filter(|name| generation_number(name).is_some());

    let mut named: Vec<(u64, String)> = Vec::new();
    for entry in fs::read_dir(&generations)? {
        let entry = entry?;
        let name = entry.file_name();
        let Some(name) = name.to_str().map(str::to_string) else {
            continue;
        };
        if let Some(number) = generation_number(&name) {
            named.push((number, name));
        }
    }
    named.sort_by_key(|(number, _)| *number);
    if named.len() <= keep {
        return Ok(0);
    }

    let mut retain: std::collections::HashSet<String> = named
        .iter()
        .rev()
        .take(keep)
        .map(|(_, name)| name.clone())
        .collect();
    if let Some(current) = current_name {
        retain.insert(current);
    }

    let mut removed = 0usize;
    for (_, name) in &named {
        if retain.contains(name) {
            continue;
        }
        let path = generations.join(name);
        if path.is_dir() {
            fs::remove_dir_all(&path)?;
            removed += 1;
        }
    }
    Ok(removed)
}

fn next_generation_name(generations: &Path) -> Result<String> {
    let mut maximum = 0u64;
    for entry in fs::read_dir(generations)? {
        let entry = entry?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if let Some(number) = generation_number(name) {
            maximum = maximum.max(number);
        }
    }
    let next = maximum
        .checked_add(1)
        .ok_or_else(|| Error::Store("v4 generation counter exhausted".into()))?;
    Ok(format!("gen-{next:020}"))
}

fn generation_number(name: &str) -> Option<u64> {
    let suffix = name.strip_prefix("gen-")?;
    if suffix.len() != 20 || !suffix.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    suffix.parse().ok()
}

fn write_checkpoint_dir(brain: &FluctlightBrain, dir: &Path) -> Result<()> {
    segment::create_private_dir_all(dir)?;
    segment::write_segment(dir, "life", &brain.life)?;
    segment::write_segment(dir, "development", &brain.development)?;
    segment::write_segment(dir, "neuromodulators", &brain.neuromodulators)?;
    segment::write_segment(dir, "graph", &brain.graph)?;
    segment::write_segment(dir, "hippocampus", &brain.hippocampus)?;
    segment::write_segment(dir, "cortex", &brain.cortex)?;
    segment::write_segment(dir, "amygdala", &brain.amygdala)?;
    segment::write_segment(dir, "prefrontal", &brain.prefrontal)?;
    segment::write_segment(dir, "core_memories", &brain.core_memories)?;
    segment::write_segment(dir, "autonomic", &brain.autonomic)?;
    segment::write_segment(dir, "recent_separations", &brain.recent_separations)?;
    segment::write_segment(dir, "semantic", &brain.semantic)?;
    // Muon/Tau lanes: persisted so imprints survive a restart. Previously these lived only in
    // process memory, so every restart silently emptied them and recall returned 200 OK with no
    // hits — indistinguishable from "no match" to a client. Both types already derived
    // Serialize/Deserialize; they just were never written.
    segment::write_segment(dir, "muon", &brain.muon)?;
    segment::write_segment(dir, "tau", &brain.tau)?;
    segment::write_segment(dir, "swarm", &brain.swarm)?;
    // Agent + governance: same omission as the muon/tau lanes above, one layer up.
    // `agent` carries the WM ring, the retention policy and the auto-consolidate flag, so
    // unflushed working memory was silently dropped on every restart. `governance` carries
    // the compliance audit log; an audit trail that does not survive a restart is worse than
    // none, because `audit_log()` still returns 200 with an empty list and no client can
    // tell the difference. `load_v4_dir` already reads both — without these writes the
    // reads only ever hit `unwrap_or_default()`, which makes the loss look fixed.
    segment::write_segment(dir, "agent", &brain.agent)?;
    segment::write_segment(dir, "governance", &brain.governance)?;

    let identity = brain.wal_identity();
    let manifest = BrainManifest {
        format_version: V4_VERSION,
        wal_seq: brain.wal_seq,
        wal_checkpoint_seq: brain.wal_seq,
        tenant_uuid: identity.map(|value| value.tenant_uuid),
        writer_epoch: identity.map(|value| value.writer_epoch).unwrap_or_default(),
        fence_generation: identity
            .map(|value| value.fence_generation)
            .unwrap_or_default(),
        durability: identity.map(|value| value.durability),
        ..BrainManifest::default()
    };
    let tmp = manifest_path(dir).with_extension("json.tmp");
    let json = serde_json::to_string_pretty(&manifest).map_err(|e| Error::Serde(e.to_string()))?;
    let mut file = segment::create_private_file(&tmp)?;
    crate::checkpoint_fault::hit("generation.before_file_write");
    file.write_all(json.as_bytes())?;
    crate::checkpoint_fault::hit("generation.after_file_write");
    file.sync_all()?;
    crate::checkpoint_fault::hit("generation.after_file_fsync");
    drop(file);
    fs::rename(tmp, manifest_path(dir))?;
    crate::checkpoint_fault::hit("generation.after_file_rename");
    segment::sync_parent_dir(&manifest_path(dir))?;
    crate::checkpoint_fault::hit("generation.after_file_dir_fsync");
    Ok(())
}

pub fn load_v4_dir(dir: &Path) -> Result<FluctlightBrain> {
    let checkpoint = resolve_checkpoint_dir(dir)?;
    load_checkpoint_dir(&checkpoint)
}

pub fn checkpoint_exists(dir: &Path) -> bool {
    dir.join("CURRENT").exists() || manifest_path(dir).exists()
}

fn resolve_checkpoint_dir(dir: &Path) -> Result<PathBuf> {
    let current = dir.join("CURRENT");
    if !current.exists() {
        return Ok(dir.to_path_buf());
    }
    let raw = fs::read_to_string(&current)?;
    let name = raw.trim();
    if generation_number(name).is_none() {
        return Err(Error::Store(format!(
            "invalid v4 CURRENT generation: {name:?}"
        )));
    }
    let generation = dir.join("generations").join(name);
    if !generation.is_dir() {
        return Err(Error::Store(format!(
            "v4 CURRENT references missing generation {name}"
        )));
    }
    Ok(generation)
}

fn load_checkpoint_dir(dir: &Path) -> Result<FluctlightBrain> {
    if !manifest_path(dir).exists() {
        return Err(Error::Store("missing v4 manifest.json".into()));
    }
    let raw = fs::read_to_string(manifest_path(dir))?;
    let manifest: BrainManifest =
        serde_json::from_str(&raw).map_err(|e| Error::Serde(e.to_string()))?;
    if manifest.format_version != V4_VERSION {
        return Err(Error::Store(format!(
            "unsupported v4 format version {}",
            manifest.format_version
        )));
    }
    let mut brain = FluctlightBrain::from_snapshot(
        manifest.wal_seq,
        crate::life::read_life_segment(dir)?,
        segment::read_segment(dir, "development")?,
        segment::read_segment(dir, "neuromodulators")?,
        segment::read_segment(dir, "graph")?,
        crate::legacy_hippocampus::read_hippocampus_segment(dir)?,
        segment::read_segment(dir, "cortex")?,
        segment::read_segment(dir, "amygdala")?,
        segment::read_segment(dir, "prefrontal").unwrap_or_default(),
        segment::read_segment(dir, "core_memories")?,
        segment::read_segment(dir, "autonomic")?,
        segment::read_segment(dir, "recent_separations")?,
        segment::read_segment(dir, "semantic")?,
    );
    // Lane segments are optional: brains written before lane persistence have no muon/tau
    // segment, so fall back to an empty lane instead of failing the whole load. That keeps
    // older brain directories readable and matches the previous (always-empty) behaviour.
    brain.muon = segment::read_segment(dir, "muon").unwrap_or_default();
    brain.tau = segment::read_segment(dir, "tau").unwrap_or_default();
    brain.swarm = segment::read_segment(dir, "swarm").unwrap_or_default();
    brain.agent = segment::read_segment(dir, "agent").unwrap_or_default();
    brain.governance = segment::read_segment(dir, "governance").unwrap_or_default();
    match (manifest.tenant_uuid, manifest.durability) {
        (Some(tenant_uuid), Some(durability)) => {
            brain.set_wal_identity(Some(crate::wal::WalIdentity {
                tenant_uuid,
                writer_epoch: manifest.writer_epoch,
                fence_generation: manifest.fence_generation,
                durability,
            }));
        }
        (None, None) => {}
        _ => {
            return Err(Error::Store(
                "incomplete fencing identity in v4 manifest".into(),
            ))
        }
    }
    detect_codec_drift(&mut brain);
    Ok(brain)
}

/// Flag every engram for re-keying when stored neuron ids can no longer be reproduced
/// (codec drift) or when the brain is still on the pre-freeze legacy codec.
fn detect_codec_drift(brain: &mut FluctlightBrain) {
    let codec = brain.life.neuron_codec;
    let expected = crate::life::codec_probes_for(codec);
    let drifted = !brain.life.codec_probes.is_empty() && brain.life.codec_probes != expected;
    let legacy = codec != crate::id::CURRENT_CODEC;
    if !drifted && !legacy {
        return;
    }
    let mut pending: Vec<(u64, uuid::Uuid)> = brain
        .hippocampus
        .engrams
        .iter()
        .map(|e| (e.encoded_at_tick, e.id))
        .collect();
    // Oldest first — see the ordering note in `derive::drain`.
    pending.sort_unstable();
    brain.rekey_pending = pending.into_iter().map(|(_, id)| id).collect();
    if drifted {
        eprintln!(
            "fluctlight: neuron codec drift detected ({} engrams queued for re-key) — \
             stored ids no longer match freshly derived cues; run `rekey_now()` or let \
             sleep drain the queue",
            brain.rekey_pending.len()
        );
    }
}

/// Resolve the published generation directory for a v4 brain root (CURRENT pointer).
pub fn active_generation_dir(dir: &Path) -> Result<PathBuf> {
    resolve_checkpoint_dir(dir)
}

pub fn migrate_v3_file_to_v4(v3_path: &Path, v4_dir: &Path) -> Result<()> {
    let brain = store::load_snapshot_only(v3_path)?;
    save_v4_dir(&brain, v4_dir)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::swarm::{BeginSwarm, SwarmTransaction, WorkerSlot, WorkerStatus};
    use crate::types::Episode;
    use tempfile::tempdir;

    fn add_swarm(brain: &mut FluctlightBrain) -> uuid::Uuid {
        let swarm_id = uuid::Uuid::new_v4();
        brain
            .apply_swarm_transaction(SwarmTransaction::Begin(BeginSwarm {
                transaction_id: uuid::Uuid::new_v4(),
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
            }))
            .unwrap();
        swarm_id
    }

    #[test]
    fn v4_roundtrip() {
        let dir = tempdir().unwrap();
        let v4 = dir.path().join("brain_v4");
        let mut brain = FluctlightBrain::new();
        brain
            .experience(Episode {
                content: "v4 segment test".into(),
                context: "test".into(),
                outcome: None,
                salience_hint: 0.5,
                semantic_vector: None,
                agent_id: None,
                tenant_id: None,
                rag: None,
                provenance: None,
            })
            .unwrap();
        save_v4_dir(&brain, &v4).unwrap();
        let loaded = load_v4_dir(&v4).unwrap();
        assert_eq!(loaded.hippocampus.engrams.len(), 1);
    }

    #[test]
    fn v4_checkpoint_publishes_immutable_generation_through_current() {
        let dir = tempdir().unwrap();
        let v4 = dir.path().join("brain_v4");
        save_v4_dir(&FluctlightBrain::new(), &v4).unwrap();

        let current = fs::read_to_string(v4.join("CURRENT")).unwrap();
        let generation = v4.join("generations").join(current.trim());
        assert!(generation.join("manifest.json").is_file());
        assert!(!manifest_path(&v4).exists());
    }

    #[test]
    fn prune_old_generations_keeps_newest_and_current() {
        let dir = tempdir().unwrap();
        let v4 = dir.path().join("brain_v4");
        for _ in 0..5 {
            save_v4_dir(&FluctlightBrain::new(), &v4).unwrap();
        }
        let before = fs::read_dir(v4.join("generations")).unwrap().count();
        assert!(before >= 5);
        let removed = prune_old_generations(&v4, 2).unwrap();
        assert!(removed >= 3);
        let after: Vec<_> = fs::read_dir(v4.join("generations"))
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert!(after.len() <= 2);
        let current = fs::read_to_string(v4.join("CURRENT")).unwrap();
        assert!(after.iter().any(|name| name == current.trim()));
    }

    #[test]
    fn v4_load_ignores_unpublished_generation_after_crash() {
        let dir = tempdir().unwrap();
        let v4 = dir.path().join("brain_v4");
        let mut published = FluctlightBrain::new();
        published
            .experience(Episode {
                content: "published".into(),
                context: "test".into(),
                outcome: None,
                salience_hint: 0.5,
                semantic_vector: None,
                agent_id: None,
                tenant_id: None,
                rag: None,
                provenance: None,
            })
            .unwrap();
        save_v4_dir(&published, &v4).unwrap();

        let unpublished = v4.join("generations").join("gen-00000000000000000002");
        write_checkpoint_dir(&FluctlightBrain::new(), &unpublished).unwrap();

        let loaded = load_v4_dir(&v4).unwrap();
        assert_eq!(loaded.hippocampus.engrams.len(), 1);
    }

    #[test]
    fn v4_loads_legacy_in_place_checkpoint_without_current() {
        let dir = tempdir().unwrap();
        let v4 = dir.path().join("legacy_v4");
        write_checkpoint_dir(&FluctlightBrain::new(), &v4).unwrap();
        assert!(!v4.join("CURRENT").exists());
        load_v4_dir(&v4).unwrap();
    }

    #[test]
    fn v4_rejects_invalid_current_instead_of_falling_back() {
        let dir = tempdir().unwrap();
        let v4 = dir.path().join("brain_v4");
        write_checkpoint_dir(&FluctlightBrain::new(), &v4).unwrap();
        fs::write(v4.join("CURRENT"), "../legacy").unwrap();
        let err = load_v4_dir(&v4).unwrap_err();
        assert!(err.to_string().contains("invalid v4 CURRENT"), "{err}");
    }

    /// Muon/Tau imprints must survive a save/load cycle. Before lane persistence these lanes
    /// lived only in process memory, so a restart silently emptied them and recall returned
    /// success with zero hits — a memory loss no client could detect.
    #[test]
    fn v4_roundtrip_preserves_muon_lane() {
        std::env::set_var("FLUCTLIGHT_MUON", "1");
        let dir = tempdir().unwrap();
        let v4 = dir.path().join("brain_v4");
        let mut brain = FluctlightBrain::new();
        brain.muon_imprint(
            "sess-1",
            "2026-07-21",
            "the quick brown fox",
            "quick brown fox",
        );
        assert_eq!(brain.muon_len(), 1, "imprint should land in the lane");

        save_v4_dir(&brain, &v4).unwrap();
        let loaded = load_v4_dir(&v4).unwrap();

        assert_eq!(
            loaded.muon_len(),
            1,
            "muon imprints must survive save/load, not reset to empty"
        );
        assert!(
            !loaded.muon_recall("quick brown fox", 4).is_empty(),
            "a reloaded imprint must still be recallable"
        );
    }

    #[test]
    fn v4_roundtrip_preserves_swarm_state() {
        let dir = tempdir().unwrap();
        let v4 = dir.path().join("brain_v4");
        let mut brain = FluctlightBrain::new();
        let swarm_id = add_swarm(&mut brain);

        save_v4_dir(&brain, &v4).unwrap();
        let loaded = load_v4_dir(&v4).unwrap();

        assert!(loaded.swarm.runs.contains_key(&swarm_id));
        assert_eq!(loaded.swarm.applied_transactions.len(), 1);
    }

    /// The compliance audit log must survive a restart.
    ///
    /// `load_v4_dir` reads `governance`, but `write_checkpoint_dir` did not write it — so the
    /// read only ever hit `unwrap_or_default()` and every restart silently emptied the audit
    /// trail while `audit_log()` kept returning 200 with an empty list.
    #[test]
    fn v4_roundtrip_preserves_governance_audit_log() {
        let dir = tempdir().unwrap();
        let v4 = dir.path().join("brain_v4");
        let mut brain = FluctlightBrain::new();
        brain
            .experience(Episode::new(
                "reach bob@example.com for the invoice",
                "test",
                0.6,
            ))
            .unwrap();
        let scrub = brain.scrub_pii().unwrap();
        assert_eq!(
            scrub.engrams_scrubbed, 1,
            "precondition: something was scrubbed"
        );
        let before = brain.governance_state().audit_log.len();
        assert!(before > 0, "precondition: an audit entry was recorded");

        save_v4_dir(&brain, &v4).unwrap();
        let loaded = load_v4_dir(&v4).unwrap();
        assert_eq!(
            loaded.governance_state().audit_log.len(),
            before,
            "compliance audit entries must survive a restart"
        );
    }

    /// Working memory that has not yet been flushed must survive a checkpoint. `agent`
    /// (AgentState) holds the WM ring, retention policy and auto-consolidate flag.
    #[test]
    fn v4_roundtrip_preserves_agent_state() {
        let dir = tempdir().unwrap();
        let v4 = dir.path().join("brain_v4");
        let mut brain = FluctlightBrain::new();
        brain.turn_begin();
        brain.wm_push("user prefers dark mode", "settings", 0.8, None);
        let before = brain.wm_len();
        assert!(before > 0, "precondition: WM holds a slot");

        save_v4_dir(&brain, &v4).unwrap();
        let loaded = load_v4_dir(&v4).unwrap();
        assert_eq!(
            loaded.wm_len(),
            before,
            "unflushed working memory must survive a checkpoint"
        );
    }

    /// Guard against the whole bug class: every segment the manifest *declares* must actually
    /// be written to the published generation.
    ///
    /// This is the check that would have caught `agent`/`governance` being dropped. Adding a
    /// persistable field to `FluctlightBrain` and listing it in `BrainManifest::default()`
    /// without a matching `write_segment` call now fails loudly here, instead of silently
    /// discarding that state at the next restart.
    #[test]
    fn every_declared_segment_is_actually_written() {
        let dir = tempdir().unwrap();
        let v4 = dir.path().join("brain_v4");
        save_v4_dir(&FluctlightBrain::new(), &v4).unwrap();
        let checkpoint = resolve_checkpoint_dir(&v4).unwrap();

        let declared = BrainManifest::default().segments;
        let missing: Vec<&str> = declared
            .iter()
            .filter(|name| !crate::segment::segment_exists(&checkpoint, name))
            .map(|s| s.as_str())
            .collect();
        assert!(
            missing.is_empty(),
            "manifest declares segments that the checkpoint never writes: {missing:?}"
        );
    }

    /// A brain directory written before lane persistence has no muon/tau segment. Loading it
    /// must still succeed (falling back to empty lanes) rather than erroring out.
    #[test]
    fn v4_load_tolerates_missing_lane_segments() {
        let dir = tempdir().unwrap();
        let v4 = dir.path().join("brain_v4");
        let brain = FluctlightBrain::new();
        save_v4_dir(&brain, &v4).unwrap();
        // simulate an older brain dir: drop the lane segments
        let checkpoint = resolve_checkpoint_dir(&v4).unwrap();
        let _ = fs::remove_file(checkpoint.join("muon.seg"));
        let _ = fs::remove_file(checkpoint.join("tau.seg"));
        let _ = fs::remove_file(checkpoint.join("swarm.seg"));

        let loaded = load_v4_dir(&v4).expect("older brain dirs must still load");
        assert_eq!(loaded.muon_len(), 0);
        assert!(loaded.swarm.runs.is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn v4_checkpoint_files_are_owner_only() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempdir().unwrap();
        let v4 = dir.path().join("brain_v4");
        save_v4_dir(&FluctlightBrain::new(), &v4).unwrap();
        let checkpoint = resolve_checkpoint_dir(&v4).unwrap();
        for path in [
            manifest_path(&checkpoint),
            segment::segment_path(&checkpoint, "life"),
        ] {
            let mode = fs::metadata(path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }
    }

    #[test]
    fn v4_manifest_roundtrips_tenant_fence_and_durability_policy() {
        let dir = tempdir().unwrap();
        let v4 = dir.path().join("fenced_v4");
        let identity = crate::wal::WalIdentity {
            tenant_uuid: uuid::Uuid::from_u128(77),
            writer_epoch: 3,
            fence_generation: 12,
            durability: crate::placement::DurabilityPolicy::All,
        };
        let mut brain = FluctlightBrain::new();
        brain.set_wal_identity(Some(identity));
        save_v4_dir(&brain, &v4).unwrap();

        let checkpoint = resolve_checkpoint_dir(&v4).unwrap();
        let value: serde_json::Value =
            serde_json::from_slice(&fs::read(manifest_path(&checkpoint)).unwrap()).unwrap();
        assert_eq!(value["tenant_uuid"], identity.tenant_uuid.to_string());
        assert_eq!(value["writer_epoch"], 3);
        assert_eq!(value["fence_generation"], 12);
        assert_eq!(value["durability_policy"], "All");

        let loaded = load_v4_dir(&v4).unwrap();
        assert_eq!(loaded.wal_identity(), Some(identity));
    }
}
