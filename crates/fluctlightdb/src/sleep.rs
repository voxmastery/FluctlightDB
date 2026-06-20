use uuid::Uuid;

use crate::activation::active_set_from_engram;
use crate::amygdala::Amygdala;
use crate::budget::{self, WiringBudget};
use crate::cortex::Cortex;
use crate::dentate::{separate_episode, SeparationResult};
use crate::separation_gate;
use crate::development::{DevStage, DevelopmentState};
use crate::engram::Engram;
use crate::graph::BrainGraph;
use crate::hippocampus::Hippocampus;
use crate::life::LifeState;
use crate::neuromodulator::Neuromodulators;
use crate::semantic::SemanticField;
use crate::types::{Episode, SleepReport};
use crate::types::Region::{HippocampusCa3, HippocampusDg, HippocampusCa1};

/// Offline processing — sharp-wave replay, consolidation, pruning (Wilson/McNaughton).
pub fn sleep_cycle(
    hippocampus: &mut Hippocampus,
    cortex: &mut Cortex,
    amygdala: &mut Amygdala,
    graph: &mut BrainGraph,
    neuromodulators: &mut Neuromodulators,
    semantic: &mut SemanticField,
    life: &LifeState,
    development: &DevelopmentState,
    replay_limit: usize,
) -> SleepReport {
    use std::collections::HashSet;

    let stage_before = development.stage.as_str().to_string();
    let threshold = development.stage.prune_threshold();

    let recent: Vec<Uuid> = hippocampus
        .recent(life.life_id, replay_limit)
        .into_iter()
        .map(|e| e.id)
        .collect();

    let mut replays = 0u32;
    let mut consolidated = 0u32;
    let mut active_union = HashSet::new();

    for engram_id in &recent {
        if let Some(engram) = hippocampus.engrams.iter_mut().find(|e| e.id == *engram_id) {
            engram.replay_count += 1;
            replays += 1;
            let text = format!(
                "{} {} {}",
                engram.episode.content,
                engram.episode.context,
                engram.episode.outcome.as_deref().unwrap_or("")
            );
            cortex.consolidate_from_text(&text, 0.1 * engram.salience);
            cortex.consolidate_neurons(&engram.neurons, 0.05);
            if let Some(ref vec) = engram.episode.semantic_vector {
                cortex.consolidate_semantic(vec, 0.08 * engram.salience);
            }
            consolidated += 1;
            active_union.extend(active_set_from_engram(engram));
        }
    }

    let _ = semantic.consolidate_from_engrams(hippocampus, life.life_id, &recent);

    let gate = neuromodulators.plasticity_gate(0.3);
    graph.co_activate(&active_union, gate);
    let pruned = graph.prune_below(threshold);
    amygdala.decay();
    neuromodulators.on_sleep();

    SleepReport {
        replays,
        consolidated,
        pruned_synapses: pruned,
        stage_before,
        stage_after: development.stage.as_str().to_string(),
        advanced: false,
    }
}

/// Pattern separation (DG) + wiring (CA3/CA1) — Marr trisynaptic path.
pub fn separate_and_encode(
    graph: &mut BrainGraph,
    hippocampus: &Hippocampus,
    episode: &Episode,
    life_id: uuid::Uuid,
    tick: u64,
    stage: u8,
    salience: f32,
) -> (Engram, SeparationResult) {
    let dev_stage = DevStage::from_u8(stage);
    let budget = WiringBudget::for_stage(dev_stage);
    let engram_id = uuid::Uuid::new_v4();
    let window = separation_gate::overlap_window();
    let existing = hippocampus.tail_for_life(life_id, window);

    let sep = separate_episode(episode, life_id, engram_id, tick, &existing);
    let mut engram = Engram::from_separation(life_id, episode.clone(), salience, tick, stage, &sep);
    engram.id = engram_id;

    budget::wire_ca3_clique(graph, &engram.neurons, HippocampusCa3, &budget);

    for &n in &engram.dg_neurons {
        graph.register_neuron(n, HippocampusDg);
    }
    for &n in &engram.ec_neurons {
        graph.register_neuron(n, HippocampusCa1);
    }

    budget::wire_chain(
        graph,
        &engram.dg_neurons,
        HippocampusDg,
        0.2,
        budget.max_dg_chain_links,
    );
    budget::wire_chain(
        graph,
        &engram.neurons,
        HippocampusCa3,
        0.3,
        budget.max_ca3_chain_links,
    );

    (engram, sep)
}
