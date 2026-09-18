use std::collections::{HashMap, HashSet};

use uuid::Uuid;

use crate::dentate::cue_to_dg_neurons;
use crate::engram::Engram;
use crate::graph::BrainGraph;
use crate::hippocampus::Hippocampus;
use crate::id::NeuronId;
use crate::semantic::SemanticField;
use crate::types::{ActivationResult, RecallResult};

fn env_truthy(key: &str) -> bool {
    std::env::var(key)
        .map(|v| matches!(v.to_lowercase().as_str(), "1" | "true" | "yes" | "on"))
        .unwrap_or(false)
}

/// Index / vector-only recall: skip graph spreading (Chroma-class latency).
///
/// Still read from the environment on demand: the Python SDK's `connect_*()` helpers set
/// these flags at runtime and expect the very next `experience()` to observe them, so
/// memoizing them process-wide would silently break documented mode switching.
pub fn vector_fast_mode() -> bool {
    env_truthy("FLUCTLIGHT_VECTOR_FAST")
}

/// Bulk IR ingest: skip dentate/graph wiring; index + semantic vector only.
pub fn fast_ingest_mode() -> bool {
    env_truthy("FLUCTLIGHT_FAST_INGEST")
}

/// Agent hot path: shallow spread + capped hybrid candidates (SYNAPSE-style selective subgraph).
pub fn agent_fast_mode() -> bool {
    env_truthy("FLUCTLIGHT_AGENT_FAST")
}

pub fn activation_max_hops() -> u32 {
    if vector_fast_mode() {
        0
    } else if agent_fast_mode() {
        1
    } else {
        4
    }
}

/// Spreading activation recall — graph propagation, optionally seeded by entorhinal semantic vectors.
// Argument count grew when the neuron codec became per-brain state. The codec must be
// threaded explicitly rather than read from a global: `serve.rs` pools many brains and
// serves them from a thread per connection, so a process-wide codec would let a
// legacy-pinned tenant and a migrated one derive each other's neuron ids mid-request.
#[allow(clippy::too_many_arguments)]
pub fn activate_from(
    cue: &str,
    graph: &BrainGraph,
    hippocampus: &Hippocampus,
    life_id: Uuid,
    max_hops: u32,
    myelination: f32,
    top_k: usize,
    codec: u8,
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
        codec,
        &[],
    )
}

/// Spreading activation over the synapse graph. Additive per hop; a node's incoming deltas
/// are summed before being applied so that inhibitory (negative-weight) edges subtract
/// deterministically regardless of `HashMap` iteration order. Graphs with no negative weight
/// run the original loop verbatim, so they stay bit-identical to the pre-connectome engine.
pub fn spread(activation: &mut HashMap<NeuronId, f32>, graph: &BrainGraph, max_hops: u32, spread_factor: f32) {
    if !graph.has_inhibitory {
        // No negative weights anywhere: run the exact pre-connectome loop so ordinary brains
        // are bit-identical to before (float re-association in the summed path below drifts
        // up to ~6e-6 on 60-node graphs; measured in the final review).
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
        return;
    }
    for _hop in 0..max_hops {
        let current: Vec<(NeuronId, f32)> = activation.iter().map(|(k, v)| (*k, *v)).collect();
        let mut incoming: HashMap<NeuronId, f32> = HashMap::new();
        for (node, act) in current {
            for (synapse, to) in graph.neighbors(node) {
                let delta = act * synapse.weight * spread_factor;
                if delta.abs() > 0.001 {
                    *incoming.entry(to).or_insert(0.0) += delta;
                }
            }
        }
        for (to, sum) in incoming {
            let e = activation.entry(to).or_insert(0.0);
            *e = (*e + sum).max(0.0);
        }
        activation.retain(|_, v| *v > 0.01);
    }
}

