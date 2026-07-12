//! Leaky Integrate-and-Fire neuron dynamics — the actual computational substrate.
//!
//! # Why this replaces the old NeuronId abstraction
//!
//! Previously, a "neuron" in FluctlightDB was just a `u64` hash of a token string.
//! It had no voltage, no firing threshold, no refractory period — it was a label, not a neuron.
//!
//! A real neuron integrates inputs over time and fires when its membrane potential crosses
//! a threshold. That firing IS the signal. The weight of the synapse receiving that signal
//! IS the memory. They are the same thing.
//!
//! The LIF model captures the three properties that make neurons computationally powerful:
//!
//! 1. **Integration**: inputs accumulate (membrane capacitance stores charge)
//! 2. **Threshold**: output is all-or-nothing (nonlinear — no partial spikes)
//! 3. **Refractory period**: a fired neuron cannot fire again for ~2ms (temporal sparsity)
//!
//! # The equation
//!
//! ```text
//! C_m × dV/dt = -(V - V_rest)/R_m + I_syn
//!
//! where:
//!   C_m   = membrane capacitance (nF)
//!   V     = membrane potential (mV)
//!   V_rest= resting potential (−70 mV, set by Na⁺/K⁺ pumps)
//!   R_m   = membrane resistance (MΩ, depends on number of open channels)
//!   I_syn = total synaptic current (nA)
//! ```
//!
//! Dividing through by C_m and defining τ_m = R_m × C_m gives the standard form:
//! ```text
//! τ_m × dV/dt = -(V - V_rest) + R_m × I_syn
//! ```
//!
//! When V ≥ V_thresh: spike → V reset to V_reset, neuron silent for τ_ref.
//!
//! # References
//! - Lapicque 1907 — original integrate-and-fire model
//! - Burkitt 2006 — review of IF neuron models (Biol. Cybern. 95:1-19)
//! - Abbott & Dayan 2001 — Theoretical Neuroscience, Ch. 5 (MIT Press)

use serde::{Deserialize, Serialize};

// ── Physiological constants for cortical pyramidal cells ─────────────────────

/// Resting membrane potential (mV) — set by Na⁺/K⁺-ATPase pump.
pub const V_REST: f32 = -70.0;

/// Spike threshold (mV) — Na⁺ channel activation gate opens.
pub const V_THRESH: f32 = -55.0;

/// Post-spike reset (mV) — K⁺ channels hyperpolarise the membrane briefly.
pub const V_RESET: f32 = -75.0;

/// Membrane time constant (ms) — τ_m = R_m × C_m ≈ 20ms for pyramidal cells.
pub const TAU_M_MS: f32 = 20.0;

/// Input resistance (MΩ) — determines how much current is needed to fire.
pub const R_M_MOHM: f32 = 100.0;

/// Absolute refractory period (ticks/ms) — Na⁺ channels inactivated.
pub const TAU_REF_TICKS: u64 = 2;

// ─────────────────────────────────────────────────────────────────────────────

/// A leaky integrate-and-fire neuron with full membrane dynamics.
///
/// This is the computational unit of FluctlightDB's neural substrate.
/// Unlike the old NeuronId (a label), this actually integrates, fires, and resets.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LIFNeuron {
    /// Current membrane potential (mV).
    pub v: f32,
    /// Resting potential (mV) — personalised per neuron for heterogeneity.
    pub v_rest: f32,
    /// Spike threshold (mV).
    pub v_thresh: f32,
    /// Post-spike reset potential (mV).
    pub v_reset: f32,
    /// Membrane time constant (ms).
    pub tau_m: f32,
    /// Input resistance (MΩ) — scales how much a given current moves the voltage.
    pub r_m: f32,
    /// Absolute refractory period ends at this tick — neuron cannot fire before then.
    pub refractory_until: u64,
    /// Tick of the most recent action potential (None if never fired).
    pub last_spike: Option<u64>,
    /// Cumulative spike count (rate coding / activity history).
    pub spike_count: u64,
}

impl Default for LIFNeuron {
    fn default() -> Self {
        Self {
            v: V_REST,
            v_rest: V_REST,
            v_thresh: V_THRESH,
            v_reset: V_RESET,
            tau_m: TAU_M_MS,
            r_m: R_M_MOHM,
            refractory_until: 0,
            last_spike: None,
            spike_count: 0,
        }
    }
}

