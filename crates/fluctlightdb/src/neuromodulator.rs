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

    /// ACh-gated encoding/retrieval mode switch (Hasselmo 2006).
    ///
    /// High ACh (≥0.6): encoding mode — hippocampus leads, cortical feedback suppressed,
    /// EC→DG→CA3 pathway dominates, new patterns are laid down without interference.
    ///
    /// Low ACh (<0.6): retrieval mode — CA3 recurrent collaterals dominate, cortex drives
    /// top-down priming, pattern completion from partial cues takes over.
    ///
    /// Biologically: basal forebrain cholinergic neurons release ACh during novelty/exploration
    /// (encoding) and withdraw during quiet wakefulness/recall (retrieval).
    pub fn is_encoding(&self) -> bool {
        self.acetylcholine >= 0.6
    }

    /// Strength of CA3 recurrent collateral drive (retrieval gate).
    /// Inversely proportional to ACh: low ACh → strong recurrent → pattern completion.
    pub fn ca3_recurrent_gain(&self) -> f32 {
        // At ACh=0 → gain=1.0 (full recurrent); at ACh=1 → gain=0.1 (suppressed).
        1.0 - 0.9 * self.acetylcholine
    }

    /// Raise ACh on novelty (new input, no matching engram) — switches to encoding mode.
    pub fn on_novelty(&mut self) {
        self.acetylcholine = (self.acetylcholine + 0.15).min(1.0);
        self.norepinephrine = (self.norepinephrine + 0.1).min(1.0);
    }

    /// Lower ACh on deliberate retrieval request — opens CA3 recurrent collaterals.
    pub fn on_retrieval(&mut self) {
        self.acetylcholine = (self.acetylcholine - 0.12).max(0.1);
    }

    /// Tick decay — ACh drifts back toward baseline (0.7) between events.
    pub fn tick_decay(&mut self) {
        let baseline = 0.7_f32;
        self.acetylcholine += (baseline - self.acetylcholine) * 0.05;
        self.dopamine += (0.5 - self.dopamine) * 0.03;
        self.norepinephrine += (0.3 - self.norepinephrine) * 0.04;
        self.serotonin += (0.5 - self.serotonin) * 0.02;
    }
}
