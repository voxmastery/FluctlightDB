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
    let brain = FluctlightBrain::from_snapshot(
        manifest.wal_seq,
        segment::read_segment(dir, "life")?,
        segment::read_segment(dir, "development")?,
        segment::read_segment(dir, "neuromodulators")?,
        segment::read_segment(dir, "graph")?,
        crate::legacy_hippocampus::read_hippocampus_segment(dir)?,
        segment::read_segment(dir, "cortex")?,
        segment::read_segment(dir, "amygdala")?,
        segment::read_segment(dir, "prefrontal")?,
        segment::read_segment(dir, "core_memories")?,
        segment::read_segment(dir, "autonomic")?,
        segment::read_segment(dir, "recent_separations")?,
        segment::read_segment(dir, "semantic")?,
    );
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
}
