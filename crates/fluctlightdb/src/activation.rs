use std::collections::{HashMap, HashSet};

use uuid::Uuid;

use crate::dentate::cue_to_dg_neurons;
use crate::engram::Engram;
use crate::graph::BrainGraph;
use crate::hippocampus::Hippocampus;
use crate::id::NeuronId;
use crate::index::DEFAULT_CANDIDATE_CAP;
use crate::semantic::SemanticField;
use crate::types::{ActivationResult, RecallResult};

/// Spreading activation recall — graph propagation, optionally seeded by entorhinal semantic vectors.
pub fn activate_from(
    cue: &str,
    graph: &BrainGraph,
    hippocampus: &Hippocampus,
    life_id: Uuid,
    max_hops: u32,
    myelination: f32,
    top_k: usize,
) -> ActivationResult {
    activate_from_hybrid(
        cue,
        None,
        graph,
        hippocampus,
        &SemanticField::default(),
        life_id,
        max_hops,
        myelination,
        top_k,
        None,
    )
}

pub fn activate_from_hybrid(
    cue: &str,
    cue_vector: Option<&[f32]>,
    graph: &BrainGraph,
    hippocampus: &Hippocampus,
    semantic: &SemanticField,
    life_id: Uuid,
    max_hops: u32,
    myelination: f32,
    top_k: usize,
    candidate_ids: Option<&HashSet<Uuid>>,
) -> ActivationResult {
    let cue_neurons = cue_to_dg_neurons(cue, life_id);

    let engram_refs: Vec<&Engram> = if let Some(ids) = candidate_ids {
        hippocampus
            .engrams_for_life(life_id)
            .filter(|e| ids.contains(&e.id))
            .collect()
    } else {
        hippocampus.engrams_for_life(life_id).collect()
    };

    let semantic_sims = if let Some(vec) = cue_vector {
        if let Some(ids) = candidate_ids {
            let id_list: Vec<Uuid> = ids.iter().copied().collect();
            crate::index::RecallIndex::semantic_sims_for_candidates(semantic, Some(vec), &id_list)
        } else {
            semantic.engram_similarities(vec, &engram_refs.iter().map(|e| (*e).clone()).collect::<Vec<_>>())
        }
    } else {
        HashMap::new()
    };

    let mut activation: HashMap<NeuronId, f32> = HashMap::new();
    for n in &cue_neurons {
        activation.insert(*n, 1.0);
    }

    if let Some(vec) = cue_vector {
        let cue_id = Uuid::new_v4();
        for n in semantic.cue_ec_neurons(vec, life_id, cue_id) {
            activation.insert(n, 0.85);
        }
        for (engram_id, sim) in &semantic_sims {
            if *sim < 0.35 {
                continue;
            }
            if let Some(ec) = semantic.ec_semantic_neurons.get(engram_id) {
                for n in ec {
                    *activation.entry(*n).or_insert(0.0) =
                        activation.get(n).copied().unwrap_or(0.0).max(*sim * 0.9);
                }
            }
        }
    }

    let spread_factor = 0.6 * myelination.max(0.1);
    for _hop in 0..max_hops {
        let current: Vec<(NeuronId, f32)> = activation.iter().map(|(k, v)| (*k, *v)).collect();
        for (node, act) in current {
            for (synapse, to) in graph.neighbors(node) {
                let delta = act * synapse.weight * spread_factor;
                if delta > 0.001 {
                    *activation.entry(to).or_insert(0.0) += delta;
                }
            }
        }
        activation.retain(|_, v| *v > 0.01);
    }

    let mut recalls: Vec<RecallResult> = engram_refs
        .iter()
        .map(|engram| {
            let overlap = engram.cue_overlap(&cue_neurons);
            let graph_boost: f32 = engram
                .neurons
                .iter()
                .filter_map(|n| activation.get(n))
                .sum();
            let semantic_boost = semantic_sims.get(&engram.id).copied().unwrap_or(0.0);
            let completion = overlap * 0.45 + graph_boost * 0.35 + semantic_boost * 0.20;
            RecallResult {
                engram_id: engram.id,
                activation: completion,
                episode: engram.episode.clone(),
                completion_strength: overlap,
                separation_index: engram.separation_index,
                verified: engram
                    .episode
                    .provenance
                    .as_ref()
                    .map(|p| p.verified)
                    .unwrap_or(false),
                trust_note: None,
            }
        })
        .filter(|r| r.activation > 0.05)
        .collect();

    recalls.sort_by(|a, b| b.activation.partial_cmp(&a.activation).unwrap());
    recalls.truncate(top_k);

    ActivationResult {
        recalls,
        active_neurons: activation.len(),
        hops: max_hops,
        myelinated: myelination > 0.5,
    }
}

