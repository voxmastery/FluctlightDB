use serde::{Deserialize, Serialize};

use crate::id::NeuronId;
use crate::types::Region;

/// Directed synaptic connection with plasticity state (not a DB row).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Synapse {
    pub from: NeuronId,
    pub to: NeuronId,
    pub weight: f32,
    pub region: Region,
    pub plasticity_ready: f32,
    pub co_activations: u32,
}

impl Synapse {
    pub fn new(from: NeuronId, to: NeuronId, region: Region, initial: f32) -> Self {
        Self {
            from,
            to,
            weight: initial,
            region,
            plasticity_ready: 1.0,
            co_activations: 0,
        }
    }
}

/// Hebbian strengthen / LTD weaken (Kandel / Hebb).
pub fn hebbian_strengthen(synapse: &mut Synapse, gate: f32, delta: f32) {
    synapse.co_activations = synapse.co_activations.saturating_add(1);
    synapse.weight = (synapse.weight + delta * gate).min(1.0);
    synapse.plasticity_ready = (synapse.plasticity_ready - 0.02).max(0.1);
}

pub fn ltd_weaken(synapse: &mut Synapse, delta: f32) {
    synapse.weight = (synapse.weight - delta).max(0.001);
}

/// Spike-timing-dependent plasticity (Bi & Poo 1998).
///
/// If the pre-synaptic neuron fires BEFORE the post-synaptic neuron within a
/// ~20ms window, the synapse potentiates (LTP). If the post fires before the
/// pre, it depresses (LTD). Magnitude decays exponentially with |Δt|.
///
/// `pre_tick` / `post_tick` are discrete spike times interpreted as milliseconds.
/// Returns the applied weight change (ΔW).
pub fn stdp_update(synapse: &mut Synapse, pre_tick: u64, post_tick: u64) -> f32 {
    stdp_update_da(synapse, pre_tick, post_tick, 0.5) // DA=0.5 → baseline, no amplification
}

/// DA-gated three-factor STDP (Frémaux & Gerstner 2016; Bi & Poo 1998).
///
/// Extends the two-factor STDP rule with a dopamine gating term:
///   ΔW = eligibility_trace × da_multiplier
///
/// The eligibility trace is the classic STDP window (causal → LTP, anti-causal → LTD).
/// Dopamine gates how much of that trace converts to a durable weight change.
/// This is the mechanism behind reward-contingent learning: a synapse that fired
/// in the right causal order AND was followed by a DA burst consolidates much faster.
///
/// DA-gating applies to LTP only (Frémaux & Gerstner 2016 finding): LTD is
/// DA-independent — pruning of anti-causal synapses happens regardless of reward.
///
/// `da_gate` [0, 1]: 0.5 = baseline (ΔW × 1.0); 1.0 = reward burst (ΔW × 2.0).
pub fn stdp_update_da(synapse: &mut Synapse, pre_tick: u64, post_tick: u64, da_gate: f32) -> f32 {
    const A_PLUS: f32 = 0.005;
    const A_MINUS: f32 = 0.005;
    const TAU_PLUS: f32 = 20.0;  // ms
    const TAU_MINUS: f32 = 20.0; // ms
    const WINDOW_MS: f32 = 20.0;

    // Δt = post − pre (positive when pre leads post → causal → LTP).
    let dt = post_tick as i64 - pre_tick as i64;
    let abs_dt = dt.unsigned_abs() as f32;

    // Outside the plasticity window: no change.
    if abs_dt > WINDOW_MS {
        return 0.0;
    }

    // da_multiplier: DA=0.5 → 1.0× (neutral baseline); DA=1.0 → 2.0× (reward doubles LTP).
    let da_multiplier = da_gate * 2.0;

    let delta_w = if dt > 0 {
        // Pre before post → LTP, amplified by dopamine (three-factor rule).
        A_PLUS * (-abs_dt / TAU_PLUS).exp() * da_multiplier
    } else if dt < 0 {
        // Post before pre → LTD (DA-independent: pruning doesn't require reward signal).
        -A_MINUS * (-abs_dt / TAU_MINUS).exp()
    } else {
        // Simultaneous: no directional causality.
        0.0
    };

    synapse.weight = (synapse.weight + delta_w).clamp(0.001, 1.0);
    if delta_w > 0.0 {
        synapse.co_activations = synapse.co_activations.saturating_add(1);
        synapse.plasticity_ready = (synapse.plasticity_ready - 0.02).max(0.1);
    }
    delta_w
}