// Argument count grew when the neuron codec became per-brain state. The codec must be
// threaded explicitly rather than read from a global: `serve.rs` pools many brains and
// serves them from a thread per connection, so a process-wide codec would let a
// legacy-pinned tenant and a migrated one derive each other's neuron ids mid-request.
#[allow(clippy::too_many_arguments)]
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
    codec: u8,
    extra_seeds: &[NeuronId],
) -> ActivationResult {
    let cue_neurons = cue_to_dg_neurons(cue, life_id, codec);

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
            semantic.engram_similarities(
                vec,
                &engram_refs.iter().map(|e| (*e).clone()).collect::<Vec<_>>(),
            )
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
        for n in semantic.cue_ec_neurons(vec, life_id, cue_id, codec) {
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

    for n in extra_seeds {
        let e = activation.entry(*n).or_insert(0.0);
        *e = e.max(1.0);
    }
    let connectome_seeds = if extra_seeds.is_empty() {
        None
    } else {
        Some(extra_seeds.len())
    };

    let spread_factor = 0.6 * myelination.max(0.1);
    spread(&mut activation, graph, max_hops, spread_factor);

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
            let completion = if vector_fast_mode() {
                // Index / IR path: cosine-dominant ranking (Chroma-class) with lexical tie-break.
                semantic_boost * 0.82 + overlap * 0.12 + graph_boost * 0.06
            } else {
                overlap * 0.45 + graph_boost * 0.35 + semantic_boost * 0.20
            };
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
        connectome_seeds,
        attention: None,
    }
}

/// Cap candidate set size when the index returns too many IDs.
///
/// `ids` arrives in rank order from [`crate::index::RecallIndex::hybrid_candidates`], so
/// truncation drops the weakest candidates. The `HashSet` is built *after* truncation and
/// is only used for membership testing in `activate_from_hybrid` — building it first would
/// re-randomize the order and put us back where we started.
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
        .unwrap_or(crate::index::DEFAULT_CANDIDATE_CAP)
}

