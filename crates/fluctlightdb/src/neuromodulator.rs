use serde::{Deserialize, Serialize};

/// Global neuromodulator state — the brain's control plane (Doya mapping).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Neuromodulators {
    /// Reward prediction error — gates strengthening (dopamine).
    pub dopamine: f32,
    /// Learning rate / encoding vs retrieval mode (acetylcholine).
    pub acetylcholine: f32,
    /// Arousal / unexpected uncertainty (norepinephrine).
    pub norepinephrine: f32,
    /// Temporal discount / patience (serotonin).
    pub serotonin: f32,
}

impl Default for Neuromodulators {
    fn default() -> Self {
        Self {
            dopamine: 0.5,
            acetylcholine: 0.7,
            norepinephrine: 0.3,
            serotonin: 0.5,
        }
    }
}

impl Neuromodulators {
    pub fn on_reward(&mut self, magnitude: f32) {
        self.dopamine = (self.dopamine + magnitude * 0.2).clamp(0.0, 1.0);
    }

    pub fn on_surprise(&mut self, magnitude: f32) {
        self.norepinephrine = (self.norepinephrine + magnitude * 0.25).clamp(0.0, 1.0);
        self.acetylcholine = (self.acetylcholine + magnitude * 0.1).clamp(0.0, 1.0);
    }

    pub fn on_sleep(&mut self) {
        self.dopamine *= 0.85;
        self.norepinephrine *= 0.7;
        self.acetylcholine = (self.acetylcholine + 0.05).min(1.0);
    }

    /// Plasticity allowed when salient or surprising (neuromodulatory gate).
    pub fn plasticity_gate(&self, salience: f32) -> f32 {
        let base = self.dopamine * 0.4 + self.acetylcholine * 0.3 + self.norepinephrine * 0.3;
        (base + salience * 0.5).clamp(0.05, 1.0)
    }
}
