//! Semantic top-k index — cosine similarity seeds without full brain scan.

use std::cmp::Ordering;
use std::collections::BinaryHeap;

use uuid::Uuid;

use crate::semantic::SemanticField;

struct ScoredId {
    sim: f32,
    id: Uuid,
}

impl PartialEq for ScoredId {
    fn eq(&self, other: &Self) -> bool {
        self.sim == other.sim && self.id == other.id
    }
}
impl Eq for ScoredId {}

impl PartialOrd for ScoredId {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ScoredId {
    fn cmp(&self, other: &Self) -> Ordering {
        self.sim
            .partial_cmp(&other.sim)
            .unwrap_or(Ordering::Equal)
            .then_with(|| self.id.cmp(&other.id))
    }
}

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    if a.is_empty() || b.len() != a.len() {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let na: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let nb: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if na > 0.0 && nb > 0.0 {
        dot / (na * nb)
    } else {
        0.0
    }
}

pub fn semantic_top_k(semantic: &SemanticField, cue_vector: &[f32], k: usize) -> Vec<Uuid> {
    if cue_vector.is_empty() || k == 0 {
        return Vec::new();
    }
    let mut heap: BinaryHeap<ScoredId> = BinaryHeap::new();
    for (id, stored) in &semantic.engram_vectors {
        let sim = cosine(cue_vector, stored);
        if sim < 0.05 {
            continue;
        }
        if heap.len() < k {
            heap.push(ScoredId { sim, id: *id });
        } else if let Some(min) = heap.peek() {
            if sim > min.sim {
                heap.pop();
                heap.push(ScoredId { sim, id: *id });
            }
        }
    }
    let mut out: Vec<_> = heap.into_iter().map(|s| s.id).collect();
    out.sort_by_key(|id| id.to_string());
    out
}

pub fn semantic_similarities_for(
    semantic: &SemanticField,
    cue_vector: &[f32],
    ids: &[Uuid],
) -> std::collections::HashMap<Uuid, f32> {
    let mut out = std::collections::HashMap::new();
    if cue_vector.is_empty() {
        return out;
    }
    for id in ids {
        if let Some(stored) = semantic.engram_vectors.get(id) {
            let sim = cosine(cue_vector, stored);
            if sim > 0.05 {
                out.insert(*id, sim);
            }
        }
    }
    out
}
