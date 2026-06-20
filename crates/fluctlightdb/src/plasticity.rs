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