/// Pattern completion — retrieve full engram from partial cue (CA3 analog).
pub fn complete(
    cue: &str,
    hippocampus: &Hippocampus,
    life_id: uuid::Uuid,
    codec: u8,
) -> Option<Engram> {
    let cue_neurons = cue_to_dg_neurons(cue, life_id, codec);
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
        semantic.register_engram(id, life, v, crate::id::CURRENT_CODEC);

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
            crate::id::CURRENT_CODEC,
            &[],
        );
        assert!(!result.recalls.is_empty());
        assert!(result.recalls[0].activation > 0.1);
    }

    /// Reference implementation of the pre-plan loop (activation.rs:145-153 before this task).
    fn legacy_spread(activation: &mut HashMap<NeuronId, f32>, graph: &BrainGraph, max_hops: u32, spread_factor: f32) {
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
    }

    #[test]
    fn spread_is_identical_to_legacy_when_all_weights_positive() {
        use crate::plasticity::Synapse;
        use crate::types::Region;
        // Deterministic pseudo-random graph: 60 nodes, 300 edges, weights in (0,1].
        let mut g = BrainGraph::default();
        g.rebuild_index();
        let mut x: u64 = 0x9E37_79B9_7F4A_7C15;
        let mut next = || { x ^= x << 13; x ^= x >> 7; x ^= x << 17; x };
        for _ in 0..300 {
            let from = NeuronId(next() % 60);
            let to = NeuronId(next() % 60);
            let w = ((next() % 1000) as f32 + 1.0) / 1000.0;
            g.add_synapse_uncapped(Synapse::new(from, to, Region::Cortex, w));
        }
        let seeds: HashMap<NeuronId, f32> = (0..5u64).map(|i| (NeuronId(i), 1.0)).collect();
        let mut a = seeds.clone();
        let mut b = seeds;
        legacy_spread(&mut a, &g, 4, 0.6);
        spread(&mut b, &g, 4, 0.6);
        assert!(!g.has_inhibitory, "an all-positive graph must take the legacy fast path");
        assert_eq!(a.len(), b.len());
        for (k, v) in &a {
            let bv = b.get(k).copied().unwrap_or(f32::NAN);
            assert_eq!(*v, bv, "{k:?}: legacy {v} vs new {bv}");
        }
        assert_eq!(a, b, "the no-inhibition path must be bit-identical to the legacy loop");
    }

    /// The moment a single negative edge exists the graph must switch to the summed,
    /// sign-aware path - and inhibition must still behave exactly as specified.
    #[test]
    fn spread_uses_summed_path_when_graph_has_inhibitory() {
        use crate::plasticity::Synapse;
        use crate::types::Region;
        // Same deterministic 60-node / 300-edge graph as the bit-identity test.
        let mut g = BrainGraph::default();
        g.rebuild_index();
        let mut x: u64 = 0x9E37_79B9_7F4A_7C15;
        let mut next = || { x ^= x << 13; x ^= x >> 7; x ^= x << 17; x };
        for _ in 0..300 {
            let from = NeuronId(next() % 60);
            let to = NeuronId(next() % 60);
            let w = ((next() % 1000) as f32 + 1.0) / 1000.0;
            g.add_synapse_uncapped(Synapse::new(from, to, Region::Cortex, w));
        }
        assert!(!g.has_inhibitory);

        // Add the inhibition triple on ids the random graph cannot reach (a -> b excitatory,
        // i -> b inhibitory), so the a/i/b behaviour is exactly the inhibition test's.
        let (a, i, b) = (NeuronId(101), NeuronId(102), NeuronId(103));
        g.add_synapse_uncapped(Synapse::new(a, b, Region::Cortex, 1.0));
        g.add_synapse_uncapped(Synapse::new(i, b, Region::Cortex, -1.0));
        assert!(g.has_inhibitory, "one negative edge must flip the graph onto the summed path");

        // Excitatory only: b lights up.
        let mut act: HashMap<NeuronId, f32> = [(a, 1.0)].into_iter().collect();
        spread(&mut act, &g, 1, 0.6);
        assert!((act[&b] - 0.6).abs() < 1e-6);
        // Equally active inhibitory seed: net zero, b drops out, regardless of map order.
        for _ in 0..20 {
            let mut act: HashMap<NeuronId, f32> = [(a, 1.0), (i, 1.0)].into_iter().collect();
            spread(&mut act, &g, 1, 0.6);
            assert!(!act.contains_key(&b), "b must be fully suppressed");
        }
        // Partial inhibition: 1.0 - 0.5 => b = 0.3
        let mut act: HashMap<NeuronId, f32> = [(a, 1.0), (i, 0.5)].into_iter().collect();
        spread(&mut act, &g, 1, 0.6);
        assert!((act[&b] - 0.3).abs() < 1e-6);
    }

    #[test]
    fn spread_inhibition_suppresses_and_is_order_independent() {
        use crate::plasticity::Synapse;
        use crate::types::Region;
        let (a, i, b) = (NeuronId(1), NeuronId(2), NeuronId(3));
        let mut g = BrainGraph::default();
        g.rebuild_index();
        g.add_synapse_uncapped(Synapse::new(a, b, Region::Cortex, 1.0));
        g.add_synapse_uncapped(Synapse::new(i, b, Region::Cortex, -1.0));
        // Excitatory only: b lights up.
        let mut act: HashMap<NeuronId, f32> = [(a, 1.0)].into_iter().collect();
        spread(&mut act, &g, 1, 0.6);
        assert!((act[&b] - 0.6).abs() < 1e-6);
        // With the inhibitory seed equally active: net zero, b drops out — regardless of map order.
        for _ in 0..20 {
            let mut act: HashMap<NeuronId, f32> = [(a, 1.0), (i, 1.0)].into_iter().collect();
            spread(&mut act, &g, 1, 0.6);
            assert!(!act.contains_key(&b), "b must be fully suppressed");
        }
        // Partial inhibition: 1.0 - 0.5 => b = 0.3
        let mut act: HashMap<NeuronId, f32> = [(a, 1.0), (i, 0.5)].into_iter().collect();
        spread(&mut act, &g, 1, 0.6);
        assert!((act[&b] - 0.3).abs() < 1e-6);
    }

    #[test]
    fn extra_seeds_enter_activation_at_full_strength() {
        use crate::hippocampus::Hippocampus;
        use crate::semantic::SemanticField;
        let g = BrainGraph::default();
        let h = Hippocampus::default();
        let seeds = [NeuronId(777), NeuronId(778)];
        let r = activate_from_hybrid("zz", None, &g, &h, &SemanticField::default(), Uuid::nil(), 0, 1.0, 8, None, crate::id::CURRENT_CODEC, &seeds);
        assert_eq!(r.connectome_seeds, Some(2));
        let r0 = activate_from_hybrid("zz", None, &g, &h, &SemanticField::default(), Uuid::nil(), 0, 1.0, 8, None, crate::id::CURRENT_CODEC, &[]);
        assert_eq!(r0.connectome_seeds, None);
        assert_eq!(r.active_neurons, r0.active_neurons + 2);
    }
}
