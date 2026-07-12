use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::engram::Engram;

/// Fast episodic store — pattern separation + completion (Marr / CLS).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Hippocampus {
    pub engrams: Vec<Engram>,
    /// Idempotent RAG chunk lookup: `doc_id#chunk_id` → engram id.
    #[serde(default)]
    pub rag_index: HashMap<String, Uuid>,
}

pub fn rag_chunk_key(doc_id: &str, chunk_id: &str) -> String {
    format!("{doc_id}#{chunk_id}")
}

impl Hippocampus {
    pub fn encode(&mut self, engram: Engram) {
        if let Some(ref rag) = engram.episode.rag {
            if let (Some(doc), Some(chunk)) = (&rag.doc_id, &rag.chunk_id) {
                self.rag_index.insert(rag_chunk_key(doc, chunk), engram.id);
            }
        }
        self.engrams.push(engram);
    }

    pub fn find_rag_chunk(&self, doc_id: &str, chunk_id: &str) -> Option<Uuid> {
        self.rag_index
            .get(&rag_chunk_key(doc_id, chunk_id))
            .copied()
            .and_then(|id| {
                if self.engrams.iter().any(|e| e.id == id) {
                    Some(id)
                } else {
                    None
                }
            })
    }

    pub fn rebuild_rag_index(&mut self) {
        self.rag_index.clear();
        for e in &self.engrams {
            if let Some(ref rag) = e.episode.rag {
                if let (Some(doc), Some(chunk)) = (&rag.doc_id, &rag.chunk_id) {
                    self.rag_index.insert(rag_chunk_key(doc, chunk), e.id);
                }
            }
        }
    }

    pub fn engrams_for_life(&self, life_id: uuid::Uuid) -> impl Iterator<Item = &Engram> {
        self.engrams.iter().filter(move |e| e.life_id == life_id)
    }

    pub fn clear_ephemeral(&mut self, life_id: uuid::Uuid) {
        self.engrams.retain(|e| e.life_id != life_id || e.is_core);
    }

    pub fn mark_core(&mut self, engram_id: uuid::Uuid) {
        if let Some(e) = self.engrams.iter_mut().find(|e| e.id == engram_id) {
            e.is_core = true;
        }
    }

    pub fn recent(&self, life_id: uuid::Uuid, n: usize) -> Vec<&Engram> {
        let mut v: Vec<_> = self.engrams_for_life(life_id).collect();
        v.sort_by_key(|e| std::cmp::Reverse(e.encoded_at_tick));
        v.truncate(n);
        v
    }

    /// Last `n` engrams for a life (append order) — O(window), no full sort.
    pub fn tail_for_life(&self, life_id: uuid::Uuid, n: usize) -> Vec<&Engram> {
        let mut out = Vec::with_capacity(n.min(self.engrams.len()));
        for e in self.engrams.iter().rev() {
            if e.life_id != life_id {
                continue;
            }
            out.push(e);
            if out.len() >= n {
                break;
            }
        }
        out.reverse();
        out
    }

    /// CA3 Hopfield attractor — pattern completion from a partial cue (Marr 1971; Hopfield 1982).
    ///
    /// The CA3 auto-associative network stores engrams as attractor states. A partial set of
    /// active neurons relaxes toward the nearest stored engram via overlap scoring, weighted
    /// by `ca3_recurrent_gain` (ACh-dependent: low ACh = strong recurrent collaterals =
    /// better completion). This is what lets "I remember something about the payment fail..."
    /// complete into the full stored trace without re-reading every engram.
    ///
    /// Returns None if the best overlap is below `overlap_threshold` (prevents hallucination).
    pub fn ca3_attractor_complete(
        &self,
        cue_neurons: &[crate::id::NeuronId],
        life_id: uuid::Uuid,
        ca3_recurrent_gain: f32,
        overlap_threshold: f32,
    ) -> Option<&Engram> {
        use std::collections::HashSet;
        let cue_set: HashSet<_> = cue_neurons.iter().copied().collect();
        if cue_set.is_empty() {
            return None;
        }
        let mut best: Option<(&Engram, f32)> = None;
        for engram in self.engrams_for_life(life_id) {
            let stored: HashSet<_> = engram.neurons.iter().copied().collect();
            if stored.is_empty() {
                continue;
            }
            let intersection = cue_set.intersection(&stored).count() as f32;
            let union = cue_set.union(&stored).count() as f32;
            let jaccard = intersection / union;
            // Scale by recurrent gain: low ACh amplifies the completion pull.
            let score = jaccard * (0.3 + 0.7 * ca3_recurrent_gain);
            if score > overlap_threshold {
                if best.map_or(true, |(_, bs)| score > bs) {
                    best = Some((engram, score));
                }
            }
        }
        best.map(|(e, _)| e)
    }

    /// Chronological replay order — forward temporal sequence (Wilson & McNaughton 1994).
    ///
    /// SWR replay during NREM sleep reactivates experiences in the ORDER they were encoded
    /// (oldest-first), not recency-first. The consolidation loop should call this instead of
    /// `recent()` so that causal chains are replayed coherently into the cortex.
    pub fn replay_sequence(&self, life_id: uuid::Uuid, n: usize) -> Vec<&Engram> {
        let mut v: Vec<_> = self.engrams_for_life(life_id).collect();
        v.sort_by_key(|e| std::cmp::Reverse(e.encoded_at_tick));
        v.truncate(n);
        v.reverse(); // oldest-first = forward temporal replay
        v
    }
}
