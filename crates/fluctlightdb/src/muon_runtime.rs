//! Muon Lane runtime — bulk penetrative imprint wired into [`FluctlightBrain`].
//!
//! Gated by `FLUCTLIGHT_MUON=1`. Replaces per-turn haystack `experience()` with session-level
//! imprint when the benchmark or API uses muon ingest.

use crate::brain::FluctlightBrain;
use crate::muon::{MuonHit, MuonImprintInput, MuonLane};
use crate::tau_runtime::{tau_enabled, tau_to_muon_hits};

pub fn muon_enabled() -> bool {
    std::env::var("FLUCTLIGHT_MUON").ok().as_deref() == Some("1") || tau_enabled()
}

impl FluctlightBrain {
    pub fn muon_len(&self) -> usize {
        if tau_enabled() {
            self.tau.session_len()
        } else {
            self.muon.len()
        }
    }

    /// Penetrative imprint of one session (no hippocampal encode, no embed HTTP).
    pub fn muon_imprint(&mut self, session_id: &str, date: &str, body: &str, user_keys: &str) {
        if !muon_enabled() {
            return;
        }
        if tau_enabled() {
            self.tau.imprint(session_id, date, body, user_keys);
        } else {
            self.muon.imprint(session_id, date, body, user_keys);
        }
    }

    pub fn muon_imprint_batch(&mut self, sessions: &[MuonImprintInput]) -> usize {
        if !muon_enabled() {
            return 0;
        }
        if tau_enabled() {
            let (n, _) = self.tau.imprint_batch(sessions);
            n
        } else {
            self.muon.imprint_batch(sessions)
        }
    }

    /// Penetrative recall — Tau episodic fission when `FLUCTLIGHT_TAU=1`, else session Muon hits.
    pub fn muon_recall(&self, cue: &str, k: usize) -> Vec<MuonHit> {
        if !muon_enabled() {
            return Vec::new();
        }
        if tau_enabled() {
            return tau_to_muon_hits(&self.tau.recall(cue, k));
        }
        self.muon.recall(cue, k)
    }

    pub fn muon_reel(&self, session_id: &str) -> Option<String> {
        if tau_enabled() {
            self.tau.muon.get_reel(session_id).map(|s| s.to_string())
        } else {
            self.muon.get_reel(session_id).map(|s| s.to_string())
        }
    }
}

pub(crate) fn new_muon_lane() -> MuonLane {
    MuonLane::default()
}