impl LIFNeuron {
    /// Advance neuron state by one timestep `dt` (ms) with synaptic input `i_syn` (nA).
    ///
    /// Implements the LIF ODE via Euler integration:
    ///   τ_m × dV/dt = -(V - V_rest) + R_m × I_syn
    ///   → dV = [-(V - V_rest) + R_m × I_syn] / τ_m × dt
    ///
    /// Steady state (dV/dt=0): V_ss = V_rest + R_m × I_syn
    /// Threshold condition: V_ss ≥ V_thresh ↔ I_syn ≥ (V_thresh − V_rest) / R_m = 0.15 nA
    ///
    /// Returns `true` if an action potential was generated this timestep.
    pub fn integrate(&mut self, i_syn: f32, dt: f32, tick: u64) -> bool {
        // Absolute refractory: membrane clamped at V_reset, no integration.
        if tick <= self.refractory_until {
            self.v = self.v_reset;
            return false;
        }

        // Euler step: R_m×I_syn is inside the τ_m divisor (standard LIF form)
        let dv = (-(self.v - self.v_rest) + self.r_m * i_syn) / self.tau_m * dt;
        self.v += dv;

        // Threshold crossing → action potential
        if self.v >= self.v_thresh {
            self.v = self.v_reset;
            self.refractory_until = tick + TAU_REF_TICKS;
            self.last_spike = Some(tick);
            self.spike_count += 1;
            return true; // fired
        }
        false
    }

    /// Passive decay toward rest (no synaptic input this timestep).
    ///
    /// This is the "leaky" part of LIF — without input, the membrane
    /// slowly relaxes back to V_rest via the leak conductance.
    pub fn decay(&mut self, dt: f32) {
        self.v += (-(self.v - self.v_rest) / self.tau_m) * dt;
    }

    /// Inject a fixed current pulse — models direct EC/sensory input.
    /// Returns true if the pulse caused a spike.
    pub fn inject(&mut self, i_amp: f32, dt: f32, tick: u64) -> bool {
        self.integrate(i_amp, dt, tick)
    }

    /// Is the neuron in its absolute refractory period?
    pub fn is_refractory(&self, tick: u64) -> bool {
        tick <= self.refractory_until
    }

    /// Ticks elapsed since last spike (`None` if neuron has never fired).
    pub fn ticks_since_spike(&self, now: u64) -> Option<u64> {
        self.last_spike.map(|t| now.saturating_sub(t))
    }

    /// Mean firing rate over the last `window_ticks` — approximation of rate coding.
    pub fn firing_rate(&self, now: u64, window_ticks: u64) -> f32 {
        // Very approximate: uses spike_count / window as a proxy.
        // A proper estimate would need a spike train, but this is sufficient
        // for rate-coded recall weighting.
        let elapsed = now.min(window_ticks) as f32;
        if elapsed <= 0.0 {
            return 0.0;
        }
        self.spike_count as f32 / elapsed * 1000.0 // spikes per second (Hz)
    }
}

/// Minimum synaptic current needed to fire a default LIF neuron with a single 1ms pulse.
///
/// I_threshold = (V_thresh - V_rest) / R_m = 15 mV / 100 MΩ = 0.15 nA
pub const I_THRESHOLD_NA: f32 = (V_THRESH - V_REST) / R_M_MOHM; // 0.15 nA

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neuron_fires_above_threshold_current() {
        let mut n = LIFNeuron::default();
        // I_threshold = (V_thresh - V_rest) / R_m = 15/100 = 0.15 nA
        // With dt=1ms and sufficient current, should spike
        let fired = n.integrate(0.20, 1.0, 1); // 0.20 nA > 0.15 nA threshold
        // May not fire in one step (leaky integration), try a sustained pulse
        let mut spiked = fired;
        for t in 2..100u64 {
            if n.integrate(0.20, 1.0, t) {
                spiked = true;
                break;
            }
        }
        assert!(spiked, "neuron should fire with sustained suprathreshold current");
    }

    #[test]
    fn neuron_does_not_fire_below_threshold() {
        let mut n = LIFNeuron::default();
        // 0.05 nA is subthreshold — leak pulls it back to rest
        let mut spiked = false;
        for t in 1..200u64 {
            if n.integrate(0.05, 1.0, t) {
                spiked = true;
                break;
            }
        }
        assert!(!spiked, "subthreshold current should not cause a spike");
    }

    #[test]
    fn refractory_period_prevents_immediate_refiring() {
        let mut n = LIFNeuron::default();
        // Drive to spike
        let mut first_spike = 0u64;
        for t in 1..100u64 {
            if n.integrate(1.0, 1.0, t) {
                first_spike = t;
                break;
            }
        }
        assert!(first_spike > 0, "should have spiked");
        // Immediately after: should be refractory
        assert!(n.is_refractory(first_spike + 1), "should be refractory immediately after spike");
        assert!(!n.is_refractory(first_spike + TAU_REF_TICKS + 1), "should exit refractory after τ_ref");
    }

    #[test]
    fn membrane_decays_to_rest_without_input() {
        let mut n = LIFNeuron::default();
        n.v = -60.0; // depolarised above rest
        for _ in 0..200 {
            n.decay(1.0);
        }
        // Should be very close to V_rest after 200ms (10× time constant)
        assert!(
            (n.v - V_REST).abs() < 0.1,
            "membrane should decay to rest: got {:.2} mV",
            n.v
        );
    }
}
