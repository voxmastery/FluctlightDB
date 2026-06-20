use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::engram::Engram;
use crate::id::NeuronId;
use crate::tokenize::{tokenize_rich, RichToken};
use crate::types::Episode;

/// Granule cells per EC token — sparse expansion (~2–4% of pool in biology).
const GRANULES_PER_TOKEN: u32 = 2;
const OVERLAP_THRESHOLD: f32 = 0.28;
const MAX_SEPARATOR_ATTEMPTS: u32 = 6;

/// Result of dentate gyrus pattern separation (Marr DG → CA3).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SeparationResult {
    pub ec_neurons: Vec<NeuronId>,
    pub dg_neurons: Vec<NeuronId>,
    pub ca3_neurons: Vec<NeuronId>,
    /// 1.0 = fully orthogonal to nearest neighbor engram.
    pub separation_index: f32,
    pub max_overlap_before: f32,
    pub max_overlap_after: f32,
    pub separators_added: u32,
    pub token_count: usize,
}

pub fn separate_episode(
    episode: &Episode,
    life_id: Uuid,
    engram_id: Uuid,
    tick: u64,
    existing: &[&Engram],
) -> SeparationResult {
    let rich = tokenize_rich(
        &episode.content,
        &episode.context,
        episode.outcome.as_deref(),
    );

    let ec_neurons: Vec<NeuronId> = rich
        .iter()
        .map(|t| NeuronId::from_seeds(&["ec", &t.surface]))
        .collect();

    let mut dg_neurons = expand_granules(&rich, life_id);

    let max_overlap_before = max_jaccard(&dg_neurons, existing);

    let mut separators_added = 0u32;
    let mut attempt = 0u32;
    while max_jaccard_single(&dg_neurons, existing) > OVERLAP_THRESHOLD
        && attempt < MAX_SEPARATOR_ATTEMPTS
    {
        let sep = NeuronId::from_seeds(&[
            "sep",
            &engram_id.to_string(),
            &attempt.to_string(),
            &tick.to_string(),
        ]);
        dg_neurons.push(sep);
        separators_added += 1;
        attempt += 1;
    }

    let max_overlap_after = max_jaccard_single(&dg_neurons, existing);
    let separation_index = (1.0 - max_overlap_after).clamp(0.0, 1.0);

    let mut ca3_neurons = dg_neurons.clone();
    // CA3 recurrent binding — pairwise within sparse code
    for i in 0..dg_neurons.len().saturating_sub(1) {
        ca3_neurons.push(NeuronId::from_pair(dg_neurons[i], dg_neurons[i + 1]));
    }
    if ec_neurons.len() >= 2 {
        ca3_neurons.push(NeuronId::from_pair(ec_neurons[0], ec_neurons[1]));
    }

    ca3_neurons.sort_unstable();
    ca3_neurons.dedup();

    SeparationResult {
        ec_neurons,
        dg_neurons,
        ca3_neurons,
        separation_index,
        max_overlap_before,
        max_overlap_after,
        separators_added,
        token_count: rich.len(),
    }
}

/// Cue neurons using same DG expansion as encoding (for ACTIVATE).
pub fn cue_to_dg_neurons(cue: &str, life_id: Uuid) -> Vec<NeuronId> {
    let rich = tokenize_rich(cue, "", None);
    expand_granules(&rich, life_id)
}

fn expand_granules(tokens: &[RichToken], life_id: Uuid) -> Vec<NeuronId> {
    let life = life_id.to_string();
    let mut out = Vec::new();
    for t in tokens {
        for g in 0..GRANULES_PER_TOKEN {
            out.push(NeuronId::from_seeds(&[
                "dg",
                &life,
                &t.surface,
                &g.to_string(),
            ]));
        }
    }
    out.sort_unstable();
    out.dedup();
    out
}

fn max_jaccard(dg: &[NeuronId], existing: &[&Engram]) -> f32 {
    max_jaccard_single(dg, existing)
}

fn max_jaccard_single(dg: &[NeuronId], existing: &[&Engram]) -> f32 {
    existing
        .iter()
        .map(|e| jaccard(dg, engram_dg(e)))
        .fold(0.0_f32, f32::max)
}

fn engram_dg(e: &Engram) -> &[NeuronId] {
    if !e.dg_neurons.is_empty() {
        &e.dg_neurons
    } else {
        &e.neurons
    }
}

fn jaccard(a: &[NeuronId], b: &[NeuronId]) -> f32 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let sa: HashSet<_> = a.iter().copied().collect();
    let sb: HashSet<_> = b.iter().copied().collect();
    let inter = sa.intersection(&sb).count() as f32;
    let union = sa.union(&sb).count() as f32;
    if union == 0.0 {
        0.0
    } else {
        inter / union
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Episode;

    #[test]
    fn similar_episodes_get_separated() {
        let life = Uuid::new_v4();
        let e1 = Episode {
            content: "task failed timeout".into(),
            context: "production".into(),
            outcome: None,
            salience_hint: 0.5,
            semantic_vector: None,
            agent_id: None,
            tenant_id: None,
            rag: None,
            provenance: None,
        };
        let id1 = Uuid::new_v4();
        let r1 = separate_episode(&e1, life, id1, 1, &[]);
        let mut engram1 = Engram::from_separation(life, e1.clone(), 0.5, 1, 1, &r1);
        engram1.id = id1;

        let e2 = Episode {
            content: "task failed latency".into(),
            context: "production".into(),
            outcome: None,
            salience_hint: 0.5,
            semantic_vector: None,
            agent_id: None,
            tenant_id: None,
            rag: None,
            provenance: None,
        };
        let r2 = separate_episode(&e2, life, Uuid::new_v4(), 2, &[&engram1]);

        assert!(r2.max_overlap_before > 0.15);
        assert!(r2.separators_added > 0 || r2.max_overlap_after < r2.max_overlap_before);
    }
}
