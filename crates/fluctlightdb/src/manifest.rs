//! FLCTLTDB v4 manifest + segmented brain layout.

use std::fs;
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
}

impl Default for BrainManifest {
    fn default() -> Self {
        Self {
            format_version: V4_VERSION,
            wal_seq: 0,
            wal_checkpoint_seq: 0,
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
    fs::create_dir_all(dir)?;
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
    // Agent + governance: same omission as the muon/tau lanes above, one layer up.
    // `agent` carries the WM ring, the retention policy and the auto-consolidate flag —
    // `connect_embedded()` sets the latter two on every open, so their loss was masked,
    // but unflushed working memory was silently dropped on every restart.
    // `governance` carries the compliance audit log; an audit trail that does not survive
    // a restart is worse than none, because `audit_log()` still returns 200 with an
    // empty list and no client can tell the difference.
    segment::write_segment(dir, "agent", &brain.agent)?;
    segment::write_segment(dir, "governance", &brain.governance)?;

    let manifest = BrainManifest {
        format_version: V4_VERSION,
        wal_seq: brain.wal_seq,
        wal_checkpoint_seq: brain.wal_seq,
        ..BrainManifest::default()
    };
    let tmp = manifest_path(dir).with_extension("json.tmp");
    let json = serde_json::to_string_pretty(&manifest).map_err(|e| Error::Serde(e.to_string()))?;
    fs::write(&tmp, json)?;
    fs::rename(tmp, manifest_path(dir))?;
    Ok(())
}

pub fn load_v4_dir(dir: &Path) -> Result<FluctlightBrain> {
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
    brain.agent = segment::read_segment(dir, "agent").unwrap_or_default();
    brain.governance = segment::read_segment(dir, "governance").unwrap_or_default();
    detect_codec_drift(&mut brain);
    Ok(brain)
}

/// Flag every engram for re-keying when this brain's stored neuron ids can no longer be
/// reproduced by the running binary.
///
/// Two triggers, both cheap and both checked on every open:
///
/// 1. **Codec drift.** The brain recorded known-answer probes under its own codec. If
///    recomputing them now yields something different, the identity function moved
///    underneath stored data — the exact silent-total-recall-loss failure that motivated
///    freezing the codec. Historically this produced no error, no crash and no log line;
///    recall simply returned empty forever.
/// 2. **Legacy codec.** A brain written before the freeze is on `CODEC_LEGACY_STD`. It still
///    recalls correctly (its ids and its cues agree), so this is not urgent — but it is
///    still riding an unstable hash, so it is queued for migration to FLCT1.
///
/// Detection is a probe comparison, not a digest, a manifest field, or a WAL entry.
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

pub fn migrate_v3_file_to_v4(v3_path: &Path, v4_dir: &Path) -> Result<()> {
    let brain = store::load_snapshot_only(v3_path)?;
    save_v4_dir(&brain, v4_dir)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Episode;
    use tempfile::tempdir;

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

    /// The compliance audit log must survive a restart. `governance` was in neither the
    /// write list nor the read list, so `scrub_pii` / `delete_by_subject` / `forget_before`
    /// recorded entries that vanished at the next open while `audit_log()` kept returning
    /// 200 with an empty list — the same undetectable-loss shape as the muon/tau bug.
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

    /// Working memory that has not yet been flushed to the hippocampus must survive a
    /// checkpoint. `agent` (AgentState) holds the WM ring, retention policy and the
    /// auto-consolidate flag, and was never written to disk.
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

    /// Guard against the whole bug class: every segment the manifest claims to persist must
    /// actually be written to disk. `BrainManifest::default().segments` is the declared
    /// contract; this asserts the writer honours it. Adding a field to `FluctlightBrain`
    /// and listing it here without a `write_segment` call now fails loudly instead of
    /// silently dropping that state at the next restart.
    #[test]
    fn every_declared_segment_is_actually_written() {
        let dir = tempdir().unwrap();
        let v4 = dir.path().join("brain_v4");
        let brain = FluctlightBrain::new();
        save_v4_dir(&brain, &v4).unwrap();

        let declared = BrainManifest::default().segments;
        let missing: Vec<&str> = declared
            .iter()
            .filter(|name| !crate::segment::segment_exists(&v4, name))
            .map(|s| s.as_str())
            .collect();
        assert!(
            missing.is_empty(),
            "manifest declares segments that save_v4_dir never writes: {missing:?}"
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
        let _ = fs::remove_file(v4.join("muon.seg"));
        let _ = fs::remove_file(v4.join("tau.seg"));

        let loaded = load_v4_dir(&v4).expect("older brain dirs must still load");
        assert_eq!(loaded.muon_len(), 0);
    }
}