/// Cap candidate set size when index returns too many IDs.
pub fn cap_candidates(mut ids: Vec<Uuid>, cap: usize) -> HashSet<Uuid> {
    if ids.len() > cap {
        ids.truncate(cap);
    }
    ids.into_iter().collect()
}

pub fn default_candidate_cap() -> usize {
    std::env::var("FLUCTLIGHT_CANDIDATE_CAP")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_CANDIDATE_CAP)
}

/// Pattern completion — retrieve full engram from partial cue (CA3 analog).
pub fn complete(cue: &str, hippocampus: &Hippocampus, life_id: uuid::Uuid) -> Option<Engram> {
    let cue_neurons = cue_to_dg_neurons(cue, life_id);
    hippocampus
        .engrams_for_life(life_id)
        .max_by(|a, b| {
            a.cue_overlap(&cue_neurons)
                .partial_cmp(&b.cue_overlap(&cue_neurons))
                .unwrap()
        })
        .filter(|e| e.cue_overlap(&cue_neurons) > 0.2)
        .cloned()
}

/// Wire engram neurons into graph (CA3 recurrent + feedforward).
pub fn wire_engram(graph: &mut BrainGraph, engram: &Engram, region: crate::types::Region) {
    use crate::plasticity::Synapse;
    use crate::types::Region;

    for &n in &engram.neurons {
        graph.register_neuron(n, Region::HippocampusDg);
    }
    for i in 0..engram.neurons.len() {
        for j in (i + 1)..engram.neurons.len().min(i + 4) {
            graph.add_synapse(Synapse::new(
                engram.neurons[i],
                engram.neurons[j],
                region,
                0.3,
            ));
        }
    }
}

pub fn active_set_from_engram(engram: &Engram) -> HashSet<NeuronId> {
    let mut s: HashSet<NeuronId> = engram.neurons.iter().copied().collect();
    s.extend(engram.dg_neurons.iter().copied());
    s.extend(engram.ec_neurons.iter().copied());
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::semantic::SemanticField;
    use crate::types::Episode;

    #[test]
    fn hybrid_activation_uses_semantic_similarity() {
        let mut hippocampus = Hippocampus::default();
        let life = Uuid::new_v4();
        let id = Uuid::new_v4();
        let v = vec![1.0, 0.0, 0.0];
        let engram = Engram {
            id,
            life_id: life,
            neurons: crate::engram::cue_neurons("alpha", "ctx"),
            ec_neurons: vec![],
            dg_neurons: crate::engram::cue_neurons("alpha", "ctx"),
            separation_index: 1.0,
            episode: Episode {
                content: "alpha event".into(),
                context: "ctx".into(),
                outcome: None,
                salience_hint: 0.7,
                semantic_vector: Some(v.clone()),
                agent_id: None,
                tenant_id: None,
                rag: None,
                provenance: None,
            },
            salience: 0.7,
            encoded_at_tick: 0,
            encoded_at_stage: 1,
            replay_count: 0,
            is_core: false,
        };
        hippocampus.encode(engram);

        let mut semantic = SemanticField::default();
        semantic.register_engram(id, life, v);

        let cue = vec![0.95, 0.05, 0.0];
        let mut candidates = HashSet::new();
        candidates.insert(id);
        let result = activate_from_hybrid(
            "unrelated words",
            Some(&cue),
            &BrainGraph::default(),
            &hippocampus,
            &semantic,
            life,
            3,
            0.5,
            4,
            Some(&candidates),
        );
        assert!(!result.recalls.is_empty());
        assert!(result.recalls[0].activation > 0.1);
    }
}
