//! Preplay — prospective spreading activation for planning (hippocampal replay analog).

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::dentate::cue_to_dg_neurons;
use crate::graph::BrainGraph;
use crate::hippocampus::Hippocampus;
use crate::id::NeuronId;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PreplayStep {
    pub hop: u32,
    pub neuron: u64,
    pub activation: f32,
    pub engram_preview: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PreplayResult {
    pub goal: String,
    pub steps: u32,
    pub path: Vec<PreplayStep>,
    pub terminal_engrams: Vec<String>,
}

pub fn preplay_forward(
    goal: &str,
    steps: u32,
    graph: &BrainGraph,
    hippocampus: &Hippocampus,
    life_id: Uuid,
    myelination: f32,
) -> PreplayResult {
    let cue_neurons = cue_to_dg_neurons(goal, life_id);
    let mut activation: HashMap<NeuronId, f32> = HashMap::new();
    for n in cue_neurons {
        activation.insert(n, 1.0);
    }
    let spread = 0.55 * myelination.max(0.1);
    let mut path = Vec::new();
    let mut visited: HashSet<NeuronId> = HashSet::new();

    for hop in 0..steps {
        let current: Vec<(NeuronId, f32)> = activation.iter().map(|(k, v)| (*k, *v)).collect();
        if current.is_empty() {
            break;
        }
        let (best_node, best_act) = *current
            .iter()
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
            .unwrap();
        if visited.contains(&best_node) {
            break;
        }
        visited.insert(best_node);
        let preview = hippocampus
            .engrams_for_life(life_id)
            .find(|e| e.neurons.contains(&best_node))
            .map(|e| e.episode.content.chars().take(80).collect());
        path.push(PreplayStep {
            hop,
            neuron: best_node.0,
            activation: best_act,
            engram_preview: preview,
        });
        for (synapse, to) in graph.neighbors(best_node) {
            let next = best_act * synapse.weight * spread;
            if next > 0.02 {
                *activation.entry(to).or_insert(0.0) =
                    activation.get(&to).copied().unwrap_or(0.0).max(next);
            }
        }
        activation.remove(&best_node);
    }

    let mut terminal: Vec<String> = path.iter().filter_map(|p| p.engram_preview.clone()).collect();
    terminal.sort();
    terminal.dedup();
    terminal.truncate(8);

    PreplayResult {
        goal: goal.to_string(),
        steps,
        path,
        terminal_engrams: terminal,
    }
}
