//! Synapse wiring budgets — cap fan-out per engram/stage without limiting plasticity rules.

use crate::development::DevStage;
use crate::graph::BrainGraph;
use crate::id::NeuronId;
use crate::plasticity::Synapse;
use crate::types::Region;

/// Per-stage wiring limits (connections added at encode time, not Hebbian co-activation).
#[derive(Debug, Clone, Copy)]
pub struct WiringBudget {
    pub max_ca3_clique_neighbors: usize,
    pub max_dg_chain_links: usize,
    pub max_ca3_chain_links: usize,
    pub max_semantic_ec_links: usize,
    pub max_dg_to_ec_links: usize,
}

impl WiringBudget {
    pub fn for_stage(stage: DevStage) -> Self {
        match stage {
            DevStage::Embryonic => Self {
                max_ca3_clique_neighbors: 2,
                max_dg_chain_links: 4,
                max_ca3_chain_links: 4,
                max_semantic_ec_links: 4,
                max_dg_to_ec_links: 8,
            },
            DevStage::Newborn => Self {
                max_ca3_clique_neighbors: 3,
                max_dg_chain_links: 6,
                max_ca3_chain_links: 6,
                max_semantic_ec_links: 6,
                max_dg_to_ec_links: 12,
            },
            DevStage::Infant | DevStage::Child => Self {
                max_ca3_clique_neighbors: 3,
                max_dg_chain_links: 8,
                max_ca3_chain_links: 8,
                max_semantic_ec_links: 8,
                max_dg_to_ec_links: 16,
            },
            DevStage::Adolescent | DevStage::Adult | DevStage::Expert => Self {
                max_ca3_clique_neighbors: 4,
                max_dg_chain_links: 10,
                max_ca3_chain_links: 10,
                max_semantic_ec_links: 12,
                max_dg_to_ec_links: 24,
            },
        }
    }
}

pub fn wire_ca3_clique(
    graph: &mut BrainGraph,
    neurons: &[NeuronId],
    region: Region,
    budget: &WiringBudget,
) {
    let n = neurons.len().min(budget.max_ca3_clique_neighbors + 1);
    for i in 0..n {
        for j in (i + 1)..n.min(i + 1 + budget.max_ca3_clique_neighbors) {
            graph.add_synapse(Synapse::new(neurons[i], neurons[j], region, 0.3));
        }
    }
}

pub fn wire_chain(
    graph: &mut BrainGraph,
    neurons: &[NeuronId],
    region: Region,
    initial_weight: f32,
    max_links: usize,
) {
    let limit = neurons.len().saturating_sub(1).min(max_links);
    for i in 0..limit {
        graph.add_synapse(Synapse::new(
            neurons[i],
            neurons[i + 1],
            region,
            initial_weight,
        ));
    }
}

pub fn wire_dg_to_ec(
    graph: &mut BrainGraph,
    dg: &[NeuronId],
    ec: &[NeuronId],
    max_links: usize,
) {
    let mut added = 0usize;
    'outer: for &d in dg {
        for &e in ec {
            graph.add_synapse(Synapse::new(d, e, Region::HippocampusCa1, 0.18));
            added += 1;
            if added >= max_links {
                break 'outer;
            }
        }
    }
}

pub const PRESSURE_COMPACT_THRESHOLD: f32 = 0.7;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn child_budget_limits_clique() {
        let b = WiringBudget::for_stage(DevStage::Child);
        assert_eq!(b.max_ca3_clique_neighbors, 3);
    }
}
