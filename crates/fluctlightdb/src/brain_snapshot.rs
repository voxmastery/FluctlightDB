//! Open Brain Snapshot interchange format — portable export/import between frameworks.

use serde::{Deserialize, Serialize};

use crate::agent_runtime::AgentState;
use crate::brain::FluctlightBrain;
use crate::chronos::Chronos;
use crate::engram::Engram;
use crate::error::Result;
use crate::retention_policy::RetentionPolicy;

pub const SNAPSHOT_FORMAT: &str = "fluctlight-brain-snapshot";
pub const SNAPSHOT_VERSION: u8 = 1;

/// Portable brain snapshot for framework interop (LangChain, Mem0 import, backups).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrainSnapshot {
    pub format: String,
    pub version: u8,
    pub exported_at_tick: u64,
    pub engrams: Vec<Engram>,
    #[serde(default)]
    pub chronos: Chronos,
    #[serde(default)]
    pub agent_state: Option<AgentState>,
    #[serde(default)]
    pub retention_policy: Option<RetentionPolicy>,
    #[serde(default)]
    pub notes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SnapshotImportReport {
    pub engrams_imported: usize,
    pub chronos_events: usize,
    pub skipped_duplicates: usize,
}

impl BrainSnapshot {
    pub fn from_brain(brain: &FluctlightBrain) -> Self {
        Self {
            format: SNAPSHOT_FORMAT.into(),
            version: SNAPSHOT_VERSION,
            exported_at_tick: brain.autonomic.total_ticks,
            engrams: brain.hippocampus.engrams.clone(),
            chronos: brain.chronos.clone(),
            agent_state: Some(brain.agent.clone()),
            retention_policy: Some(brain.agent.retention.policy.clone()),
            notes: "FluctlightDB open brain snapshot v1 — https://github.com/voxmastery/FluctlightDB/blob/main/docs/BRAIN_SNAPSHOT.md".into(),
        }
    }
}

pub fn export_snapshot_json(brain: &FluctlightBrain) -> Result<String> {
    let snap = BrainSnapshot::from_brain(brain);
    serde_json::to_string_pretty(&snap).map_err(|e| crate::error::Error::Serde(e.to_string()))
}

pub fn import_snapshot_json(
    brain: &mut FluctlightBrain,
    json: &str,
) -> Result<SnapshotImportReport> {
    brain.reject_distributed_mutation("brain_snapshot::import_snapshot_json")?;
    let snap: BrainSnapshot =
        serde_json::from_str(json).map_err(|e| crate::error::Error::Serde(e.to_string()))?;
    import_snapshot(brain, &snap)
}

pub fn import_snapshot(
    brain: &mut FluctlightBrain,
    snap: &BrainSnapshot,
) -> Result<SnapshotImportReport> {
    brain.reject_distributed_mutation("brain_snapshot::import_snapshot")?;
    if snap.format != SNAPSHOT_FORMAT {
        return Err(crate::error::Error::Serde(format!(
            "unknown snapshot format: {}",
            snap.format
        )));
    }
    if snap.version > SNAPSHOT_VERSION {
        return Err(crate::error::Error::Serde(format!(
            "unsupported snapshot version {}",
            snap.version
        )));
    }
    let mut report = SnapshotImportReport::default();
    let existing: std::collections::HashSet<_> =
        brain.hippocampus.engrams.iter().map(|e| e.id).collect();
    for e in &snap.engrams {
        if existing.contains(&e.id) {
            report.skipped_duplicates += 1;
            continue;
        }
        brain.hippocampus.engrams.push(e.clone());
        report.engrams_imported += 1;
    }
    brain.chronos = snap.chronos.clone();
    report.chronos_events = brain.chronos.len();
    if let Some(agent) = &snap.agent_state {
        brain.agent = agent.clone();
    }
    if let Some(policy) = &snap.retention_policy {
        brain.agent.retention.set_policy(policy.clone());
    }
    brain.invalidate_activation_cache();
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Episode;

    #[test]
    fn roundtrip_snapshot() {
        let mut brain = FluctlightBrain::new();
        brain
            .experience(Episode::new("hello snapshot", "test", 0.7))
            .unwrap();
        let json = export_snapshot_json(&brain).unwrap();
        let mut brain2 = FluctlightBrain::new();
        let r = import_snapshot_json(&mut brain2, &json).unwrap();
        assert_eq!(r.engrams_imported, 1);
        assert_eq!(brain2.hippocampus.engrams.len(), 1);
    }
}
