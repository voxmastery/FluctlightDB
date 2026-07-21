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
        segment::read_segment(dir, "life")?,
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
    Ok(brain)
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
        brain.muon_imprint("sess-1", "2026-07-21", "the quick brown fox", "quick brown fox");
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
