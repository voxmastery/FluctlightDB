use serde::{Deserialize, Serialize};

use crate::brain::FluctlightBrain;
use crate::engram::Engram;
use crate::error::Result;
use crate::life::CoreMemory;
use crate::plasticity::Synapse;

#[derive(Debug, Clone, serde::Serialize)]
pub struct RawExport {
    pub status: Option<crate::brain::BrainStatus>,
    pub engrams: Vec<Engram>,
    pub synapses_total: usize,
    pub synapses_truncated: bool,
    pub synapses: Vec<Synapse>,
    pub core_memories: Vec<CoreMemory>,
    pub recent_separations: Vec<crate::dentate::SeparationResult>,
}

#[derive(Debug, Clone, Deserialize)]
struct RawImportFile {
    engrams: Vec<Engram>,
    synapses_total: usize,
    synapses_truncated: bool,
    synapses: Vec<Synapse>,
    #[serde(default)]
    core_memories: Vec<CoreMemory>,
    #[serde(default)]
    recent_separations: Vec<crate::dentate::SeparationResult>,
    #[serde(default)]
    status: Option<crate::brain::BrainStatus>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RawImportReport {
    pub engrams: usize,
    pub synapses: usize,
    pub core_memories: usize,
    pub synapses_truncated: bool,
    pub synapses_total: usize,
}

pub fn export_raw(brain: &FluctlightBrain) -> RawExport {
    let synapses_total = brain.graph.synapse_count();
    let synapses: Vec<Synapse> = brain.graph.synapses.clone();
    let engrams: Vec<Engram> = brain
        .hippocampus
        .engrams_for_life(brain.life.life_id)
        .cloned()
        .collect();
    RawExport {
        status: Some(brain.status()),
        engrams,
        synapses_total,
        synapses_truncated: false,
        synapses,
        core_memories: brain.core_memories.memories.clone(),
        recent_separations: brain.recent_separations.clone(),
    }
}

pub fn import_raw_json(brain: &mut FluctlightBrain, json: &str) -> Result<RawImportReport> {
    let raw: RawImportFile =
        serde_json::from_str(json).map_err(|e| crate::error::Error::Serde(e.to_string()))?;
    import_raw_parts(
        brain,
        raw.engrams,
        raw.synapses,
        raw.core_memories,
        raw.recent_separations,
        raw.synapses_truncated,
        raw.synapses_total,
        raw.status,
    )
}

pub fn import_raw(brain: &mut FluctlightBrain, raw: RawExport) -> Result<RawImportReport> {
    import_raw_parts(
        brain,
        raw.engrams,
        raw.synapses,
        raw.core_memories,
        raw.recent_separations,
        raw.synapses_truncated,
        raw.synapses_total,
        raw.status,
    )
}

fn import_raw_parts(
    brain: &mut FluctlightBrain,
    engrams: Vec<Engram>,
    synapses: Vec<Synapse>,
    core_memories: Vec<CoreMemory>,
    recent_separations: Vec<crate::dentate::SeparationResult>,
    synapses_truncated: bool,
    synapses_total: usize,
    status: Option<crate::brain::BrainStatus>,
) -> Result<RawImportReport> {
    if let Some(first) = engrams.first() {
        brain.life.life_id = first.life_id;
    }
    for n in &synapses {
        brain.graph.register_neuron(n.from, n.region);
        brain.graph.register_neuron(n.to, n.region);
    }
    brain.hippocampus.engrams = engrams;
    brain.graph.synapses = synapses;
    brain.graph.rebuild_index();
    brain.core_memories.memories = core_memories;
    brain.recent_separations = recent_separations;
    brain.hippocampus.rebuild_rag_index();
    if let Some(st) = status {
        brain.development.metrics.experience_count = st.experiences;
        brain.development.metrics.sleep_cycles = st.sleep_cycles;
        brain.autonomic.auto_sleeps = st.auto_sleeps;
    } else {
        let life_id = brain.life.life_id;
        let count = brain
            .hippocampus
            .engrams
            .iter()
            .filter(|e| e.life_id == life_id)
            .count() as u64;
        brain.development.metrics.experience_count = count;
    }
    for e in &brain.hippocampus.engrams {
        if let Some(ref v) = e.episode.semantic_vector {
            brain.semantic.register_engram(e.id, e.life_id, v.clone());
        }
    }
    Ok(RawImportReport {
        engrams: brain.hippocampus.engrams.len(),
        synapses: brain.graph.synapse_count(),
        core_memories: brain.core_memories.memories.len(),
        synapses_truncated,
        synapses_total,
    })
}
