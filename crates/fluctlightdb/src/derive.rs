//! Re-derivation spine — recovering the clean content code from a stored engram.
//!
//! # The structural property this rests on
//!
//! Every neuron-identity artifact on an [`Engram`] is a pure function of fields that are
//! *already persisted*: `ec_neurons` from `episode`, `dg_neurons` from `episode` + `life_id`,
//! the separator tail from `id` + `encoded_at_tick`, and the CA3 set from those. Nothing in
//! the neuron layer is primary data — it is a rebuildable projection.
//!
//! # Why that matters
//!
//! [`crate::dentate::separate_episode`] appends up to [`MAX_SEPARATOR_ATTEMPTS`] artificial
//! `sep` neurons into `dg_neurons` when a new episode overlaps an existing one, deliberately
//! pushing the two codes apart. That is correct for *storage* — it is what stops two similar
//! memories from colliding — but it means `dg_neurons` is **a content code plus deliberate
//! noise**, and the noise is unique per engram by construction.
//!
//! Two consumers were reading it as if it were a pure content code:
//!
//! - [`crate::compact`] merges engrams whose `dg_neurons` Jaccard exceeds 0.85. The
//!   separators are in that set and never match, so the score is capped below the threshold
//!   and near-identical engrams were never merged.
//! - [`crate::separation_gate`] scored a candidate against `peer.separation_index`, which is
//!   *high precisely because the DG fabricated distinctness for that peer* — so a duplicate
//!   could inherit its neighbour's manufactured uniqueness and slip through the gate.
//!
//! [`content_dg`] recovers the clean code by recomputing the ≤6 separator ids from the
//! engram's own persisted `id` and `encoded_at_tick` and subtracting them. That is exact
//! (it reproduces precisely what the separator loop added) and costs six hashes per engram —
//! no re-tokenization, so it is safe on the ingest hot path where the gate scans up to 512
//! peers per write.

use crate::dentate::MAX_SEPARATOR_ATTEMPTS;
use crate::engram::Engram;
use crate::id::NeuronId;

/// The clean content code: stored `dg_neurons` with this engram's own separators removed.
///
/// Exact rather than approximate — it recomputes the same seeds the separator loop used
/// (`["sep", engram_id, attempt, tick]`) and filters them out, so what remains is byte-for-byte
/// the granule set `expand_granules` produced from the episode text.
pub fn content_dg(engram: &Engram, codec: u8) -> Vec<NeuronId> {
    if engram.dg_neurons.is_empty() {
        return Vec::new();
    }
    let id = engram.id.to_string();
    let tick = engram.encoded_at_tick.to_string();
    let separators: Vec<NeuronId> = (0..MAX_SEPARATOR_ATTEMPTS)
        .map(|attempt| NeuronId::from_seeds_with(codec, &["sep", &id, &attempt.to_string(), &tick]))
        .collect();
    engram
        .dg_neurons
        .iter()
        .copied()
        .filter(|n| !separators.contains(n))
        .collect()
}

/// Jaccard over two neuron sets. Shared by the gate and the compactor so both score
/// similarity the same way.
pub fn neuron_jaccard(a: &[NeuronId], b: &[NeuronId]) -> f32 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    use std::collections::HashSet;
    let sa: HashSet<NeuronId> = a.iter().copied().collect();
    let sb: HashSet<NeuronId> = b.iter().copied().collect();
    let inter = sa.intersection(&sb).count() as f32;
    let union = sa.union(&sb).count() as f32;
    if union > 0.0 {
        inter / union
    } else {
        0.0
    }
}

/// Re-derive one engram's whole neuron ensemble under `codec`, in place.
///
/// Every input is already persisted, so this needs no extra state and no new segment.
/// The peer window is cloned before the mutable borrow because `tail_for_life` hands back
/// references into the hippocampus we are about to write to.
pub fn rekey_engram(brain: &mut crate::brain::FluctlightBrain, engram_id: uuid::Uuid) -> bool {
    let codec = crate::id::CURRENT_CODEC;
    let Some(idx) = brain
        .hippocampus
        .engrams
        .iter()
        .position(|e| e.id == engram_id)
    else {
        return false;
    };
    let (episode, life_id, tick, stage) = {
        let e = &brain.hippocampus.engrams[idx];
        (
            e.episode.clone(),
            e.life_id,
            e.encoded_at_tick,
            e.encoded_at_stage,
        )
    };

    // Exclude the engram being re-keyed from its own peer window, or it inflates its
    // separator count against its own stale code.
    let window = crate::separation_gate::overlap_window();
    let peers: Vec<Engram> = brain
        .hippocampus
        .tail_for_life(life_id, window)
        .into_iter()
        .filter(|e| e.id != engram_id)
        .cloned()
        .collect();
    let peer_refs: Vec<&Engram> = peers.iter().collect();

    let sep =
        crate::dentate::separate_episode(&episode, life_id, engram_id, tick, &peer_refs, codec);

    {
        let e = &mut brain.hippocampus.engrams[idx];
        e.ec_neurons = sep.ec_neurons.clone();
        e.dg_neurons = sep.dg_neurons.clone();
        e.neurons = sep.ca3_neurons.clone();
        e.separation_index = sep.separation_index;
    }

    // Re-wire at the engram's ORIGINAL developmental budget, not today's — an engram encoded
    // as a newborn should not acquire an adult's fan-out just because it was re-keyed.
    let budget =
        crate::budget::WiringBudget::for_stage(crate::development::DevStage::from_u8(stage));
    crate::budget::wire_chain(
        &mut brain.graph,
        &sep.dg_neurons,
        crate::types::Region::HippocampusDg,
        0.3,
        budget.max_dg_chain_links,
    );
    crate::budget::wire_dg_to_ec(
        &mut brain.graph,
        &sep.dg_neurons,
        &sep.ec_neurons,
        budget.max_dg_to_ec_links,
    );

    if let Some(v) = brain.semantic.engram_vectors.get(&engram_id).cloned() {
        brain.semantic.register_engram(engram_id, life_id, v, codec);
    }
    true
}

