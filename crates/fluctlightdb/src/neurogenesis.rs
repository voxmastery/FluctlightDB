//! Adult neurogenesis analog — immature engrams compete to improve pattern separation.

use uuid::Uuid;

use crate::dentate::SeparationResult;
use crate::engram::Engram;
use crate::hippocampus::Hippocampus;
use crate::types::Episode;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct NeurogenesisReport {
    pub seeded: u32,
    pub pruned_immature: u32,
    pub rate: f32,
}

pub fn neurogenesis_rate() -> f32 {
    std::env::var("FLUCTLIGHT_NEUROGENESIS_RATE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.05)
}

pub fn pulse(
    hippocampus: &mut Hippocampus,
    life_id: Uuid,
    tick: u64,
    stage: u8,
) -> NeurogenesisReport {
    let rate = neurogenesis_rate();
    let n_seed = if rate <= 0.0 {
        0
    } else {
        (rate * 3.0).ceil() as u32
    };
    let mut seeded = 0u32;
    for i in 0..n_seed {
        let content = format!("immature probe tick{tick}#{i}");
        let sep = SeparationResult {
            ec_neurons: vec![],
            dg_neurons: vec![],
            ca3_neurons: vec![],
            separation_index: 0.2,
            max_overlap_before: 0.0,
            max_overlap_after: 0.0,
            separators_added: 0,
            token_count: 3,
        };
        hippocampus.encode(Engram::from_separation(
            life_id,
            Episode {
                content,
                context: "neurogenesis:immature".into(),
                outcome: None,
                salience_hint: 0.15,
                semantic_vector: None,
                agent_id: None,
                tenant_id: None,
                rag: None,
                provenance: None,
            },
            0.15,
            tick,
            stage,
            &sep,
        ));
        seeded += 1;
    }

    let before = hippocampus.engrams.len();
    hippocampus.engrams.retain(|e| {
        if e.life_id != life_id {
            return true;
        }
        if e.salience > 0.2 {
            return true;
        }
        if e.episode.context.starts_with("neurogenesis:") {
            return e.separation_index >= 0.5;
        }
        true
    });
    let pruned = (before.saturating_sub(hippocampus.engrams.len())) as u32;

    NeurogenesisReport {
        seeded,
        pruned_immature: pruned,
        rate,
    }
}
