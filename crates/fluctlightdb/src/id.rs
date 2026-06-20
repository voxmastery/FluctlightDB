use std::hash::{Hash, Hasher};

use serde::{Deserialize, Serialize};

/// Sparse neuron identity — hash of semantic token, not a vector dimension.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
pub struct NeuronId(pub u64);

impl NeuronId {
    pub fn from_token(token: &str) -> Self {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        token.hash(&mut hasher);
        Self(hasher.finish())
    }

    pub fn from_pair(a: NeuronId, b: NeuronId) -> Self {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        a.0.hash(&mut hasher);
        b.0.hash(&mut hasher);
        Self(hasher.finish())
    }

    pub fn from_seeds(parts: &[&str]) -> Self {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        for p in parts {
            p.hash(&mut hasher);
        }
        Self(hasher.finish())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EngramId(pub uuid::Uuid);

impl EngramId {
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4())
    }
}

impl Default for EngramId {
    fn default() -> Self {
        Self::new()
    }
}