/// Re-key up to `limit` pending engrams, oldest first, and return how many were done.
pub fn drain(brain: &mut crate::brain::FluctlightBrain, limit: usize) -> u64 {
    if brain.rekey_pending.is_empty() || limit == 0 {
        return 0;
    }
    let batch: Vec<uuid::Uuid> = brain.rekey_pending.iter().take(limit).copied().collect();
    let mut done = 0u64;
    for id in batch {
        // Order is CORRECTNESS, not throughput. `separate_episode`'s separator loop reads a
        // live `tail_for_life` window, so engram N's derived code depends on engrams 0..N-1
        // as they stood at encode time. Re-keying out of order yields a different separator
        // count and a different separation_index. A generic REINDEX has no analogue: a
        // Postgres row's index entry does not depend on the previous row's.
        if rekey_engram(brain, id) {
            done += 1;
        }
        brain.rekey_pending.retain(|p| *p != id);
    }
    if done > 0 {
        brain.life.neuron_codec = crate::id::CURRENT_CODEC;
        brain.life.codec_probes = crate::life::codec_probes_for(crate::id::CURRENT_CODEC);
        brain.invalidate_activation_cache();
    }
    done
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dentate::{expand_granules, separate_episode};
    use crate::id::CURRENT_CODEC;
    use crate::tokenize::tokenize_rich;
    use crate::types::Episode;
    use uuid::Uuid;

    fn engram_with_separators() -> (Engram, u32) {
        let life = Uuid::from_u128(11);
        // Two near-identical episodes force the DG to synthesize separators for the second.
        let e1 = Episode::new("task failed with gateway timeout", "prod", 0.5);
        let id1 = Uuid::from_u128(1);
        let r1 = separate_episode(&e1, life, id1, 1, &[], CURRENT_CODEC);
        let mut g1 = Engram::from_separation(life, e1, 0.5, 1, 1, &r1);
        g1.id = id1;

        let e2 = Episode::new("task failed with gateway timeout", "prod", 0.5);
        let id2 = Uuid::from_u128(2);
        let r2 = separate_episode(&e2, life, id2, 7, &[&g1], CURRENT_CODEC);
        let mut g2 = Engram::from_separation(life, e2, 0.5, 7, 1, &r2);
        g2.id = id2;
        (g2, r2.separators_added)
    }

    /// `content_dg` must return exactly the granule set the episode text produces —
    /// no separators, nothing missing.
    #[test]
    fn content_dg_strips_exactly_the_separators() {
        let (engram, added) = engram_with_separators();
        assert!(
            added > 0,
            "precondition: the DG had to fabricate distinctness"
        );

        let clean = content_dg(&engram, CURRENT_CODEC);
        assert_eq!(
            engram.dg_neurons.len() - clean.len(),
            added as usize,
            "exactly the fabricated separators should be removed"
        );

        let rich = tokenize_rich(
            &engram.episode.content,
            &engram.episode.context,
            engram.episode.outcome.as_deref(),
        );
        let expected = expand_granules(&rich, engram.life_id, CURRENT_CODEC);
        let mut got = clean;
        got.sort_unstable();
        let mut want = expected;
        want.sort_unstable();
        assert_eq!(got, want, "clean code must equal the pure content granules");
    }

    /// The whole point: two engrams with identical text score 1.0 on the clean code but
    /// are pushed apart on the raw stored code by separators that exist only to tell them
    /// apart in storage.
    #[test]
    fn separators_suppress_similarity_of_identical_content() {
        let (g2, added) = engram_with_separators();
        assert!(added > 0);
        let life = g2.life_id;
        let e1 = Episode::new("task failed with gateway timeout", "prod", 0.5);
        let r1 = separate_episode(&e1, life, Uuid::from_u128(1), 1, &[], CURRENT_CODEC);
        let mut g1 = Engram::from_separation(life, e1, 0.5, 1, 1, &r1);
        g1.id = Uuid::from_u128(1);

        let raw = neuron_jaccard(&g1.dg_neurons, &g2.dg_neurons);
        let clean = neuron_jaccard(
            &content_dg(&g1, CURRENT_CODEC),
            &content_dg(&g2, CURRENT_CODEC),
        );
        assert!(
            clean > raw,
            "clean similarity ({clean}) must exceed separator-polluted similarity ({raw})"
        );
        assert!(
            clean > 0.85,
            "identical content must clear the compactor's 0.85 merge threshold, got {clean}"
        );
    }

    #[test]
    fn content_dg_is_a_noop_without_separators() {
        let life = Uuid::from_u128(3);
        let e = Episode::new("a unique unrepeated observation", "ctx", 0.4);
        let id = Uuid::from_u128(9);
        let r = separate_episode(&e, life, id, 2, &[], CURRENT_CODEC);
        assert_eq!(
            r.separators_added, 0,
            "precondition: nothing to separate from"
        );
        let mut g = Engram::from_separation(life, e, 0.4, 2, 1, &r);
        g.id = id;
        assert_eq!(content_dg(&g, CURRENT_CODEC), g.dg_neurons);
    }
}
