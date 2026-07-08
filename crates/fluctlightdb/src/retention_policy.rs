//! Retention policy DSL — declarative memory hygiene for agents.
//!
//! Example: retain 30 days unless verified; prune low-salience CHORUS traces on consolidate.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Agent-facing retention rule.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RetentionPolicy {
    /// Wall-clock days to keep non-verified memories (None = no age limit).
    #[serde(default)]
    pub retain_days: Option<u32>,
    /// Verified / ledger memories survive age pruning.
    #[serde(default = "default_unless_verified")]
    pub unless_verified: bool,
    /// Minimum salience to survive pruning.
    #[serde(default = "default_min_salience")]
    pub min_salience: f32,
    /// Ticks ≈ seconds in autonomic loop; used when wall clock unavailable.
    #[serde(default = "default_ticks_per_day")]
    pub ticks_per_day: u64,
}

fn default_unless_verified() -> bool {
    true
}

fn default_min_salience() -> f32 {
    0.12
}

fn default_ticks_per_day() -> u64 {
    86_400
}

impl Default for RetentionPolicy {
    fn default() -> Self {
        Self {
            retain_days: None,
            unless_verified: true,
            min_salience: default_min_salience(),
            ticks_per_day: default_ticks_per_day(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RetentionState {
    pub policy: RetentionPolicy,
    pub engram_ticks: HashMap<Uuid, u64>,
}

impl RetentionState {
    pub fn record_engram(&mut self, id: Uuid, tick: u64) {
        self.engram_ticks.insert(id, tick);
    }

    pub fn set_policy(&mut self, policy: RetentionPolicy) {
        self.policy = policy;
    }

    pub fn should_prune_engram(
        &self,
        engram_id: Uuid,
        now_tick: u64,
        salience: f32,
        verified: bool,
    ) -> bool {
        if self.policy.unless_verified && verified {
            return false;
        }
        if salience >= self.policy.min_salience.max(0.85) {
            return false;
        }
        if salience < self.policy.min_salience {
            return true;
        }
        let Some(days) = self.policy.retain_days else {
            return false;
        };
        let encoded = self.engram_ticks.get(&engram_id).copied().unwrap_or(now_tick);
        let age_ticks = now_tick.saturating_sub(encoded);
        age_ticks > days as u64 * self.policy.ticks_per_day
    }
}

impl Default for RetentionState {
    fn default() -> Self {
        Self {
            policy: RetentionPolicy::default(),
            engram_ticks: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RetentionReport {
    pub pruned_engrams: u32,
    pub pruned_chorus: u32,
}
