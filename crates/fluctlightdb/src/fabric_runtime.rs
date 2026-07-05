//! Recall Fabric runtime — wires Photon, Lattice, Phase, relation, chronos, crystallize,
//! forgetting, and confidence into the live `experience` / `sleep` / `activate` cycle.
//!
//! All state here is runtime-only (`serde(skip)` on [`FluctlightBrain`]); snapshot format unchanged.
//! Gated by `FLUCTLIGHT_FABRIC=1` (off by default).

use std::collections::HashMap;
use std::sync::Mutex;

use uuid::Uuid;

use crate::brain::FluctlightBrain;
use crate::confidence::{activation_multiplier, recall_confidence, Evidence, SourceKind};
use crate::forgetting::{LoadController, MemoryTrace};
use crate::recall_fabric::{FabricConfig, RecallFabric};
use crate::types::{ProvenanceKind, RecallResult};

/// Master switch — off by default so frozen benchmark numbers stay reproducible.
pub fn fabric_enabled() -> bool {
    std::env::var("FLUCTLIGHT_FABRIC").ok().as_deref() == Some("1")
}

/// Fabric tuning from env (also used when rebuilding after load).
pub fn fabric_config() -> FabricConfig {
    let mut cfg = FabricConfig::default();
    if let Ok(v) = std::env::var("FLUCTLIGHT_FABRIC_PREFILTER_K") {
        if let Ok(k) = v.parse() {
            cfg.prefilter_k = k;
        }
    }
    cfg
}

/// Weight for merging composed fabric scores into hybrid activation (0..1 typical).
fn fabric_blend_weight() -> f32 {
    std::env::var("FLUCTLIGHT_FABRIC_BLEND")
        .ok()
        .and_then(|v| v.parse::<f32>().ok())
        .unwrap_or(0.35)
}

fn confidence_mult(recall: &RecallResult) -> f32 {
    let kind = match recall.episode.provenance.as_ref().map(|p| &p.kind) {
        Some(ProvenanceKind::LedgerVerified)
        | Some(ProvenanceKind::ToolGrounded)
        | Some(ProvenanceKind::FileObservation) => SourceKind::Verified,
        Some(ProvenanceKind::UserExplicit) => SourceKind::UserStated,
        Some(ProvenanceKind::ChatAssertion) => SourceKind::Inferred,
        None => SourceKind::Unknown,
    };
    let conf = recall_confidence(&[Evidence::new(kind, 1.0)]);
    activation_multiplier(conf)
}

impl FluctlightBrain {
    /// Rebuild the in-memory Recall Fabric index from all hippocampal engrams (call after load).
    pub fn warm_fabric_runtime(&mut self) {
        if !fabric_enabled() {
            return;
        }
        let tick = self.autonomic.total_ticks;
        self.fabric = RecallFabric::new(fabric_config());
        let mut traces = self.fabric_traces.lock().unwrap();
        traces.clear();
        for e in &self.hippocampus.engrams {
            let id = e.id.to_string();
            let vec = self.semantic.engram_vectors.get(&e.id);
            self.fabric
                .insert(&id, &e.episode.content, vec.map(|v| v.as_slice()));
            traces.insert(id, MemoryTrace::new(tick, e.salience));
        }
    }

    /// Ingest path: chronos + crystallize + fabric index + forgetting trace.
    pub(crate) fn fabric_on_experience(
        &mut self,
        engram_id: Uuid,
        content: &str,
        salience: f32,
        tick: u64,
        vector: Option<&[f32]>,
    ) {
        if !fabric_enabled() {
            return;
        }
        let id = engram_id.to_string();
        self.chronos.add_event(id.clone(), tick);
        self.fabric.insert(&id, content, vector);
        self.fabric_traces
            .lock()
            .unwrap()
            .insert(id.clone(), MemoryTrace::new(tick, salience));
        if let Some(vec) = vector {
            let scalar = crate::recall_fabric::semantic_scalar(vec);
            let sig = crate::recall_fabric::structure_signature(content);
            self.crystallizer.crystallize(id, scalar, sig);
        }
    }

    /// Sleep consolidation: crystallize salient engrams + elastic lattice growth if crowded.
    pub(crate) fn fabric_on_sleep(&mut self) {
        if !fabric_enabled() {
            return;
        }
        for e in &self.hippocampus.engrams {
            if e.salience < 0.55 {
                continue;
            }
            let id = e.id.to_string();
            if let Some(vec) = self.semantic.engram_vectors.get(&e.id) {
                let scalar = crate::recall_fabric::semantic_scalar(vec);
                let sig = crate::recall_fabric::structure_signature(&e.episode.content);
                self.crystallizer.crystallize(id, scalar, sig);
            }
        }
        let live = self.hippocampus.engrams.len() as u64;
        let cap = self.fabric.lattice_capacity();
        let ctrl = LoadController::default();
        if let Some(scale) = ctrl.recommend_growth(live, cap) {
            self.fabric.grow_lattice(scale);
        }
    }

    /// Read path: full Photon → Lattice → Phase composed recall merged into hybrid activation,
    /// then forgetting retention + confidence trust weighting.
    pub(crate) fn fabric_on_activate(
        &self,
        cue: &str,
        cue_vector: Option<&[f32]>,
        recalls: &mut Vec<RecallResult>,
    ) {
        if !fabric_enabled() || recalls.is_empty() {
            return;
        }
        let blend_w = fabric_blend_weight();
        let top_k = recalls.len().max(32).min(128);
        let hits = self.fabric.recall(cue, cue_vector, top_k);
        let scores: HashMap<String, f32> = hits.iter().map(|h| (h.id.clone(), h.score)).collect();

        let tick = self.autonomic.total_ticks;
        let mut traces = self.fabric_traces.lock().unwrap();
        for recall in recalls.iter_mut() {
            let id = recall.engram_id.to_string();
            if let Some(&score) = scores.get(&id) {
                recall.activation += score * blend_w;
            }
            if let Some(trace) = traces.get_mut(&id) {
                let retention = trace.retention(tick).max(0.08);
                recall.activation *= retention;
                trace.rehearse(tick);
            }
            recall.activation *= confidence_mult(recall);
        }
    }

    pub fn fabric_len(&self) -> usize {
        self.fabric.len()
    }
}

/// Default fabric + trace store for new brains.
pub(crate) fn new_fabric_state() -> (RecallFabric, Mutex<HashMap<String, MemoryTrace>>) {
    (
        RecallFabric::new(fabric_config()),
        Mutex::new(HashMap::new()),
    )
}
