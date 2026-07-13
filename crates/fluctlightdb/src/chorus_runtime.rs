//! CHORUS runtime — phase-field imprint/recall wired into [`FluctlightBrain`].
//!
//! Gated by `FLUCTLIGHT_CHORUS=1`. Bulk imprint via wavelet injection; sleep collapses
//! high-resonance traces into hippocampal engrams.

use uuid::Uuid;

use crate::brain::FluctlightBrain;
use crate::chorus::{
    parent_memory_id, ChorusHit, ChorusImprintInput, ChorusRecallOpts, ChorusSleepReport,
    ChorusTrace,
};
use crate::fabric_runtime::fabric_enabled;
use crate::types::{Episode, Provenance, ProvenanceKind, RagRef, RecallResult};

pub fn chorus_enabled() -> bool {
    std::env::var("FLUCTLIGHT_CHORUS")
        .map(|v| matches!(v.to_lowercase().as_str(), "1" | "true" | "yes" | "on"))
        .unwrap_or(false)
}

pub fn chorus_fast_enabled() -> bool {
    std::env::var("FLUCTLIGHT_CHORUS_FAST")
        .map(|v| matches!(v.to_lowercase().as_str(), "1" | "true" | "yes" | "on"))
        .unwrap_or(false)
}

pub fn chorus_float_rerank_enabled() -> bool {
    std::env::var("FLUCTLIGHT_CHORUS_FLOAT_RERANK")
        .map(|v| !matches!(v.to_lowercase().as_str(), "0" | "false" | "no" | "off"))
        .unwrap_or(true)
}

pub fn new_chorus_field() -> crate::chorus::ChorusField {
    crate::chorus::ChorusField::default()
}

impl FluctlightBrain {
    pub fn chorus_len(&self) -> usize {
        self.chorus.len()
    }

    pub fn chorus_tick(&mut self) -> u8 {
        self.chorus.tick_theta()
    }

    pub fn chorus_imprint(&mut self, input: &ChorusImprintInput) -> bool {
        if !chorus_enabled() {
            return false;
        }
        let ok = self.chorus.imprint(input);
        if ok {
            self.fabric_on_chorus_imprint(input);
        }
        ok
    }

    pub fn chorus_imprint_batch(&mut self, batch: &[ChorusImprintInput]) -> usize {
        if !chorus_enabled() {
            return 0;
        }
        let mut n = 0usize;
        for input in batch {
            if self.chorus.imprint(input) {
                self.fabric_on_chorus_imprint(input);
                n += 1;
            }
        }
        n
    }

    pub fn chorus_recall(&self, cue: &str, k: usize, cue_vector: Option<&[f32]>) -> Vec<ChorusHit> {
        if !chorus_enabled() {
            return Vec::new();
        }
        let opts = ChorusRecallOpts {
            fast: chorus_fast_enabled(),
            float_rerank: chorus_float_rerank_enabled(),
        };
        let mut hits = self.chorus.recall_with_opts(cue, k, cue_vector, opts);
        if fabric_enabled() && !hits.is_empty() {
            self.fabric_rerank_chorus_hits(cue, cue_vector, &mut hits);
        }
        hits
    }

    pub fn chorus_recall_with_opts(
        &self,
        cue: &str,
        k: usize,
        cue_vector: Option<&[f32]>,
        opts: ChorusRecallOpts,
    ) -> Vec<ChorusHit> {
        if !chorus_enabled() {
            return Vec::new();
        }
        let mut hits = self.chorus.recall_with_opts(cue, k, cue_vector, opts);
        if fabric_enabled() && !hits.is_empty() {
            self.fabric_rerank_chorus_hits(cue, cue_vector, &mut hits);
        }
        hits
    }

    /// Late-interaction recall: token-population MaxSim ⊕ BM25 (RRF).
    pub fn chorus_recall_maxsim(
        &self,
        cue: &str,
        k: usize,
        query_tokens: &[Vec<f32>],
        cue_vector: Option<&[f32]>,
        w_bm: f32,
    ) -> Vec<ChorusHit> {
        if !chorus_enabled() {
            return Vec::new();
        }
        self.chorus
            .recall_maxsim(cue, k, query_tokens, cue_vector, w_bm)
    }

