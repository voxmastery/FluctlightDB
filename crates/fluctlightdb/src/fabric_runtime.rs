//! Recall Fabric runtime — wires Photon, Lattice, Phase, relation, chronos, crystallize,
//! forgetting, and confidence into the live `experience` / `sleep` / `activate` cycle.
//!
//! All state here is runtime-only (`serde(skip)` on [`FluctlightBrain`]); snapshot format unchanged.
//! Gated by `FLUCTLIGHT_FABRIC=1` (off by default).

use std::collections::HashMap;
use std::sync::Mutex;

use uuid::Uuid;

use crate::brain::FluctlightBrain;
use crate::chorus::{ChorusHit, ChorusImprintInput};
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
    if let Ok(v) = std::env::var("FLUCTLIGHT_PHOTON_BITS") {
        if let Ok(b) = v.parse() {
            cfg.photon_bits = b;
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
    /// Rebuild the in-memory Recall Fabric index and Chronos timeline from hippocampal engrams
    /// (call after load). Existing snapshots predate Fabric — this backfills runtime state.
    pub fn warm_fabric_runtime(&mut self) {
        if !fabric_enabled() {
            return;
        }
        self.fabric = RecallFabric::new(fabric_config());
        let mut traces = self.fabric_traces.lock().unwrap();
        traces.clear();
        self.chronos = crate::chronos::Chronos::default();
        for e in &self.hippocampus.engrams {
            let id = e.id.to_string();
            let tick = e.encoded_at_tick;
            let vec = self.semantic.engram_vectors.get(&e.id);
            self.fabric
                .insert_rich(&id, &e.episode.content, vec.map(|v| v.as_slice()));
            traces.insert(id.clone(), MemoryTrace::new(tick, e.salience));
            self.chronos.add_event(id, tick);
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
        // IR / connect_index(): hybrid sidecar handles ranking; skip Fabric ingest here.
        if crate::activation::fast_ingest_mode() {
            return;
        }
        let _ = vector; // indexed by recall sidecar on IR path
        let id = engram_id.to_string();
        self.chronos.add_event(id.clone(), tick);
        self.fabric.insert_rich(&id, content, vector);
        self.fabric_traces
            .lock()
            .unwrap()
            .insert(id.clone(), MemoryTrace::new(tick, salience));
        // Crystallize on sleep for salient memories; skip per-write lattice work on hot ingest path.
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

    /// Photon LSH candidate ids for the hybrid activation prefilter (fast path).
    pub(crate) fn fabric_photon_candidates(
        &self,
        cue_vector: Option<&[f32]>,
    ) -> Option<std::collections::HashSet<Uuid>> {
        if !fabric_enabled() {
            return None;
        }
        let ids = self
            .fabric
            .photon_shortlist_ids(cue_vector, fabric_config().prefilter_k);
        if ids.is_empty() {
            return None;
        }
        let set: std::collections::HashSet<Uuid> = ids
            .into_iter()
            .filter_map(|s| Uuid::parse_str(&s).ok())
            .collect();
        if set.is_empty() {
            None
        } else {
            Some(set)
        }
    }

    /// Read path: Photon prefilter already narrowed candidates; score only the hybrid
    /// shortlist (O(k)), then blend Fabric scores. No full-store fabric.recall scan.
    ///
    /// Forgetting retention is intentionally *not* applied here: bulk ingest advances ticks
    /// per engram (e.g. LongMemEval haystacks with 100+ sessions), which would wrongly
    /// penalize early sessions on the first query. Decay belongs in sleep / idle consolidate.
    pub(crate) fn fabric_on_activate(
        &self,
        cue: &str,
        cue_vector: Option<&[f32]>,
        recalls: &mut Vec<RecallResult>,
    ) {
        if !fabric_enabled() || recalls.is_empty() {
            return;
        }
        // Hybrid recall-index path (LongMemEval connect_index): BM25+dense already ranks.
        // Fabric rerank for paper benchmarks runs on CHORUS hits (fabric_rerank_chorus_hits).
        if self.has_recall_index() {
            return;
        }
        let blend_w = fabric_blend_weight();
        let tick = self.autonomic.total_ticks;
        let ids: Vec<String> = recalls.iter().map(|r| r.engram_id.to_string()).collect();
        let id_refs: Vec<&str> = ids.iter().map(|s| s.as_str()).collect();
        let fabric_scores = self.fabric.score_shortlist_lite(&id_refs, cue, cue_vector);
        let mut traces = self.fabric_traces.lock().unwrap();
        for recall in recalls.iter_mut() {
            let id = recall.engram_id.to_string();
            if let Some(fs) = fabric_scores.get(&id) {
                recall.activation += fs * blend_w;
            }
            if let Some(trace) = traces.get_mut(&id) {
                trace.rehearse(tick);
            }
            recall.activation *= confidence_mult(recall);
        }
    }

    pub fn fabric_len(&self) -> usize {
        self.fabric.len()
    }

    /// CHORUS imprint path: index trace ids (string memory_id) for Fabric scoring at recall.
    pub(crate) fn fabric_on_chorus_imprint(&mut self, input: &ChorusImprintInput) {
        if !fabric_enabled() {
            return;
        }
        let salience = if input.salience > 0.0 {
            input.salience
        } else {
            0.55
        };
        let tick = self.autonomic.total_ticks;
        let id = input.memory_id.clone();
        self.chronos.add_event(id.clone(), tick);
        self.fabric
            .insert(&id, &input.content, input.semantic_vector.as_deref());
        self.fabric_traces
            .lock()
            .unwrap()
            .insert(id, MemoryTrace::new(tick, salience));
    }

    /// Re-rank CHORUS hits with Fabric scores + forgetting rehearsal (same blend as activate).
    pub(crate) fn fabric_rerank_chorus_hits(
        &self,
        cue: &str,
        cue_vector: Option<&[f32]>,
        hits: &mut Vec<ChorusHit>,
    ) {
        if !fabric_enabled() || hits.is_empty() {
            return;
        }
        let ids: Vec<&str> = hits.iter().map(|h| h.memory_id.as_str()).collect();
        let fabric_scores = self.fabric.score_shortlist_lite(&ids, cue, cue_vector);
        let blend_w = fabric_blend_weight();
        let tick = self.autonomic.total_ticks;
        let mut traces = self.fabric_traces.lock().unwrap();
        for hit in hits.iter_mut() {
            if let Some(fs) = fabric_scores.get(&hit.memory_id) {
                hit.score += fs * blend_w;
            }
            if let Some(trace) = traces.get_mut(&hit.memory_id) {
                let retention = trace.retention(tick).max(0.08);
                hit.score *= retention;
                trace.rehearse(tick);
            }
        }
        hits.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }
}

/// Default fabric + trace store for new brains.
pub(crate) fn new_fabric_state() -> (RecallFabric, Mutex<HashMap<String, MemoryTrace>>) {
    (
        RecallFabric::new(fabric_config()),
        Mutex::new(HashMap::new()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Episode;

    #[test]
    fn warm_fabric_backfills_chronos_from_engrams() {
        std::env::set_var("FLUCTLIGHT_FABRIC", "1");
        std::env::remove_var("FLUCTLIGHT_FAST_INGEST");
        std::env::remove_var("FLUCTLIGHT_VECTOR_FAST");
        let mut brain = FluctlightBrain::new();
        for content in ["alpha", "beta", "gamma"] {
            let ep = Episode {
                content: content.into(),
                context: "test".into(),
                outcome: None,
                salience_hint: 0.5,
                semantic_vector: None,
                agent_id: None,
                tenant_id: None,
                rag: None,
                provenance: None,
            };
            brain.experience(ep).unwrap();
        }
        assert_eq!(brain.chronos_len(), 3);
        brain.chronos = crate::chronos::Chronos::default();
        assert_eq!(brain.chronos_len(), 0);
        brain.warm_fabric_runtime();
        assert_eq!(brain.chronos_len(), 3);
        assert_eq!(brain.fabric_len(), 3);
        let recent = brain.timeline(2);
        assert_eq!(recent.len(), 2);
        std::env::remove_var("FLUCTLIGHT_FABRIC");
    }

    #[test]
    fn fabric_skips_ingest_on_fast_index_path() {
        use crate::types::RagRef;
        std::env::set_var("FLUCTLIGHT_FABRIC", "1");
        std::env::set_var("FLUCTLIGHT_FAST_INGEST", "1");
        std::env::set_var("FLUCTLIGHT_VECTOR_FAST", "1");
        let mut brain = FluctlightBrain::new();
        let ep = Episode {
            content: "session user enjoys hiking".into(),
            context: "bench".into(),
            outcome: None,
            salience_hint: 0.6,
            semantic_vector: Some(vec![0.1, 0.5, 0.3]),
            agent_id: None,
            tenant_id: None,
            rag: Some(RagRef {
                doc_id: Some("s0".into()),
                chunk_id: Some("session".into()),
                ..Default::default()
            }),
            provenance: None,
        };
        brain.experience(ep).unwrap();
        assert_eq!(brain.fabric_len(), 0);
        assert!(brain.has_recall_index());
        std::env::remove_var("FLUCTLIGHT_FABRIC");
        std::env::remove_var("FLUCTLIGHT_FAST_INGEST");
        std::env::remove_var("FLUCTLIGHT_VECTOR_FAST");
    }
}
