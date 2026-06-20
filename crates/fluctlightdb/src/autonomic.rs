use serde::{Deserialize, Serialize};

use crate::types::SleepReport;

/// Brainstem autonomic loop — background heartbeat + auto-sleep.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AutonomicConfig {
    pub auto_sleep: bool,
    /// Ticks between automatic sleep cycles when idle.
    pub ticks_per_sleep: u64,
    /// Fraction of stage max synapses that triggers pressure sleep (0–1).
    pub synapse_pressure_ratio: f32,
    /// Max auto-sleeps within one sleep window (rate limit).
    #[serde(default = "default_max_auto_sleeps_per_hour")]
    pub max_auto_sleeps_per_hour: u32,
    /// Tick window for auto-sleep rate limiting (~1 hour if tick ≈ 1s).
    #[serde(default = "default_sleep_window_ticks")]
    pub sleep_window_ticks: u64,
}

fn default_max_auto_sleeps_per_hour() -> u32 {
    6
}

fn default_sleep_window_ticks() -> u64 {
    3600
}

impl Default for AutonomicConfig {
    fn default() -> Self {
        Self {
            auto_sleep: true,
            ticks_per_sleep: 360,
            synapse_pressure_ratio: 0.92,
            max_auto_sleeps_per_hour: 6,
            sleep_window_ticks: 3600,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct AutonomicState {
    pub config: AutonomicConfig,
    pub ticks_since_sleep: u64,
    pub total_ticks: u64,
    pub auto_sleeps: u64,
    #[serde(default)]
    pub sleeps_in_window: u32,
    #[serde(default)]
    pub window_start_tick: u64,
}

impl AutonomicState {
    pub fn new() -> Self {
        Self {
            config: AutonomicConfig::default(),
            ..Default::default()
        }
    }

    pub fn on_tick(&mut self) {
        self.ticks_since_sleep += 1;
        self.total_ticks += 1;
    }

    pub fn on_sleep(&mut self) {
        self.ticks_since_sleep = 0;
        self.auto_sleeps += 1;
        self.sleeps_in_window += 1;
    }

    pub fn roll_sleep_window(&mut self, total_ticks: u64) {
        if total_ticks.saturating_sub(self.window_start_tick) >= self.config.sleep_window_ticks {
            self.window_start_tick = total_ticks;
            self.sleeps_in_window = 0;
        }
    }

    pub fn should_sleep(&self, synapse_count: usize, max_synapses: usize) -> bool {
        if !self.config.auto_sleep {
            return false;
        }
        let pressure = synapse_count as f32 / max_synapses.max(1) as f32;
        let pressure_sleep = pressure >= self.config.synapse_pressure_ratio;
        let interval_sleep = self.ticks_since_sleep >= self.config.ticks_per_sleep;
        if !(pressure_sleep || interval_sleep) {
            return false;
        }
        // Rate-limit interval-driven sleep; synapse pressure always allowed.
        if !pressure_sleep
            && self.sleeps_in_window >= self.config.max_auto_sleeps_per_hour
        {
            return false;
        }
        true
    }

    pub fn synapse_pressure(&self, synapse_count: usize, max_synapses: usize) -> f32 {
        (synapse_count as f32 / max_synapses.max(1) as f32).clamp(0.0, 1.0)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TickReport {
    pub tick: u64,
    pub stage: String,
    pub ticks_since_sleep: u64,
    pub synapse_pressure: f32,
    pub slept: bool,
    pub sleep_report: Option<SleepReport>,
    pub stage_advanced: bool,
}
