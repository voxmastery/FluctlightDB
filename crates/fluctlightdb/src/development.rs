use serde::{Deserialize, Serialize};

/// Developmental stages — baby brain → AGI path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[repr(u8)]
pub enum DevStage {
    /// Reflexes only — brainstem
    Embryonic = 0,
    /// Massive encoding, minimal pruning
    Newborn = 1,
    /// Synaptic blooming, high salience weight
    Infant = 2,
    /// Pruning active, schemas forming
    Child = 3,
    /// PFC unlocks — planning, inhibition
    Adolescent = 4,
    /// Stable engrams, efficient completion
    Adult = 5,
    /// Cross-domain consolidation boost
    Expert = 6,
}

impl DevStage {
    pub fn from_u8(v: u8) -> Self {
        match v {
            0 => Self::Embryonic,
            1 => Self::Newborn,
            2 => Self::Infant,
            3 => Self::Child,
            4 => Self::Adolescent,
            5 => Self::Adult,
            6 => Self::Expert,
            _ => Self::Expert,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Embryonic => "embryonic",
            Self::Newborn => "newborn",
            Self::Infant => "infant",
            Self::Child => "child",
            Self::Adolescent => "adolescent",
            Self::Adult => "adult",
            Self::Expert => "expert",
        }
    }

    /// Myelination factor — faster activation spread at later stages.
    pub fn myelination(self) -> f32 {
        match self {
            Self::Embryonic => 0.1,
            Self::Newborn => 0.2,
            Self::Infant => 0.35,
            Self::Child => 0.5,
            Self::Adolescent => 0.7,
            Self::Adult => 0.85,
            Self::Expert => 1.0,
        }
    }

    /// Pruning threshold during sleep — higher = more aggressive prune.
    pub fn prune_threshold(self) -> f32 {
        match self {
            Self::Embryonic => 0.001,
            Self::Newborn => 0.005,
            Self::Infant => 0.01,
            Self::Child => 0.02,
            Self::Adolescent => 0.03,
            Self::Adult => 0.04,
            Self::Expert => 0.05,
        }
    }

    pub fn max_synapses(self) -> usize {
        match self {
            Self::Embryonic => 1_000,
            Self::Newborn => 50_000,
            Self::Infant => 200_000,
            Self::Child => 500_000,
            Self::Adolescent => 1_000_000,
            Self::Adult => 2_000_000,
            Self::Expert => 5_000_000,
        }
    }

    pub fn next(self) -> Option<Self> {
        match self {
            Self::Embryonic => Some(Self::Newborn),
            Self::Newborn => Some(Self::Infant),
            Self::Infant => Some(Self::Child),
            Self::Child => Some(Self::Adolescent),
            Self::Adolescent => Some(Self::Adult),
            Self::Adult => Some(Self::Expert),
            Self::Expert => None,
        }
    }
}

/// Metrics that drive automatic maturation (no manual stage flags).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct DevelopmentMetrics {
    pub experience_count: u64,
    pub sleep_cycles: u64,
    pub ticks: u64,
    pub total_salience: f32,
    pub pruned_synapses: u64,
    pub deaths_survived: u32,
}

/// Thresholds to advance — biology-inspired, tuned for agents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageThreshold {
    pub min_experiences: u64,
    pub min_sleep_cycles: u64,
    pub min_ticks: u64,
    pub min_pruned: u64,
}

impl StageThreshold {
    /// Minimum metrics required to **enter** this stage (from previous).
    pub fn to_enter(stage: DevStage) -> Option<Self> {
        Some(match stage {
            DevStage::Embryonic => Self {
                min_experiences: 0,
                min_sleep_cycles: 0,
                min_ticks: 0,
                min_pruned: 0,
            },
            DevStage::Newborn => Self {
                min_experiences: 0,
                min_sleep_cycles: 0,
                min_ticks: 1,
                min_pruned: 0,
            },
            DevStage::Infant => Self {
                min_experiences: 3,
                min_sleep_cycles: 0,
                min_ticks: 3,
                min_pruned: 0,
            },
            DevStage::Child => Self {
                min_experiences: 15,
                min_sleep_cycles: 1,
                min_ticks: 20,
                min_pruned: 0,
            },
            DevStage::Adolescent => Self {
                min_experiences: 50,
                min_sleep_cycles: 3,
                min_ticks: 100,
                min_pruned: 10,
            },
            DevStage::Adult => Self {
                min_experiences: 150,
                min_sleep_cycles: 8,
                min_ticks: 300,
                min_pruned: 100,
            },
            DevStage::Expert => Self {
                min_experiences: 500,
                min_sleep_cycles: 20,
                min_ticks: 1000,
                min_pruned: 500,
            },
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DevelopmentState {
    pub stage: DevStage,
    pub metrics: DevelopmentMetrics,
    pub stage_entered_at_tick: u64,
}

impl Default for DevelopmentState {
    fn default() -> Self {
        Self {
            stage: DevStage::Embryonic,
            metrics: DevelopmentMetrics::default(),
            stage_entered_at_tick: 0,
        }
    }
}

impl DevelopmentState {
    /// Called after every experience, sleep, and tick — **automatic growth**.
    pub fn maybe_advance(&mut self) -> bool {
        let Some(next) = self.stage.next() else {
            return false;
        };
        let Some(th) = StageThreshold::to_enter(next) else {
            return false;
        };
        let m = &self.metrics;
        if m.experience_count >= th.min_experiences
            && m.sleep_cycles >= th.min_sleep_cycles
            && m.ticks >= th.min_ticks
            && m.pruned_synapses >= th.min_pruned
        {
            self.stage = next;
            self.stage_entered_at_tick = m.ticks;
            return true;
        }
        false
    }

    pub fn on_experience(&mut self, salience: f32) {
        self.metrics.experience_count += 1;
        self.metrics.total_salience += salience;
        self.metrics.ticks += 1;
        self.maybe_advance();
    }

    pub fn on_sleep(&mut self, pruned: u32) {
        self.metrics.sleep_cycles += 1;
        self.metrics.pruned_synapses += pruned as u64;
        self.metrics.ticks += 1;
        self.maybe_advance();
    }

    pub fn on_tick(&mut self) {
        self.metrics.ticks += 1;
        self.maybe_advance();
    }

    pub fn pfc_unlocked(&self) -> bool {
        self.stage >= DevStage::Adolescent
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_advances_embryonic_to_newborn() {
        let mut dev = DevelopmentState::default();
        assert_eq!(dev.stage, DevStage::Embryonic);
        dev.on_tick();
        assert_eq!(dev.stage, DevStage::Newborn);
    }

    #[test]
    fn auto_advances_with_experiences() {
        let mut dev = DevelopmentState {
            stage: DevStage::Newborn,
            ..Default::default()
        };
        for _ in 0..3 {
            dev.on_experience(0.5);
        }
        assert_eq!(dev.stage, DevStage::Infant);
    }
}
