use serde::{Deserialize, Serialize};

/// Salience ledger — emotional / importance tagging (amygdala).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Amygdala {
    pub tags: Vec<SalienceTag>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SalienceTag {
    pub engram_id: uuid::Uuid,
    pub weight: f32,
    pub decay_rate: f32,
}

impl Amygdala {
    pub fn tag(&mut self, engram_id: uuid::Uuid, weight: f32) {
        self.tags.push(SalienceTag {
            engram_id,
            weight: weight.clamp(0.0, 1.0),
            decay_rate: 0.01,
        });
    }

    pub fn weight_for(&self, engram_id: uuid::Uuid) -> f32 {
        self.tags
            .iter()
            .filter(|t| t.engram_id == engram_id)
            .map(|t| t.weight)
            .sum::<f32>()
            .min(1.0)
    }

    pub fn decay(&mut self) {
        for tag in &mut self.tags {
            tag.weight = (tag.weight - tag.decay_rate).max(0.0);
        }
        self.tags.retain(|t| t.weight > 0.01);
    }
}
