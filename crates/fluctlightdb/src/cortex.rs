use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::id::NeuronId;
use crate::tokenize::tokenize;

/// Slow semantic store — gradual statistical knowledge (McClelland CLS).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Cortex {
    pub facts: HashMap<String, f32>,
    pub token_strength: HashMap<NeuronId, f32>,
    /// Running semantic centroid (neocortical consolidation from EC vectors).
    #[serde(default)]
    pub semantic_centroid: Vec<f32>,
    #[serde(default)]
    pub semantic_strength: f32,
}

impl Cortex {
    pub fn consolidate_from_text(&mut self, text: &str, weight: f32) {
        for token in tokenize(text) {
            *self.facts.entry(token).or_insert(0.0) += weight;
        }
    }

    pub fn consolidate_neurons(&mut self, neurons: &[NeuronId], weight: f32) {
        for n in neurons {
            *self.token_strength.entry(*n).or_insert(0.0) += weight;
        }
    }

    pub fn consolidate_semantic(&mut self, vector: &[f32], weight: f32) {
        if vector.is_empty() {
            return;
        }
        if self.semantic_centroid.len() != vector.len() {
            self.semantic_centroid = vector.to_vec();
            self.semantic_strength = weight;
            return;
        }
        let alpha = (weight * 0.2).clamp(0.01, 0.35);
        for (i, &v) in vector.iter().enumerate() {
            self.semantic_centroid[i] = self.semantic_centroid[i] * (1.0 - alpha) + v * alpha;
        }
        self.semantic_strength += weight;
    }

    pub fn fact_boost(&self, cue: &str) -> f32 {
        let tokens = tokenize(cue);
        if tokens.is_empty() {
            return 0.0;
        }
        let sum: f32 = tokens
            .iter()
            .filter_map(|t| self.facts.get(t))
            .sum();
        sum / tokens.len() as f32
    }

    pub fn semantic_boost(&self, cue_vector: Option<&[f32]>) -> f32 {
        let Some(cue) = cue_vector else {
            return 0.0;
        };
        if cue.is_empty() || self.semantic_centroid.is_empty() {
            return 0.0;
        }
        crate::semantic::cosine_similarity(cue, &self.semantic_centroid)
            * self.semantic_strength.sqrt().min(3.0)
            * 0.15
    }
}