    pub fn chorus_recall_batch(
        &self,
        queries: &[(&str, Option<&[f32]>)],
        k: usize,
        opts: ChorusRecallOpts,
    ) -> Vec<Vec<ChorusHit>> {
        if !chorus_enabled() {
            return vec![Vec::new(); queries.len()];
        }
        let mut batches = self.chorus.recall_batch(queries, k, opts);
        for (hits, (cue, cue_vector)) in batches.iter_mut().zip(queries.iter()) {
            self.fabric_rerank_chorus_hits(cue, *cue_vector, hits);
        }
        batches
    }

    /// Tag last recall hits (SWR-TAG) for sleep triage.
    pub fn chorus_tag_hits(&mut self, hits: &[ChorusHit]) {
        if chorus_enabled() {
            self.chorus.tag_recall_hits(hits);
        }
    }

    /// θ-sweep + collapse promoted traces into full hippocampal engrams.
    pub fn chorus_sleep(&mut self) -> Result<ChorusSleepReport, crate::error::Error> {
        if !chorus_enabled() {
            return Ok(ChorusSleepReport::default());
        }
        let (mut report, queue) = self.chorus.sleep_sweep();
        for trace in queue {
            self.collapse_chorus_trace(&trace)?;
        }
        report.pruned = self.chorus.decay_untagged(1);
        Ok(report)
    }

    fn collapse_chorus_trace(&mut self, trace: &ChorusTrace) -> Result<(), crate::error::Error> {
        let mut episode =
            Episode::new(trace.content.clone(), trace.context.clone(), trace.salience);
        episode.semantic_vector = trace.vector.clone();
        episode.rag = Some(RagRef {
            doc_id: Some(trace.memory_id.clone()),
            ..Default::default()
        });
        if trace.sheath.verified || trace.sheath.provenance_kind > 0 {
            episode.provenance = Some(Provenance {
                kind: match trace.sheath.provenance_kind {
                    1 => ProvenanceKind::FileObservation,
                    2 => ProvenanceKind::ToolGrounded,
                    3 => ProvenanceKind::LedgerVerified,
                    4 => ProvenanceKind::UserExplicit,
                    _ => ProvenanceKind::ChatAssertion,
                },
                source_uri: trace.sheath.source_uri.clone(),
                confidence: if trace.sheath.verified { 0.95 } else { 0.5 },
                verified: trace.sheath.verified,
            });
        }
        if let Some(ref aid) = trace.sheath.agent_id {
            episode.agent_id = Some(aid.clone());
        }
        let _ = self.experience_internal(episode, false)?;
        Ok(())
    }

    pub(crate) fn merge_chorus_recalls(
        &self,
        cue: &str,
        cue_vector: Option<&[f32]>,
        recalls: &mut Vec<RecallResult>,
        top_k: usize,
    ) {
        if !chorus_enabled() {
            return;
        }
        let hits = self.chorus.recall(cue, top_k, cue_vector);
        for hit in hits {
            let parent = parent_memory_id(&hit.memory_id);
            let trace = self
                .chorus
                .get_trace(parent)
                .or_else(|| self.chorus.find_trace_by_parent(parent));
            let Some(trace) = trace else {
                continue;
            };
            let mut episode =
                Episode::new(trace.content.clone(), trace.context.clone(), trace.salience);
            episode.semantic_vector = trace.vector.clone();
            episode.rag = Some(RagRef {
                doc_id: Some(trace.memory_id.clone()),
                ..Default::default()
            });
            recalls.push(RecallResult {
                engram_id: Uuid::nil(),
                activation: hit.score,
                episode,
                completion_strength: hit.lexical,
                separation_index: 1.0,
                verified: trace.sheath.verified,
                trust_note: Some(format!("chorus:{} θ{}", hit.lane, hit.theta)),
            });
        }
        recalls.sort_by(|a, b| {
            b.activation
                .partial_cmp(&a.activation)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        recalls.truncate(top_k);
    }
}
