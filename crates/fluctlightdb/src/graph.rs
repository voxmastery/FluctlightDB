use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::id::NeuronId;
use crate::plasticity::{hebbian_strengthen, ltd_weaken, Synapse};
use crate::types::Region;

/// The connectome — neurons linked by weighted synapses (not a graph DB API).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BrainGraph {
    pub synapses: Vec<Synapse>,
    pub neuron_regions: HashMap<NeuronId, Region>,
    #[serde(default)]
    synapse_index: HashMap<(u64, u64), usize>,
}

impl BrainGraph {
    pub fn register_neuron(&mut self, id: NeuronId, region: Region) {
        self.neuron_regions.entry(id).or_insert(region);
    }

    /// Add or strengthen — dedup (from,to) keeping max weight.
    pub fn add_synapse(&mut self, synapse: Synapse) {
        self.register_neuron(synapse.from, synapse.region);
        self.register_neuron(synapse.to, synapse.region);
        let key = (synapse.from.0, synapse.to.0);
        if let Some(&idx) = self.synapse_index.get(&key) {
            if self.synapses[idx].weight < synapse.weight {
                self.synapses[idx].weight = synapse.weight;
            }
            return;
        }
        let idx = self.synapses.len();
        self.synapses.push(synapse);
        self.synapse_index.insert(key, idx);
    }

    pub fn rebuild_index(&mut self) {
        self.synapse_index.clear();
        for (i, s) in self.synapses.iter().enumerate() {
            self.synapse_index.insert((s.from.0, s.to.0), i);
        }
    }

    pub fn synapse_count(&self) -> usize {
        self.synapses.len()
    }

    pub fn neighbors(&self, from: NeuronId) -> impl Iterator<Item = (&Synapse, NeuronId)> {
        self.synapses
            .iter()
            .filter(move |s| s.from == from)
            .map(|s| (s, s.to))
    }

    pub fn co_activate(&mut self, active: &HashSet<NeuronId>, gate: f32) {
        for synapse in &mut self.synapses {
            if active.contains(&synapse.from) && active.contains(&synapse.to) {
                hebbian_strengthen(synapse, gate, 0.05);
            }
        }
    }

    pub fn prune_below(&mut self, threshold: f32) -> u32 {
        let before = self.synapses.len();
        self.synapses.retain(|s| s.weight >= threshold);
        let pruned = (before - self.synapses.len()) as u32;
        if pruned > 0 {
            self.rebuild_index();
        }
        pruned
    }

    pub fn weaken_unused(&mut self, active: &HashSet<NeuronId>, delta: f32) {
        for synapse in &mut self.synapses {
            if !active.contains(&synapse.from) && !active.contains(&synapse.to) {
                ltd_weaken(synapse, delta);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dedup_keeps_max_weight() {
        let mut g = BrainGraph::default();
        let a = NeuronId::from_token("a");
        let b = NeuronId::from_token("b");
        g.add_synapse(Synapse::new(a, b, Region::HippocampusCa3, 0.3));
        g.add_synapse(Synapse::new(a, b, Region::HippocampusCa3, 0.8));
        assert_eq!(g.synapse_count(), 1);
        assert!((g.synapses[0].weight - 0.8).abs() < 1e-5);
    }
}
