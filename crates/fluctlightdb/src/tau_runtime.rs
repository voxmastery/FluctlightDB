//! Tau Lane runtime — episodic fission wired into [`FluctlightBrain`].

use crate::brain::FluctlightBrain;
use crate::muon::{MuonHit, MuonImprintInput};
use crate::tau::{TauHit, TauLane};

pub fn tau_enabled() -> bool {
    std::env::var("FLUCTLIGHT_TAU").ok().as_deref() == Some("1")
}

impl FluctlightBrain {
    pub fn tau_shard_len(&self) -> usize {
        self.tau.shard_len()
    }

    pub fn tau_imprint_batch(&mut self, sessions: &[MuonImprintInput]) -> (usize, usize) {
        if !tau_enabled() {
            return (0, 0);
        }
        self.tau.imprint_batch(sessions)
    }

    pub fn tau_recall(&self, cue: &str, k: usize) -> Vec<TauHit> {
        self.tau_recall_typed(cue, k, "")
    }

    pub fn tau_recall_typed(&self, cue: &str, k: usize, question_type: &str) -> Vec<TauHit> {
        if !tau_enabled() {
            return Vec::new();
        }
        self.tau.recall_typed(cue, k, question_type)
    }

    pub fn tau_recall_rrf(&self, cues: &[&str], k: usize) -> Vec<TauHit> {
        self.tau_recall_rrf_typed(cues, k, "")
    }

    pub fn tau_recall_rrf_typed(
        &self,
        cues: &[&str],
        k: usize,
        question_type: &str,
    ) -> Vec<TauHit> {
        if !tau_enabled() {
            return Vec::new();
        }
        self.tau.recall_rrf_typed(cues, k, question_type)
    }

    /// Promote one episodic shard into a full hippocampal engram (lazy crystallize).
    pub fn tau_crystallize_shard(&mut self, shard_id: &str) -> crate::error::Result<uuid::Uuid> {
        use crate::types::Episode;
        if !tau_enabled() {
            return Err(crate::error::Error::Store(
                "FLUCTLIGHT_TAU=1 required".into(),
            ));
        }
        let Some(shard) = self.tau.get_shard(shard_id) else {
            return Err(crate::error::Error::Store(format!(
                "tau shard not found: {shard_id}"
            )));
        };
        let salience = if shard.is_fact || shard.role == "user" {
            0.78
        } else {
            0.65
        };
        let ep = Episode {
            content: shard.content.clone(),
            context: format!("tau:{}:{}", shard.session_id, shard.chunk_id),
            outcome: None,
            salience_hint: salience,
            semantic_vector: None,
            agent_id: None,
            tenant_id: None,
            rag: None,
            provenance: None,
        };
        let rep = self.experience(ep)?;
        Ok(rep.engram_id)
    }
}

pub(crate) fn new_tau_lane() -> TauLane {
    TauLane::default()
}

/// Convert episodic hits to session-level MuonHit for legacy API consumers.
pub fn tau_to_muon_hits(hits: &[TauHit]) -> Vec<MuonHit> {
    hits.iter()
        .map(|h| MuonHit {
            session_id: h.session_id.clone(),
            score: h.score,
            photon: h.photon,
            lexical: h.lexical,
            phase: h.phase,
            date: h.date.clone(),
            snippet: h.content.chars().take(420).collect(),
        })
        .collect()
}
