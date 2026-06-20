//! Entorhinal semantic gateway — bridges distributional embeddings into the hippocampal circuit.
//!
//! Biology: EC receives high-level conceptual features from perirhinal/anterior temporal cortex;
//! grid-like codes route into DG/CA3. Sleep consolidates semantic structure into neocortex (CLS).
//! Vectors assist encoding and recall; activation through synapses remains the mind.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::engram::Engram;
use crate::hippocampus::Hippocampus;
use crate::id::NeuronId;

/// Default embedding dimension when callers use hash-projection fallback (MiniLM-compatible).
pub const DEFAULT_SEMANTIC_DIM: usize = 384;

/// Number of random hyperplane bands per vector → sparse EC semantic neurons.
const EC_PROJECTION_BANDS: usize = 16;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct SemanticField {
    pub dimension: u16,
    pub engram_vectors: HashMap<Uuid, Vec<f32>>,
    pub ec_semantic_neurons: HashMap<Uuid, Vec<NeuronId>>,
    pub centroids: Vec<SemanticCentroid>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SemanticCentroid {
    pub id: Uuid,
    pub vector: Vec<f32>,
    pub label_tokens: Vec<String>,
    pub strength: f32,
    pub engram_ids: Vec<Uuid>,
}

impl SemanticField {
    pub fn register_engram(
        &mut self,
        engram_id: Uuid,
        life_id: Uuid,
        vector: Vec<f32>,
    ) -> Vec<NeuronId> {
        if vector.is_empty() {
            return Vec::new();
        }
        if self.dimension == 0 {
            self.dimension = vector.len().min(u16::MAX as usize) as u16;
        }
        let ec = project_to_ec_neurons(&vector, life_id, engram_id);
        self.engram_vectors.insert(engram_id, vector);
        self.ec_semantic_neurons.insert(engram_id, ec.clone());
        ec
    }

    pub fn cue_ec_neurons(&self, cue_vector: &[f32], life_id: Uuid, cue_id: Uuid) -> Vec<NeuronId> {
        if cue_vector.is_empty() {
            return Vec::new();
        }
        project_to_ec_neurons(cue_vector, life_id, cue_id)
    }

    /// Cosine similarity recall boost per engram (hippocampal index lookup).
    pub fn engram_similarities(
        &self,
        cue_vector: &[f32],
        engrams: &[Engram],
    ) -> HashMap<Uuid, f32> {
        let mut out = HashMap::new();
        if cue_vector.is_empty() {
            return out;
        }
        for engram in engrams {
            if let Some(stored) = self.engram_vectors.get(&engram.id) {
                let sim = cosine_similarity(cue_vector, stored);
                if sim > 0.05 {
                    out.insert(engram.id, sim);
                }
            }
        }
        out
    }

    /// Consolidate semantic centroids during sleep (neocortical slow learning).
    pub fn consolidate_from_engrams(
        &mut self,
        hippocampus: &Hippocampus,
        life_id: Uuid,
        replay_ids: &[Uuid],
    ) -> u32 {
        let mut merged = 0u32;
        for engram_id in replay_ids {
            let Some(vector) = self.engram_vectors.get(engram_id).cloned() else {
                continue;
            };
            let Some(engram) = hippocampus.engrams.iter().find(|e| e.id == *engram_id) else {
                continue;
            };
            let tokens: Vec<String> = engram
                .episode
                .content
                .split_whitespace()
                .take(8)
                .map(|w| w.to_lowercase())
                .collect();

            let mut best_idx: Option<usize> = None;
            let mut best_sim = 0.72_f32;
            for (i, c) in self.centroids.iter().enumerate() {
                let sim = cosine_similarity(&vector, &c.vector);
                if sim > best_sim {
                    best_sim = sim;
                    best_idx = Some(i);
                }
            }

            if let Some(i) = best_idx {
                let c = &mut self.centroids[i];
                blend_vector_inplace(&mut c.vector, &vector, 0.15);
                c.strength = (c.strength + engram.salience * 0.1).min(10.0);
                if !c.engram_ids.contains(engram_id) {
                    c.engram_ids.push(*engram_id);
                }
                for t in tokens {
                    if !c.label_tokens.contains(&t) {
                        c.label_tokens.push(t);
                    }
                }
                merged += 1;
            } else {
                self.centroids.push(SemanticCentroid {
                    id: Uuid::new_v4(),
                    vector,
                    label_tokens: tokens,
                    strength: engram.salience,
                    engram_ids: vec![*engram_id],
                });
                merged += 1;
            }
        }
        let _ = life_id;
        merged
    }

    pub fn centroid_boost(&self, cue_vector: &[f32]) -> f32 {
        if cue_vector.is_empty() || self.centroids.is_empty() {
            return 0.0;
        }
        self.centroids
            .iter()
            .map(|c| cosine_similarity(cue_vector, &c.vector) * c.strength.sqrt())
            .fold(0.0_f32, f32::max)
            .min(1.0)
    }
}

/// Random hyperplane projection → sparse EC neuron IDs (entorhinal gateway).
pub fn project_to_ec_neurons(vector: &[f32], life_id: Uuid, seed_id: Uuid) -> Vec<NeuronId> {
    let mut neurons = Vec::with_capacity(EC_PROJECTION_BANDS);
    for band in 0..EC_PROJECTION_BANDS {
        let mut acc = 0.0_f32;
        for (i, &v) in vector.iter().enumerate() {
            let h = hash_mix(life_id, seed_id, band as u32, i as u32);
            let sign = if h & 1 == 0 { 1.0 } else { -1.0 };
            acc += v * sign;
        }
        let surface = format!("ec:sem:{band}:{acc:.4}");
        neurons.push(NeuronId::from_seeds(&["semantic", &surface]));
    }
    neurons
}

pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let n = a.len().min(b.len());
    if n == 0 {
        return 0.0;
    }
    let mut dot = 0.0_f32;
    let mut na = 0.0_f32;
    let mut nb = 0.0_f32;
    for i in 0..n {
        dot += a[i] * b[i];
        na += a[i] * a[i];
        nb += b[i] * b[i];
    }
    if na <= 1e-8 || nb <= 1e-8 {
        return 0.0;
    }
    (dot / na.sqrt() / nb.sqrt()).clamp(-1.0, 1.0)
}

