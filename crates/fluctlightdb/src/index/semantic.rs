//! Semantic top-k index — cosine similarity seeds without full brain scan.

use uuid::Uuid;

use crate::semantic::SemanticField;

/// Similarity floor — below this a candidate is not worth seeding into recall.
const MIN_SIM: f32 = 0.05;

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

/// Top-k engram ids by cosine similarity to `cue_vector`, **returned in rank order**
/// (most similar first).
///
/// History: this used a `BinaryHeap` as if it were a min-heap. `BinaryHeap::peek()`
/// returns the *greatest* element, so the eviction branch popped the best candidate
/// on every improvement and the function returned the k *worst* matches above the
/// floor. It then sorted the survivors by `id.to_string()`, discarding the ranking
/// it had just computed — so even a correct heap would have handed `hybrid_candidates`
/// an arbitrarily ordered set. Both are fixed here; rank order is now part of the
/// contract, because callers truncate to a cap and must drop the *weakest* candidates.
pub fn semantic_top_k(semantic: &SemanticField, cue_vector: &[f32], k: usize) -> Vec<Uuid> {
    if cue_vector.is_empty() || k == 0 {
        return Vec::new();
    }
    let mut scored: Vec<(f32, Uuid)> = semantic
        .engram_vectors
        .iter()
        .map(|(id, stored)| (cosine(cue_vector, stored), *id))
        .filter(|(sim, _)| *sim >= MIN_SIM)
        .collect();
    // `total_cmp` rather than `partial_cmp(..).unwrap_or(Equal)`: a NaN similarity from a
    // malformed stored vector previously made the ordering non-transitive, which can panic
    // `sort_by` on a strict-weak-ordering violation. Ties break on id for determinism.
    scored.sort_by(|a, b| b.0.total_cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    scored.truncate(k);
    scored.into_iter().map(|(_, id)| id).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::semantic::SemanticField;

    /// Ten unit vectors whose cosine against the cue rises with `i`, so the true top-3
    /// is unambiguously [9, 8, 7].
    fn ascending_field() -> (SemanticField, Vec<Uuid>) {
        let mut sf = SemanticField::default();
        let mut ids = Vec::new();
        for i in 0..10u32 {
            let c = 0.10 + 0.08 * i as f32;
            let id = Uuid::from_u128(i as u128 + 1);
            sf.engram_vectors.insert(id, vec![c, (1.0 - c * c).sqrt()]);
            ids.push(id);
        }
        (sf, ids)
    }

    /// Before the fix this returned the two WORST candidates plus one good one:
    /// the heap filled with ids 0,1,2; `peek()` yielded id2 (the best of the three);
    /// id3 beat it, so the *best* was evicted — and so on, leaving {id0, id1, id9}.
    /// The signature is unchanged, so this fails behaviourally, not by failing to compile.
    #[test]
    fn top_k_returns_highest_cosine_in_rank_order() {
        let (sf, ids) = ascending_field();
        let got = semantic_top_k(&sf, &[1.0, 0.0], 3);
        assert_eq!(
            got,
            vec![ids[9], ids[8], ids[7]],
            "semantic_top_k must return the three most similar engrams, best first"
        );
    }

    #[test]
    fn top_k_is_rank_ordered_not_id_ordered() {
        let (sf, ids) = ascending_field();
        let got = semantic_top_k(&sf, &[1.0, 0.0], 10);
        let mut by_rank = got.clone();
        by_rank.sort_by_key(|id| std::cmp::Reverse(ids.iter().position(|x| x == id).unwrap()));
        assert_eq!(got, by_rank, "results must come back strongest-first");
    }

    /// A malformed stored vector must not make the comparator non-transitive.
    #[test]
    fn nan_similarity_does_not_panic_the_sort() {
        let (mut sf, _) = ascending_field();
        sf.engram_vectors
            .insert(Uuid::from_u128(999), vec![f32::NAN, f32::NAN]);
        let _ = semantic_top_k(&sf, &[1.0, 0.0], 5);
    }

    #[test]
    fn below_floor_candidates_are_dropped() {
        let mut sf = SemanticField::default();
        sf.engram_vectors.insert(Uuid::from_u128(1), vec![0.0, 1.0]); // orthogonal -> sim 0
        assert!(semantic_top_k(&sf, &[1.0, 0.0], 4).is_empty());
    }
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
