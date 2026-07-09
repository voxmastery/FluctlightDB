//! Forgetting & Interference Control — homeostatic memory hygiene for open-ended agents.
//!
//! # Why this exists
//! An AGI-scale memory that never forgets drowns in interference: old traces collide with new ones,
//! recall precision collapses, and capacity is wasted on noise. The brain forgets *adaptively* —
//! the Ebbinghaus forgetting curve `R = exp(-Δt / S)` where stability `S` grows with salience and
//! spaced rehearsal (Ebbinghaus 1885; Bjork's "desirable difficulty"). Important, rehearsed, or
//! corroborated memories persist; incidental ones decay.
//!
//! This module models a memory trace's retention over time, strengthens it on access (spaced
//! repetition), estimates interference from crowding, and drives **elastic capacity**: when the
//! live set approaches the lattice's addressable load, the [`LoadController`] recommends growing
//! the lattice (adding a co-prime scale) instead of overwriting — neurogenesis, not amnesia.

use serde::{Deserialize, Serialize};

/// A single memory's decay state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemoryTrace {
    pub created_tick: u64,
    pub last_access_tick: u64,
    /// Base salience in [0,1] — emotional/goal relevance at encoding.
    pub salience: f32,
    /// Number of successful rehearsals (recalls). Drives spaced-repetition stability.
    pub rehearsals: u32,
}

impl MemoryTrace {
    pub fn new(now: u64, salience: f32) -> Self {
        Self {
            created_tick: now,
            last_access_tick: now,
            salience: salience.clamp(0.0, 1.0),
            rehearsals: 0,
        }
    }

    /// Stability `S`: higher → slower decay. Grows with salience and (super-linearly) rehearsals.
    pub fn stability(&self) -> f32 {
        let base = 10.0 + 90.0 * self.salience; // salient memories start far more stable
        base * (1.0 + self.rehearsals as f32).powf(1.3)
    }

    /// Retention `R = exp(-Δt / S)` in (0,1]. `now` must be ≥ `last_access_tick`.
    pub fn retention(&self, now: u64) -> f32 {
        let dt = now.saturating_sub(self.last_access_tick) as f32;
        (-dt / self.stability()).exp()
    }

    /// A recall event: spaced repetition strengthens the trace and resets the decay clock.
    pub fn rehearse(&mut self, now: u64) {
        self.rehearsals += 1;
        self.last_access_tick = now;
    }

    /// Should this trace be evicted? Retention below `threshold` and never strongly encoded.
    pub fn should_forget(&self, now: u64, threshold: f32) -> bool {
        self.retention(now) < threshold && self.salience < 0.85
    }
}

/// Estimate interference: how much a new trace crowds existing ones with similar signatures.
/// `similarity` in [0,1]; more crowding → higher interference (recall precision cost).
pub fn interference(similar_neighbors: &[f32]) -> f32 {
    if similar_neighbors.is_empty() {
        return 0.0;
    }
    // Interference saturates: each additional near-duplicate hurts less than the first.
    let crowd: f32 = similar_neighbors.iter().map(|s| s.clamp(0.0, 1.0)).sum();
    1.0 - (-crowd).exp()
}

/// Drives elastic capacity: recommend lattice growth before crowding degrades recall.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadController {
    /// Fraction of addressable capacity at which to grow (e.g. 0.6).
    pub grow_threshold: f32,
    /// Co-prime scales available to add, in preference order.
    pub growth_scales: Vec<u32>,
}

impl Default for LoadController {
    fn default() -> Self {
        Self {
            grow_threshold: 0.6,
            growth_scales: vec![37, 41, 43, 47, 53, 59, 61, 67],
        }
    }
}

impl LoadController {
    /// Given live memory count and current lattice capacity, return the next scale to add (if any).
    pub fn recommend_growth(&self, live_count: u64, capacity: u128) -> Option<u32> {
        if capacity == 0 {
            return self.growth_scales.first().copied();
        }
        let load = live_count as f64 / capacity as f64;
        if load as f32 >= self.grow_threshold {
            self.growth_scales.first().copied()
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retention_decays_monotonically() {
        let t = MemoryTrace::new(0, 0.3);
        let r10 = t.retention(10);
        let r100 = t.retention(100);
        let r1000 = t.retention(1000);
        assert!(
            r10 > r100 && r100 > r1000,
            "not monotonic: {r10} {r100} {r1000}"
        );
        assert!(r10 <= 1.0 && r1000 > 0.0);
    }

    #[test]
    fn salience_slows_decay() {
        let low = MemoryTrace::new(0, 0.1);
        let high = MemoryTrace::new(0, 0.9);
        assert!(high.retention(500) > low.retention(500));
    }

    #[test]
    fn rehearsal_strengthens_and_resets_clock() {
        let mut t = MemoryTrace::new(0, 0.3);
        let before = t.stability();
        t.rehearse(200);
        assert!(t.stability() > before, "rehearsal should raise stability");
        // Right after rehearsal, retention is ~1 (clock reset).
        assert!(t.retention(200) > 0.99);
    }

    #[test]
    fn incidental_memories_are_forgotten_salient_ones_kept() {
        let incidental = MemoryTrace::new(0, 0.2);
        let core = MemoryTrace::new(0, 0.95);
        let now = 5000;
        assert!(
            incidental.should_forget(now, 0.2),
            "incidental should be forgettable"
        );
        assert!(
            !core.should_forget(now, 0.2),
            "core memory must never be forgotten"
        );
    }

    #[test]
    fn interference_saturates() {
        let none = interference(&[]);
        let one = interference(&[0.9]);
        let many = interference(&[0.9, 0.9, 0.9, 0.9]);
        assert_eq!(none, 0.0);
        assert!(one > 0.0 && many > one);
        assert!(many < 1.0, "interference must stay bounded: {many}");
    }

    #[test]
    fn load_controller_grows_before_overflow() {
        let lc = LoadController::default();
        // Under threshold → no growth.
        assert!(lc.recommend_growth(10, 1000).is_none());
        // Over threshold → recommend the first growth scale.
        assert_eq!(lc.recommend_growth(700, 1000), Some(37));
    }
}
