//! Backward-compatible hippocampus segment load (pre-provenance Episode layout).

use uuid::Uuid;

use crate::engram::Engram;
use crate::error::{Error, Result};
use crate::hippocampus::Hippocampus;
use crate::id::NeuronId;
use crate::types::{Episode, RagRef};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct EpisodePreProvenance {
    pub content: String,
    pub context: String,
    pub outcome: Option<String>,
    pub salience_hint: f32,
    #[serde(default)]
    pub semantic_vector: Option<Vec<f32>>,
    #[serde(default)]
    pub agent_id: Option<String>,
    #[serde(default)]
    pub tenant_id: Option<String>,
    #[serde(default)]
    pub rag: Option<RagRef>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct EngramPreProvenance {
    pub id: Uuid,
    pub life_id: Uuid,
    pub neurons: Vec<NeuronId>,
    #[serde(default)]
    pub ec_neurons: Vec<NeuronId>,
    #[serde(default)]
    pub dg_neurons: Vec<NeuronId>,
    #[serde(default)]
    pub separation_index: f32,
    pub episode: EpisodePreProvenance,
    pub salience: f32,
    pub encoded_at_tick: u64,
    pub encoded_at_stage: u8,
    pub replay_count: u32,
    pub is_core: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
struct HippocampusPreProvenance {
    pub engrams: Vec<EngramPreProvenance>,
    #[serde(default)]
    pub rag_index: std::collections::HashMap<String, Uuid>,
}

fn upgrade_episode(ep: EpisodePreProvenance) -> Episode {
    Episode {
        content: ep.content,
        context: ep.context,
        outcome: ep.outcome,
        salience_hint: ep.salience_hint,
        semantic_vector: ep.semantic_vector,
        agent_id: ep.agent_id,
        tenant_id: ep.tenant_id,
        rag: ep.rag,
        provenance: None,
    }
}

fn upgrade_engram(e: EngramPreProvenance) -> Engram {
    Engram {
        id: e.id,
        life_id: e.life_id,
        neurons: e.neurons,
        ec_neurons: e.ec_neurons,
        dg_neurons: e.dg_neurons,
        separation_index: e.separation_index,
        episode: upgrade_episode(e.episode),
        salience: e.salience,
        encoded_at_tick: e.encoded_at_tick,
        encoded_at_stage: e.encoded_at_stage,
        replay_count: e.replay_count,
        is_core: e.is_core,
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct EpisodeV2 {
    content: String,
    context: String,
    outcome: Option<String>,
    salience_hint: f32,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct EngramOnDisk {
    id: Uuid,
    life_id: Uuid,
    neurons: Vec<NeuronId>,
    episode: EpisodePreProvenance,
    salience: f32,
    encoded_at_tick: u64,
    encoded_at_stage: u8,
    replay_count: u32,
    is_core: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
struct HippocampusPreNoIndex {
    engrams: Vec<EngramPreProvenance>,
}

fn from_pre_no_index(legacy: HippocampusPreNoIndex) -> Hippocampus {
    let mut h = Hippocampus {
        engrams: legacy.engrams.into_iter().map(upgrade_engram).collect(),
        rag_index: Default::default(),
    };
    h.rebuild_rag_index();
    h
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
struct HippocampusOnDisk {
    engrams: Vec<EngramOnDisk>,
}

fn upgrade_engram_on_disk(e: EngramOnDisk) -> Engram {
    Engram {
        id: e.id,
        life_id: e.life_id,
        neurons: e.neurons,
        ec_neurons: vec![],
        dg_neurons: vec![],
        separation_index: 0.0,
        episode: upgrade_episode(e.episode),
        salience: e.salience,
        encoded_at_tick: e.encoded_at_tick,
        encoded_at_stage: e.encoded_at_stage,
        replay_count: e.replay_count,
        is_core: e.is_core,
    }
}

fn from_on_disk(legacy: HippocampusOnDisk) -> Hippocampus {
    let mut h = Hippocampus {
        engrams: legacy.engrams.into_iter().map(upgrade_engram_on_disk).collect(),
        rag_index: Default::default(),
    };
    h.rebuild_rag_index();
    h
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct EngramLegacyMinimal {
    id: Uuid,
    life_id: Uuid,
    neurons: Vec<NeuronId>,
    episode: EpisodeV2,
    salience: f32,
    encoded_at_tick: u64,
    encoded_at_stage: u8,
    replay_count: u32,
    is_core: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
struct HippocampusLegacyMinimal {
    engrams: Vec<EngramLegacyMinimal>,
}

fn upgrade_engram_minimal(e: EngramLegacyMinimal) -> Engram {
    Engram {
        id: e.id,
        life_id: e.life_id,
        neurons: e.neurons,
        ec_neurons: vec![],
        dg_neurons: vec![],
        separation_index: 0.0,
        episode: upgrade_episode_v2(e.episode),
        salience: e.salience,
        encoded_at_tick: e.encoded_at_tick,
        encoded_at_stage: e.encoded_at_stage,
        replay_count: e.replay_count,
        is_core: e.is_core,
    }
}

fn from_legacy_minimal(legacy: HippocampusLegacyMinimal) -> Hippocampus {
    let mut h = Hippocampus {
        engrams: legacy
            .engrams
            .into_iter()
            .map(upgrade_engram_minimal)
            .collect(),
        rag_index: Default::default(),
    };
    h.rebuild_rag_index();
    h
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct EngramV2 {
    id: Uuid,
    life_id: Uuid,
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

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
struct HippocampusV2 {
    engrams: Vec<EngramV2>,
}

fn upgrade_episode_v2(ep: EpisodeV2) -> Episode {
    Episode {
        content: ep.content,
        context: ep.context,
        outcome: ep.outcome,
        salience_hint: ep.salience_hint,
        semantic_vector: None,
        agent_id: None,
        tenant_id: None,
        rag: None,
        provenance: None,
    }
}

fn upgrade_engram_v2(e: EngramV2) -> Engram {
    Engram {
        id: e.id,
        life_id: e.life_id,
        neurons: e.neurons,
        ec_neurons: e.ec_neurons,
        dg_neurons: e.dg_neurons,
        separation_index: e.separation_index,
        episode: upgrade_episode_v2(e.episode),
        salience: e.salience,
        encoded_at_tick: e.encoded_at_tick,
        encoded_at_stage: e.encoded_at_stage,
        replay_count: e.replay_count,
        is_core: e.is_core,
    }
}

fn from_pre_prov(legacy: HippocampusPreProvenance) -> Hippocampus {
    Hippocampus {
        engrams: legacy.engrams.into_iter().map(upgrade_engram).collect(),
        rag_index: legacy.rag_index,
    }
}

fn from_v2(legacy: HippocampusV2) -> Hippocampus {
    let mut h = Hippocampus {
        engrams: legacy.engrams.into_iter().map(upgrade_engram_v2).collect(),
        rag_index: Default::default(),
    };
    h.rebuild_rag_index();
    h
}

pub fn read_hippocampus_segment(base: &std::path::Path) -> Result<Hippocampus> {
    if let Ok(h) = crate::segment::read_segment::<Hippocampus>(base, "hippocampus") {
        return Ok(h);
    }
    if let Ok(legacy) = crate::segment::read_segment::<HippocampusPreProvenance>(base, "hippocampus") {
        return Ok(from_pre_prov(legacy));
    }
    if let Ok(legacy) = crate::segment::read_segment::<HippocampusPreNoIndex>(base, "hippocampus") {
        return Ok(from_pre_no_index(legacy));
    }
    if let Ok(on_disk) = crate::segment::read_segment::<HippocampusOnDisk>(base, "hippocampus") {
        return Ok(from_on_disk(on_disk));
    }
    if let Ok(min) = crate::segment::read_segment::<HippocampusLegacyMinimal>(base, "hippocampus") {
        return Ok(from_legacy_minimal(min));
    }
    if let Ok(v2) = crate::segment::read_segment::<HippocampusV2>(base, "hippocampus") {
        return Ok(from_v2(v2));
    }
    crate::segment::read_segment(base, "hippocampus")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_roundtrip_upgrade() {
        let leg = HippocampusPreProvenance {
            engrams: vec![EngramPreProvenance {
                id: Uuid::new_v4(),
                life_id: Uuid::new_v4(),
                neurons: vec![],
                ec_neurons: vec![],
                dg_neurons: vec![],
                separation_index: 0.0,
                episode: EpisodePreProvenance {
                    content: "hello".into(),
                    context: "c".into(),
                    outcome: None,
                    salience_hint: 0.5,
                    semantic_vector: None,
                    agent_id: None,
                    tenant_id: None,
                    rag: None,
                },
                salience: 0.5,
                encoded_at_tick: 0,
                encoded_at_stage: 1,
                replay_count: 0,
                is_core: false,
            }],
            rag_index: Default::default(),
        };
        let bytes = bincode::serialize(&leg).unwrap();
        let decoded: HippocampusPreProvenance = bincode::deserialize(&bytes).unwrap();
        let up = Hippocampus {
            engrams: decoded.engrams.into_iter().map(upgrade_engram).collect(),
            rag_index: Default::default(),
        };
        assert_eq!(up.engrams[0].episode.provenance, None);
    }

    #[test]
    fn load_prod_hippocampus_if_present() {
        let path = std::path::Path::new("/home/ambugo/.fluctlight/tenants/default/brain");
        if !path.join("hippocampus.seg").exists() {
            return;
        }
        let h = read_hippocampus_segment(path).expect("prod hippocampus");
        assert!(!h.engrams.is_empty(), "expected engrams in prod brain");
    }
}
