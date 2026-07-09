//! Crystallization — write-time systems consolidation into lattice coordinates.
//!
//! # Why this exists
//! Existing sleep consolidation blends float centroids. That keeps consolidated knowledge in the
//! same float space the recall hot path is trying to escape. Biologically, systems consolidation
//! (hippocampus → neocortex over sleep, McClelland's CLS) doesn't just average — it *re-files*
//! memories into a structured cortical map where related concepts sit near each other and can be
//! reached without replaying the episode.
//!
//! Crystallization is that re-filing step for FluctlightDB: during consolidation each concept is
//! assigned a **stable lattice address** (see [`crate::lattice`]) derived from its semantic scalar
//! and its structural signature. Once crystallized, a concept is *content-addressable* — recall is
//! coarse/fine geometry on the lattice, no embedding comparison, no episode replay. Paraphrases
//! with nearby semantics land in the same coarse cell; structurally distinct facts separate on the
//! Structure axis.

use serde::{Deserialize, Serialize};

use crate::lattice::{Axis, Lattice, LatticeCode, LatticeStore};

/// A crystallized concept: its id plus the lattice address it consolidated to.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Crystal {
    pub concept_id: String,
    pub semantic_scalar: f64,
    pub structure_signature: u64,
}

/// The consolidated cortical map: lattice-addressed concepts.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Crystallizer {
    lattice: Lattice,
    store: LatticeStore,
    crystals: Vec<Crystal>,
}

impl Crystallizer {
    pub fn len(&self) -> usize {
        self.crystals.len()
    }

    pub fn is_empty(&self) -> bool {
        self.crystals.is_empty()
    }

    fn code_for(&self, semantic_scalar: f64, structure_signature: u64) -> LatticeCode {
        let mut code = self
            .lattice
            .encode_with_semantic_position(semantic_scalar, &[]);
        code.axes.insert(
            Axis::Structure,
            self.lattice.encode_structure(structure_signature),
        );
        code
    }

    /// Consolidate a concept into the cortical map at its lattice address.
    pub fn crystallize(
        &mut self,
        concept_id: impl Into<String>,
        semantic_scalar: f64,
        structure_signature: u64,
    ) {
        let concept_id = concept_id.into();
        let code = self.code_for(semantic_scalar, structure_signature);
        self.store.insert(concept_id.clone(), code);
        self.crystals.push(Crystal {
            concept_id,
            semantic_scalar,
            structure_signature,
        });
    }

    /// Coarse (gist) recall: nearest consolidated concepts by semantic neighbourhood.
    pub fn recall_gist(&self, semantic_scalar: f64, k: usize) -> Vec<(String, f32)> {
        let cue = self.code_for(semantic_scalar, 0);
        self.store
            .query_coarse(&cue, self.lattice_scales(), &[(Axis::Semantic, 1.0)], k)
    }

    /// Fine + structural recall: same meaning AND same structure.
    pub fn recall_exact_structure(
        &self,
        semantic_scalar: f64,
        structure_signature: u64,
        k: usize,
    ) -> Vec<(String, f32)> {
        let cue = self.code_for(semantic_scalar, structure_signature);
        self.store.query_coarse(
            &cue,
            self.lattice_scales(),
            &[(Axis::Semantic, 1.0), (Axis::Structure, 2.0)],
            k,
        )
    }

    fn lattice_scales(&self) -> &[u32] {
        &self.lattice.scales
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paraphrases_crystallize_near_each_other() {
        let mut c = Crystallizer::default();
        // Two paraphrases → near semantic scalars; an unrelated fact far away.
        c.crystallize("upgrade_a", 0.400_000, 111);
        c.crystallize("upgrade_b", 0.400_003, 222);
        c.crystallize("weather", 0.850_000, 333);

        let gist = c.recall_gist(0.400_001, 2);
        let ids: Vec<&str> = gist.iter().map(|(id, _)| id.as_str()).collect();
        assert!(
            ids.contains(&"upgrade_a") && ids.contains(&"upgrade_b"),
            "gist: {ids:?}"
        );
        assert!(
            !ids.contains(&"weather"),
            "unrelated leaked into gist: {ids:?}"
        );
    }

    #[test]
    fn structure_axis_separates_same_meaning_different_relation() {
        let mut c = Crystallizer::default();
        c.crystallize("user_upgraded_plan", 0.5, 0xAAAA);
        c.crystallize("plan_upgraded_user", 0.5, 0xBBBB);
        // Query with one structure signature → the matching relation ranks first.
        let hits = c.recall_exact_structure(0.5, 0xAAAA, 2);
        assert_eq!(
            hits[0].0, "user_upgraded_plan",
            "structure disambiguation failed: {hits:?}"
        );
    }

    #[test]
    fn crystallized_concepts_are_content_addressable_without_replay() {
        let mut c = Crystallizer::default();
        for i in 0..200u64 {
            c.crystallize(format!("c{i}"), (i as f64) / 200.0, i.wrapping_mul(7));
        }
        // Recall by address alone (no embedding, no episode).
        let hits = c.recall_gist(100.0 / 200.0, 1);
        assert_eq!(hits[0].0, "c100");
        assert_eq!(c.len(), 200);
    }
}
