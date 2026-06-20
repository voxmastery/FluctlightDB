//! Offline compaction — merge redundant engrams, dedupe synapses, prune weak edges.

use std::collections::HashSet;

use serde::{Serialize};
use uuid::Uuid;

use crate::brain::FluctlightBrain;
use crate::id::NeuronId;
use crate::engram::Engram;
use crate::graph::BrainGraph;
use crate::hippocampus::Hippocampus;
use crate::plasticity::Synapse;
use crate::semantic::cosine_similarity;

#[derive(Debug, Clone, Serialize, Default, PartialEq)]
pub struct CompactReport {
    pub merged_engrams: u32,
    pub removed_engrams: u32,
    pub deduped_synapses: u32,
    pub pruned_synapses: u32,
    pub semantic_centroids: usize,
}

pub fn compact_brain(brain: &mut FluctlightBrain) -> CompactReport {
    let life_id = brain.life.life_id;
    let threshold = brain.development.stage.prune_threshold();

    let merge_result = merge_similar_engrams(
        &mut brain.hippocampus,
        &mut brain.graph,
        &mut brain.semantic,
        life_id,
    );
    let deduped = dedupe_synapses(&mut brain.graph);
    let pruned = brain.graph.prune_below(threshold);

    // Drop orphaned semantic vectors.
    let live: HashSet<Uuid> = brain.hippocampus.engrams.iter().map(|e| e.id).collect();
    brain
        .semantic
        .engram_vectors
        .retain(|id, _| live.contains(id));
    brain
        .semantic
        .ec_semantic_neurons
        .retain(|id, _| live.contains(id));
    for c in &mut brain.semantic.centroids {
        c.engram_ids.retain(|id| live.contains(id));
    }
    brain.semantic.centroids.retain(|c| !c.engram_ids.is_empty());

    CompactReport {
        merged_engrams: merge_result.merged,
        removed_engrams: merge_result.removed,
        deduped_synapses: deduped,
        pruned_synapses: pruned,
        semantic_centroids: brain.semantic.centroids.len(),
    }
}

struct MergeResult {
    merged: u32,
    removed: u32,
}

fn merge_similar_engrams(
    hippocampus: &mut Hippocampus,
    graph: &mut BrainGraph,
    semantic: &mut crate::semantic::SemanticField,
    life_id: Uuid,
) -> MergeResult {
    let mut merged = 0u32;
    let mut removed = 0u32;
    let mut i = 0usize;
    while i < hippocampus.engrams.len() {
        if hippocampus.engrams[i].life_id != life_id || hippocampus.engrams[i].is_core {
            i += 1;
            continue;
        }
        let mut j = i + 1;
        while j < hippocampus.engrams.len() {
            if hippocampus.engrams[j].life_id != life_id || hippocampus.engrams[j].is_core {
                j += 1;
                continue;
            }
            if should_merge(&hippocampus.engrams[i], &hippocampus.engrams[j], semantic) {
                let absorbed = hippocampus.engrams[j].clone();
                let absorbed_id = absorbed.id;
                absorb_engram(graph, &mut hippocampus.engrams[i], absorbed);
                hippocampus.engrams.remove(j);
                semantic.engram_vectors.remove(&absorbed_id);
                semantic.ec_semantic_neurons.remove(&absorbed_id);
                merged += 1;
                removed += 1;
            } else {
                j += 1;
            }
        }
        i += 1;
    }
    MergeResult { merged, removed }
}

fn should_merge(a: &Engram, b: &Engram, semantic: &crate::semantic::SemanticField) -> bool {
    if a.episode.content == b.episode.content && a.episode.context == b.episode.context {
        return true;
    }
    let dg_overlap = jaccard(&a.dg_neurons, &b.dg_neurons);
    if dg_overlap > 0.85 {
        return true;
    }
    if let (Some(va), Some(vb)) = (
        semantic.engram_vectors.get(&a.id),
        semantic.engram_vectors.get(&b.id),
    ) {
        if cosine_similarity(va, vb) > 0.94 && dg_overlap > 0.35 {
            return true;
        }
    }
    false
}

fn absorb_engram(graph: &mut BrainGraph, keeper: &mut Engram, absorbed: Engram) {
    keeper.salience = keeper.salience.max(absorbed.salience);
    keeper.replay_count += absorbed.replay_count;
    merge_neurons(&mut keeper.dg_neurons, &absorbed.dg_neurons);
    merge_neurons(&mut keeper.neurons, &absorbed.neurons);
    merge_neurons(&mut keeper.ec_neurons, &absorbed.ec_neurons);

    // Bridge absorbed ensemble into keeper via associative synapses (Hebbian-ready).
    for &from in &absorbed.neurons {
        for &to in &keeper.neurons {
            if from != to {
                graph.add_synapse(Synapse::new(
                    from,
                    to,
                    crate::types::Region::HippocampusCa3,
                    0.25,
                ));
            }
        }
    }
}

fn merge_neurons(dst: &mut Vec<NeuronId>, src: &[NeuronId]) {
    for &n in src {
        if !dst.contains(&n) {
            dst.push(n);
        }
    }
}

fn jaccard(a: &[crate::id::NeuronId], b: &[crate::id::NeuronId]) -> f32 {
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }
    let sa: HashSet<_> = a.iter().copied().collect();
    let sb: HashSet<_> = b.iter().copied().collect();
    let inter = sa.intersection(&sb).count() as f32;
    let union = sa.union(&sb).count() as f32;
    if union <= 0.0 {
        0.0
    } else {
        inter / union
    }
}

fn dedupe_synapses(graph: &mut BrainGraph) -> u32 {
    let mut seen: HashSet<(u64, u64)> = HashSet::new();
    let mut deduped = 0u32;
    graph.synapses.retain(|s| {
        let key = (s.from.0, s.to.0);
        if seen.contains(&key) {
            deduped += 1;
            false
        } else {
            seen.insert(key);
            true
        }
    });
    deduped
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Episode;

    #[test]
    fn dedupes_identical_synapses() {
        let mut graph = BrainGraph::default();
        let a = crate::id::NeuronId::from_token("a");
        let b = crate::id::NeuronId::from_token("b");
        graph.add_synapse(Synapse::new(a, b, crate::types::Region::HippocampusCa3, 0.5));
        graph.add_synapse(Synapse::new(a, b, crate::types::Region::HippocampusCa3, 0.6));
        let n = dedupe_synapses(&mut graph);
        assert_eq!(n, 0);
        assert_eq!(graph.synapse_count(), 1);
        assert!((graph.synapses[0].weight - 0.6).abs() < 1e-5);
    }

    #[test]
    fn compact_merges_duplicate_content() {
        let mut brain = FluctlightBrain::new();
        let ep = Episode {
            content: "same memory".into(),
            context: "ctx".into(),
            outcome: None,
            salience_hint: 0.5,
            semantic_vector: None,
        agent_id: None,
        tenant_id: None,
        rag: None,
                provenance: None,
    };
        brain.experience(ep.clone()).unwrap();
        let second = brain.experience(ep).unwrap();
        if second.gate_rejected {
            assert_eq!(brain.hippocampus.engrams.len(), 1);
            return;
        }
        let before = brain.hippocampus.engrams.len();
        let report = compact_brain(&mut brain);
        assert!(report.merged_engrams >= 1 || brain.hippocampus.engrams.len() < before);
    }
}
