//! Stage-aware consolidation schedule — CLS maturation metrics exposed to agents.

use serde::{Deserialize, Serialize};

use crate::brain::FluctlightBrain;
use crate::development::{DevStage, StageThreshold};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StageConsolidationReport {
    pub stage: String,
    pub next_stage: Option<String>,
    pub myelination: f32,
    pub prune_threshold: f32,
    pub max_synapses: usize,
    pub synapse_pressure: f32,
    pub experiences: u64,
    pub sleep_cycles: u64,
    pub progress_to_next: f32,
    pub pfc_unlocked: bool,
    pub auto_sleep_due: bool,
}

pub fn report(brain: &FluctlightBrain) -> StageConsolidationReport {
    let stage = brain.development.stage;
    let m = &brain.development.metrics;
    let pressure = brain.autonomic.synapse_pressure(
        brain.graph.synapse_count(),
        stage.max_synapses(),
    );
    StageConsolidationReport {
        stage: stage.as_str().into(),
        next_stage: stage.next().map(|s| s.as_str().into()),
        myelination: stage.myelination(),
        prune_threshold: stage.prune_threshold(),
        max_synapses: stage.max_synapses(),
        synapse_pressure: pressure,
        experiences: m.experience_count,
        sleep_cycles: m.sleep_cycles,
        progress_to_next: progress_to_next(stage, m),
        pfc_unlocked: brain.prefrontal.unlocked,
        auto_sleep_due: brain.autonomic.should_sleep(
            brain.graph.synapse_count(),
            stage.max_synapses(),
        ),
    }
}

fn progress_to_next(stage: DevStage, m: &crate::development::DevelopmentMetrics) -> f32 {
    let Some(next) = stage.next() else {
        return 1.0;
    };
    let Some(th) = StageThreshold::to_enter(next) else {
        return 1.0;
    };
    let exp = (m.experience_count as f32 / th.min_experiences.max(1) as f32).min(1.0);
    let sleep = (m.sleep_cycles as f32 / th.min_sleep_cycles.max(1) as f32).min(1.0);
    let ticks = (m.ticks as f32 / th.min_ticks.max(1) as f32).min(1.0);
    (exp + sleep + ticks) / 3.0
}