fn blend_vector_inplace(dst: &mut [f32], src: &[f32], alpha: f32) {
    let n = dst.len().min(src.len());
    for i in 0..n {
        dst[i] = dst[i] * (1.0 - alpha) + src[i] * alpha;
    }
}

fn hash_mix(life: Uuid, seed: Uuid, band: u32, idx: u32) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    life.hash(&mut h);
    seed.hash(&mut h);
    band.hash(&mut h);
    idx.hash(&mut h);
    h.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cosine_identical_is_one() {
        let v = vec![1.0, 0.0, 0.0];
        assert!((cosine_similarity(&v, &v) - 1.0).abs() < 1e-5);
    }

    #[test]
    fn ec_projection_is_stable() {
        let life = Uuid::new_v4();
        let seed = Uuid::new_v4();
        let v = vec![0.1, 0.2, 0.3, 0.4];
        let a = project_to_ec_neurons(&v, life, seed);
        let b = project_to_ec_neurons(&v, life, seed);
        assert_eq!(a, b);
        assert_eq!(a.len(), EC_PROJECTION_BANDS);
    }

    #[test]
    fn semantic_field_registers_and_matches() {
        let mut field = SemanticField::default();
        let life = Uuid::new_v4();
        let id = Uuid::new_v4();
        let v = vec![0.5, 0.5, 0.0];
        field.register_engram(id, life, v.clone());
        let cue = vec![0.48, 0.52, 0.01];
        let engram = Engram {
            id,
            life_id: life,
            neurons: vec![],
            ec_neurons: vec![],
            dg_neurons: vec![],
            separation_index: 1.0,
            episode: crate::types::Episode {
                content: "test".into(),
                context: "ctx".into(),
                outcome: None,
                salience_hint: 0.5,
                semantic_vector: Some(v),
                agent_id: None,
                tenant_id: None,
                rag: None,
                provenance: None,
            },
            salience: 0.5,
            encoded_at_tick: 0,
            encoded_at_stage: 1,
            replay_count: 0,
            is_core: false,
        };
        let sims = field.engram_similarities(&cue, &[engram]);
        assert!(sims.get(&id).copied().unwrap_or(0.0) > 0.9);
    }
}
