use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::dentate::SeparationResult;
use crate::id::{EngramId, NeuronId};
use crate::tokenize::tokenize;
use crate::tokenize::tokenize_rich;
use crate::types::Episode;

/// Physical memory trace — sparse neuron ensemble + metadata (Tonegawa engram).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Engram {
    pub id: Uuid,
    pub life_id: Uuid,
    /// CA3 output / completion set.
    pub neurons: Vec<NeuronId>,
    /// Entorhinal input layer.
    #[serde(default)]
    pub ec_neurons: Vec<NeuronId>,
    /// Dentate gyrus sparse code after separation.
    #[serde(default)]
    pub dg_neurons: Vec<NeuronId>,
    #[serde(default)]
    pub separation_index: f32,
    pub episode: Episode,
    pub salience: f32,
    pub encoded_at_tick: u64,
    pub encoded_at_stage: u8,
    pub replay_count: u32,
    pub is_core: bool,
}

impl Engram {
    pub fn from_separation(
        life_id: Uuid,
        episode: Episode,
        salience: f32,
        tick: u64,
        stage: u8,
        sep: &SeparationResult,
    ) -> Self {
        Self {
            id: EngramId::new().0,
            life_id,
            neurons: sep.ca3_neurons.clone(),
            ec_neurons: sep.ec_neurons.clone(),
            dg_neurons: sep.dg_neurons.clone(),
            separation_index: sep.separation_index,
            episode,
            salience,
            encoded_at_tick: tick,
            encoded_at_stage: stage,
            replay_count: 0,
            is_core: false,
        }
    }

    #[deprecated(note = "use from_separation via dentate gyrus")]
    pub fn new(life_id: Uuid, episode: Episode, salience: f32, tick: u64, stage: u8) -> Self {
        let rich = tokenize_rich(
            &episode.content,
            &episode.context,
            episode.outcome.as_deref(),
        );
        let tokens: Vec<String> = rich.iter().map(|t| t.surface.clone()).collect();
        let neurons: Vec<NeuronId> = tokens
            .iter()
            .map(|t| NeuronId::from_seeds(&["legacy", t]))
            .collect();
        Self {
            id: EngramId::new().0,
            life_id,
            neurons,
            ec_neurons: vec![],
            dg_neurons: vec![],
            separation_index: 0.0,
            episode,
            salience,
            encoded_at_tick: tick,
            encoded_at_stage: stage,
            replay_count: 0,
            is_core: false,
        }
    }

    pub fn cue_overlap(&self, cue_neurons: &[NeuronId]) -> f32 {
        if cue_neurons.is_empty() {
            return 0.0;
        }
        let mut best = 0.0_f32;
        for set in [&self.dg_neurons, &self.neurons] {
            if set.is_empty() {
                continue;
            }
            let hs: HashSet<_> = set.iter().copied().collect();
            let overlap = cue_neurons.iter().filter(|n| hs.contains(n)).count();
            best = best.max(overlap as f32 / cue_neurons.len() as f32);
        }
        best
    }
}

/// Cue neurons from text using same rich token pipeline as encoding.
pub fn cue_neurons(content: &str, context: &str) -> Vec<NeuronId> {
    let rich = tokenize_rich(content, context, None);
    rich.iter()
        .flat_map(|t| {
            (0..2).map(move |g| NeuronId::from_seeds(&["dg", "cue", &t.surface, &g.to_string()]))
        })
        .collect()
}

/// Legacy cue path for activate().
pub fn cue_neurons_simple(text: &str) -> Vec<NeuronId> {
    tokenize(text)
        .iter()
        .map(|t| NeuronId::from_token(t))
        .collect()
}
