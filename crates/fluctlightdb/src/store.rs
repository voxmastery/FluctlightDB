use std::fs;
use std::io::Write;
use std::path::Path;
use std::time::SystemTime;

use crate::brain::FluctlightBrain;
use crate::error::{Error, Result};
use crate::semantic::SemanticField;
use crate::storage::{self, StorageFormat};
use crate::store_lock::{SharedStoreLock, StoreLock};
use crate::wal;

const MAGIC: &[u8; 8] = b"FLCTLTDB";
const VERSION: u32 = 3;
/// v3.1 header: magic(8) + version(4) + payload_len(8) + crc(4) + reserved(4)
const HEADER_LEN: usize = 28;
const LEGACY_HEADER_LEN: usize = 12;

fn crc32_ieee(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        crc ^= u32::from(b);
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

pub fn snapshot_mtime(path: &Path) -> Option<SystemTime> {
    fs::metadata(storage::snapshot_path(path))
        .ok()
        .and_then(|m| m.modified().ok())
}

pub fn save(brain: &FluctlightBrain, path: &Path) -> Result<()> {
    let _lock = StoreLock::acquire(path).map_err(Error::Io)?;
    save_atomic(brain, path)?;
    wal::truncate(path)?;
    wal::flush(path)?;
    Ok(())
}

pub fn save_atomic(brain: &FluctlightBrain, path: &Path) -> Result<()> {
    if storage::should_use_v4(path) {
        return save_v4_atomic(brain, path);
    }
    save_v3_atomic(brain, path)
}

fn save_v4_atomic(brain: &FluctlightBrain, path: &Path) -> Result<()> {
    if save_backup_enabled() && path.exists() && path.is_dir() {
        let bak = path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(format!(
                "{}.brain.bak",
                path.file_name().and_then(|s| s.to_str()).unwrap_or("brain")
            ));
        let _ = copy_dir(path, &bak);
    }
    crate::manifest::save_v4_dir(brain, path)?;
    if save_verify_enabled() {
        let loaded = crate::manifest::load_v4_dir(path)?;
        if loaded.hippocampus.engrams.len() != brain.hippocampus.engrams.len()
            || loaded.graph.synapse_count() != brain.graph.synapse_count()
        {
            return Err(Error::Store("v4 pre-commit verify failed".into()));
        }
    }
    Ok(())
}

fn env_flag(name: &str) -> bool {
    std::env::var(name)
        .ok()
        .map(|v| matches!(v.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
        .unwrap_or(false)
}

fn save_verify_enabled() -> bool {
    env_flag("FLUCTLIGHT_SAVE_VERIFY")
}

fn save_backup_enabled() -> bool {
    env_flag("FLUCTLIGHT_SAVE_BACKUP")
}

fn copy_dir(src: &Path, dst: &Path) -> std::io::Result<()> {
    if dst.exists() {
        fs::remove_dir_all(dst)?;
    }
    fn rec(src: &Path, dst: &Path) -> std::io::Result<()> {
        fs::create_dir_all(dst)?;
        for entry in fs::read_dir(src)? {
            let entry = entry?;
            let ty = entry.file_type()?;
            let to = dst.join(entry.file_name());
            if ty.is_dir() {
                rec(&entry.path(), &to)?;
            } else {
                fs::copy(entry.path(), to)?;
            }
        }
        Ok(())
    }
    rec(src, dst)
}

fn save_v3_atomic(brain: &FluctlightBrain, path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    if path.exists() && path.is_file() {
        let bak = path.with_extension("flct.bak");
        let _ = fs::copy(path, &bak);
    }
    let payload = BrainSnapshot::from_brain(brain);
    let encoded = bincode::serialize(&payload).map_err(|e| Error::Store(e.to_string()))?;
    verify_encoded(&encoded)?;
    let crc = crc32_ieee(&encoded);
    let payload_len = encoded.len() as u64;
    let mut out = Vec::with_capacity(HEADER_LEN + encoded.len());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&VERSION.to_le_bytes());
    out.extend_from_slice(&payload_len.to_le_bytes());
    out.extend_from_slice(&crc.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&encoded);
    let tmp = path.with_extension("flct.tmp");
    {
        let mut file = fs::File::create(&tmp)?;
        file.write_all(&out)?;
        file.sync_all()?;
    }
    // Verify roundtrip before publish.
    let verify = fs::read(&tmp)?;
    let loaded = decode_snapshot_bytes(&verify)?;
    if loaded.hippocampus.engrams.len() != brain.hippocampus.engrams.len()
        || loaded.graph.synapse_count() != brain.graph.synapse_count()
    {
        let _ = fs::remove_file(&tmp);
        return Err(Error::Store("pre-commit snapshot verify failed".into()));
    }
    fs::rename(tmp, path)?;
    Ok(())
}

fn verify_encoded(encoded: &[u8]) -> Result<()> {
    let snap: BrainSnapshot =
        bincode::deserialize(encoded).map_err(|e| Error::Serde(e.to_string()))?;
    let _ = snap;
    Ok(())
}

fn decode_snapshot_bytes(raw: &[u8]) -> Result<FluctlightBrain> {
    if raw.len() < LEGACY_HEADER_LEN || &raw[..8] != MAGIC {
        return Err(Error::Store("invalid fluctlightdb file".into()));
    }
    let version = u32::from_le_bytes(raw[8..12].try_into().unwrap());
    match version {
        2 => {
            let v2: legacy_v2::BrainSnapshotV2 =
                bincode::deserialize(&raw[12..]).map_err(|e| Error::Serde(e.to_string()))?;
            return Ok(v2.upgrade().into_brain());
        }
        1 => {
            let old: BrainSnapshotV1 =
                bincode::deserialize(&raw[12..]).map_err(|e| Error::Serde(e.to_string()))?;
            return Ok(old.upgrade().into_brain());
        }
        3 => {}
        _ => return Err(Error::Store(format!("unsupported version {version}"))),
    }
    let payload = locate_v3_payload(raw)?;
    try_deserialize_snapshot(payload)
}

fn locate_v3_payload(raw: &[u8]) -> Result<&[u8]> {
    // v3.1: explicit payload length at offset 12
    if raw.len() >= HEADER_LEN {
        let declared_len = u64::from_le_bytes(raw[12..20].try_into().unwrap()) as usize;
        let stored_crc = u32::from_le_bytes(raw[20..24].try_into().unwrap());
        let payload = &raw[HEADER_LEN..];
        if declared_len == payload.len() && stored_crc == crc32_ieee(payload) {
            return Ok(payload);
        }
    }
    // Deprecated 20-byte header (crc + reserved, no payload_len)
    if raw.len() >= 20 {
        let stored_crc = u32::from_le_bytes(raw[12..16].try_into().unwrap());
        let reserved = u32::from_le_bytes(raw[16..20].try_into().unwrap());
        let payload = &raw[20..];
        if reserved == 0 && stored_crc != 0 && stored_crc == crc32_ieee(payload) {
            return Ok(payload);
        }
        // Corrupted migration: crc slot holds wal_seq (small integer), payload intact at 20
        if stored_crc < 10_000 && payload.len() > 32 {
            if let Ok(b) = try_deserialize_snapshot(payload) {
                eprintln!(
                    "fluctlightdb: recovered snapshot with damaged header (crc slot={stored_crc})"
                );
                let _ = b;
                return Ok(payload);
            }
        }
    }
    // Legacy v3: payload immediately after 12-byte header
    let legacy = &raw[LEGACY_HEADER_LEN..];
    if try_deserialize_snapshot(legacy).is_ok() {
        return Ok(legacy);
    }
    Err(Error::Store(
        "unable to locate valid v3 snapshot payload — restore from .flct.bak or import-raw".into(),
    ))
}

fn load_snapshot(path: &Path) -> Result<FluctlightBrain> {
    if storage::is_v4_path(path) {
        if crate::manifest::manifest_path(path).exists() {
            return crate::manifest::load_v4_dir(path);
        }
        return Ok(FluctlightBrain::new());
    }
    if !path.exists() && storage::should_use_v4(path) {
        return Ok(FluctlightBrain::new());
    }
    let raw = fs::read(path)?;
    decode_snapshot_bytes(&raw)
}

fn try_deserialize_snapshot(payload_bytes: &[u8]) -> Result<FluctlightBrain> {
    let snapshot: BrainSnapshot =
        bincode::deserialize(payload_bytes).map_err(|e| Error::Serde(e.to_string()))?;
    Ok(snapshot.into_brain())
}

pub fn verify_path(path: &Path) -> Result<BrainVerifyReport> {
    if storage::is_v4_path(path) {
        let size = dir_size(path).unwrap_or(0);
        let mut report = BrainVerifyReport {
            path: path.display().to_string(),
            size_bytes: size,
            ok: false,
            format: "v4".into(),
            engrams: 0,
            synapses: 0,
            error: None,
        };
        match crate::manifest::load_v4_dir(path) {
            Ok(brain) => {
                report.ok = true;
                report.engrams = brain.hippocampus.engrams.len();
                report.synapses = brain.graph.synapse_count();
            }
            Err(e) => report.error = Some(e.to_string()),
        }
        return Ok(report);
    }
    let raw = fs::read(path)?;
    let size = raw.len();
    let mut report = BrainVerifyReport {
        path: path.display().to_string(),
        size_bytes: size,
        ok: false,
        format: "unknown".into(),
        engrams: 0,
        synapses: 0,
        error: None,
    };
    match decode_snapshot_bytes(&raw) {
        Ok(brain) => {
            report.ok = true;
            report.engrams = brain.hippocampus.engrams.len();
            report.synapses = brain.graph.synapse_count();
            report.format = if raw.len() >= HEADER_LEN {
                let declared = u64::from_le_bytes(raw[12..20].try_into().unwrap()) as usize;
                if declared + HEADER_LEN == raw.len() {
                    "v3.1".into()
                } else {
                    "v3-legacy".into()
                }
            } else {
                "v3-legacy".into()
            };
        }
        Err(e) => report.error = Some(e.to_string()),
    }
    Ok(report)
}

fn dir_size(path: &Path) -> Result<usize> {
    let mut total = 0usize;
    if path.is_dir() {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let meta = entry.metadata()?;
            if meta.is_file() {
                total += meta.len() as usize;
            } else if meta.is_dir() {
                total += dir_size(&entry.path())?;
            }
        }
    }
    Ok(total)
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct BrainVerifyReport {
    pub path: String,
    pub size_bytes: usize,
    pub ok: bool,
    pub format: String,
    pub engrams: usize,
    pub synapses: usize,
    pub error: Option<String>,
}

pub(crate) fn load_snapshot_only(path: &Path) -> Result<FluctlightBrain> {
    load_snapshot(path)
}

fn load_inner(path: &Path, persist_wal_replay: bool) -> Result<FluctlightBrain> {
    let mut brain = if path.exists() {
        load_snapshot(path)?
    } else {
        FluctlightBrain::new()
    };
    brain.attach_store_path(path.to_path_buf());
    let replayed = {
        let seq = brain.wal_seq;
        wal::replay_with_corruption_skip(&mut brain, path, seq)?
    };
    if replayed.0 > 0 {
        brain.wal_seq += replayed.0;
        if persist_wal_replay {
            save_atomic(&brain, path)?;
            wal::truncate(path)?;
        }
    }
    Ok(brain)
}

pub fn load(path: &Path) -> Result<FluctlightBrain> {
    let _lock = StoreLock::acquire(path).map_err(Error::Io)?;
    load_inner(path, true)
}

/// Read-only open — shared flock; replays WAL in memory but does not checkpoint.
pub fn load_readonly(path: &Path) -> Result<FluctlightBrain> {
    let _lock = SharedStoreLock::acquire(path).map_err(Error::Io)?;
    load_inner(path, false)
}

#[derive(serde::Serialize, serde::Deserialize)]
struct BrainSnapshot {
    #[serde(default)]
    wal_seq: u64,
    life: crate::life::LifeState,
    development: crate::development::DevelopmentState,
    neuromodulators: crate::neuromodulator::Neuromodulators,
    graph: crate::graph::BrainGraph,
    hippocampus: crate::hippocampus::Hippocampus,
    cortex: crate::cortex::Cortex,
    amygdala: crate::amygdala::Amygdala,
    prefrontal: crate::prefrontal::Prefrontal,
    core_memories: crate::life::CoreMemoryStore,
    autonomic: crate::autonomic::AutonomicState,
    #[serde(default)]
    recent_separations: Vec<crate::dentate::SeparationResult>,
    #[serde(default)]
    semantic: SemanticField,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct BrainSnapshotV1 {
    life: crate::life::LifeState,
    development: crate::development::DevelopmentState,
    neuromodulators: crate::neuromodulator::Neuromodulators,
    graph: crate::graph::BrainGraph,
    hippocampus: crate::hippocampus::Hippocampus,
    cortex: crate::cortex::Cortex,
    amygdala: crate::amygdala::Amygdala,
    prefrontal: crate::prefrontal::Prefrontal,
    core_memories: crate::life::CoreMemoryStore,
}

impl BrainSnapshotV1 {
    fn upgrade(self) -> BrainSnapshot {
        BrainSnapshot {
            wal_seq: 0,
            life: self.life,
            development: self.development,
            neuromodulators: self.neuromodulators,
            graph: self.graph,
            hippocampus: self.hippocampus,
            cortex: self.cortex,
            amygdala: self.amygdala,
            prefrontal: self.prefrontal,
            core_memories: self.core_memories,
            autonomic: crate::autonomic::AutonomicState::new(),
            recent_separations: Vec::new(),
            semantic: SemanticField::default(),
        }
    }
}

/// Frozen v2 bincode layout — must match bytes on disk before v3 (no trailing field drift).
mod legacy_v2 {
    use std::collections::HashMap;

    use serde::{Deserialize, Serialize};

    use crate::amygdala::Amygdala;
    use crate::autonomic::{AutonomicConfig, AutonomicState};
    use crate::cortex::Cortex;
    use crate::dentate::SeparationResult;
    use crate::development::DevelopmentState;
    use crate::engram::Engram;
    use crate::graph::BrainGraph;
    use crate::hippocampus::Hippocampus;
    use crate::id::NeuronId;
    use crate::life::{CoreMemoryStore, LifeState};
    use crate::neuromodulator::Neuromodulators;
    use crate::prefrontal::Prefrontal;
    use crate::semantic::SemanticField;
    use crate::store::BrainSnapshot;
    use crate::types::Episode;

    #[derive(Serialize, Deserialize)]
    struct EpisodeV2 {
        content: String,
        context: String,
        outcome: Option<String>,
        salience_hint: f32,
    }

    #[derive(Serialize, Deserialize)]
    struct EngramV2 {
        id: uuid::Uuid,
        life_id: uuid::Uuid,
        neurons: Vec<NeuronId>,
        #[serde(default)]
        ec_neurons: Vec<NeuronId>,
        #[serde(default)]
        dg_neurons: Vec<NeuronId>,
        #[serde(default)]
        separation_index: f32,
        episode: EpisodeV2,
        salience: f32,
        encoded_at_tick: u64,
        encoded_at_stage: u8,
        replay_count: u32,
        is_core: bool,
    }

    #[derive(Serialize, Deserialize)]
    struct HippocampusV2 {
        engrams: Vec<EngramV2>,
    }

    #[derive(Serialize, Deserialize)]
    struct CortexV2 {
        facts: HashMap<String, f32>,
        token_strength: HashMap<NeuronId, f32>,
    }

    #[derive(Serialize, Deserialize)]
    struct AutonomicConfigV2 {
        auto_sleep: bool,
        ticks_per_sleep: u64,
        synapse_pressure_ratio: f32,
    }

    #[derive(Serialize, Deserialize)]
    struct AutonomicStateV2 {
        config: AutonomicConfigV2,
        ticks_since_sleep: u64,
        total_ticks: u64,
        auto_sleeps: u64,
    }

    #[derive(Serialize, Deserialize)]
    pub struct BrainSnapshotV2 {
        life: LifeState,
        development: DevelopmentState,
        neuromodulators: Neuromodulators,
        graph: BrainGraph,
        hippocampus: HippocampusV2,
        cortex: CortexV2,
        amygdala: Amygdala,
        prefrontal: Prefrontal,
        core_memories: CoreMemoryStore,
        autonomic: AutonomicStateV2,
        recent_separations: Vec<SeparationResult>,
    }

    impl BrainSnapshotV2 {
        pub fn upgrade(self) -> BrainSnapshot {
            let mut hippocampus = Hippocampus {
                engrams: self
                    .hippocampus
                    .engrams
                    .into_iter()
                    .map(|e| Engram {
                        id: e.id,
                        life_id: e.life_id,
                        neurons: e.neurons,
                        ec_neurons: e.ec_neurons,
                        dg_neurons: e.dg_neurons,
                        separation_index: e.separation_index,
                        episode: Episode {
                            content: e.episode.content,
                            context: e.episode.context,
                            outcome: e.episode.outcome,
                            salience_hint: e.episode.salience_hint,
                            semantic_vector: None,
                            agent_id: None,
                            tenant_id: None,
                            rag: None,
                            provenance: None,
                        },
                        salience: e.salience,
                        encoded_at_tick: e.encoded_at_tick,
                        encoded_at_stage: e.encoded_at_stage,
                        replay_count: e.replay_count,
                        is_core: e.is_core,
                    })
                    .collect(),
                rag_index: std::collections::HashMap::new(),
            };
            hippocampus.rebuild_rag_index();
            let cortex = Cortex {
                facts: self.cortex.facts,
                token_strength: self.cortex.token_strength,
                semantic_centroid: Vec::new(),
                semantic_strength: 0.0,
            };
            let autonomic = AutonomicState {
                config: AutonomicConfig {
                    auto_sleep: self.autonomic.config.auto_sleep,
                    ticks_per_sleep: self.autonomic.config.ticks_per_sleep,
                    synapse_pressure_ratio: self.autonomic.config.synapse_pressure_ratio,
                    max_auto_sleeps_per_hour: 6,
                    sleep_window_ticks: 3600,
                },
                ticks_since_sleep: self.autonomic.ticks_since_sleep,
                total_ticks: self.autonomic.total_ticks,
                auto_sleeps: self.autonomic.auto_sleeps,
                sleeps_in_window: 0,
                window_start_tick: self.autonomic.total_ticks,
            };
            BrainSnapshot {
                wal_seq: 0,
                life: self.life,
                development: self.development,
                neuromodulators: self.neuromodulators,
                graph: self.graph,
                hippocampus,
                cortex,
                amygdala: self.amygdala,
                prefrontal: self.prefrontal,
                core_memories: self.core_memories,
                autonomic,
                recent_separations: self.recent_separations,
                semantic: SemanticField::default(),
            }
        }
    }
}

impl BrainSnapshot {
    fn from_brain(b: &FluctlightBrain) -> Self {
        Self {
            wal_seq: b.wal_seq,
            life: b.life.clone(),
            development: b.development.clone(),
            neuromodulators: b.neuromodulators.clone(),
            graph: b.graph.clone(),
            hippocampus: b.hippocampus.clone(),
            cortex: b.cortex.clone(),
            amygdala: b.amygdala.clone(),
            prefrontal: b.prefrontal.clone(),
            core_memories: b.core_memories.clone(),
            autonomic: b.autonomic.clone(),
            recent_separations: b.recent_separations.clone(),
            semantic: b.semantic.clone(),
        }
    }

    fn into_brain(self) -> FluctlightBrain {
        FluctlightBrain::from_snapshot(
            self.wal_seq,
            self.life,
            self.development,
            self.neuromodulators,
            self.graph,
            self.hippocampus,
            self.cortex,
            self.amygdala,
            self.prefrontal,
            self.core_memories,
            self.autonomic,
            self.recent_separations,
            self.semantic,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Episode;
    use tempfile::tempdir;

    #[test]
    fn persistence_roundtrip_v31_header() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("roundtrip.flct");
        let mut brain = FluctlightBrain::open(&path).unwrap();
        brain
            .experience(Episode {
                content: "persist me".into(),
                context: "test".into(),
                outcome: None,
                salience_hint: 0.6,
                semantic_vector: Some(vec![0.1, 0.2, 0.3]),
                agent_id: None,
                tenant_id: None,
                rag: None,
                provenance: None,
            })
            .unwrap();
        drop(brain);

        let loaded = FluctlightBrain::open(&path).unwrap();
        assert_eq!(loaded.hippocampus.engrams.len(), 1);
        let report = verify_path(&path).unwrap();
        assert!(report.ok);
        assert_eq!(report.format, "v3.1");
    }

    #[test]
    fn verify_rejects_garbage() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("bad.flct");
        fs::write(&path, b"not a brain").unwrap();
        let report = verify_path(&path).unwrap();
        assert!(!report.ok);
    }
}
