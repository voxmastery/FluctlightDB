//! Tau Lane — episodic fission from Muon penetrative imprints.
//!
//! # Why this exists
//! Muon Lane ([`crate::muon`]) stores bulk history at **session** granularity in one penetrative
//! pass — fast, but not full episodic memory. Real agent recall needs **turn-level** and
//! **fact-level** traces (what the user said, when, in which session) without reverting to
//! hundreds of hippocampal `experience()` calls.
//!
//! **Tau particles** decay into lighter leptons — Muon → Tau → Episodic shards. Tau Lane is that
//! decay: at imprint time, each session **fissions** into turn shards and atomic fact shards,
//! each with its own count-sketch bitcode, linked to a parent session imprint.
//!
//! ## Mechanism: Penetrative Imprint → Episodic Fission (PIEF)
//! 1. **Muon imprint** — one pass stores session reel + session-level LSH (unchanged speed).
//! 2. **Fission** — parse `user:`/`assistant:` turns + extract user atomic facts → [`TauShard`].
//! 3. **Shard index** — global Photon LSH over all shards (turn + fact granularity).
//! 4. **Two-stage recall** — Muon session shortlist (top 32) ∪ shard LSH → score shards with
//!    parent-session boost + user/fact weights → episodic hits with `chunk_id` (`turn-N`, `fact-N`).
//! 5. **Lazy crystallize** (optional) — promote top shard to full hippocampal engram on demand.
//!
//! Write cost stays **O(sessions)**; read returns **full episodic** candidates at ~ms latency.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::muon::{count_sketch, jaccard, token_bag, MuonImprintInput, MuonLane};
use crate::photon::{PhotonCode, PhotonStore, SimHasher};
use crate::recall_fabric::structural_boost;

/// One episodic shard — a turn or atomic fact fissioned from a Muon imprint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TauShard {
    pub shard_id: String,
    pub session_id: String,
    pub chunk_id: String,
    pub date: String,
    pub role: String,
    pub content: String,
    pub tokens: Vec<String>,
    pub code: PhotonCode,
    /// Fact shards (index–value keys) get retrieval boost.
    pub is_fact: bool,
}

/// Scored episodic recall hit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TauHit {
    pub shard_id: String,
    pub session_id: String,
    pub chunk_id: String,
    pub score: f32,
    pub photon: f32,
    pub lexical: f32,
    pub phase: f32,
    pub date: String,
    pub role: String,
    pub content: String,
}

/// Muon session store + episodic shard index.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TauLane {
    sketch_dim: usize,
    photon_bits: usize,
    hasher: SimHasher,
    pub muon: MuonLane,
    shards: Vec<TauShard>,
    by_session: HashMap<String, Vec<usize>>,
    shard_prefilter: PhotonStore,
}

impl Default for TauLane {
    fn default() -> Self {
        Self::new(128, 256)
    }
}

impl TauLane {
    pub fn new(sketch_dim: usize, photon_bits: usize) -> Self {
        Self {
            sketch_dim,
            photon_bits,
            hasher: SimHasher::new(photon_bits, 0x7A00_F155),
            muon: MuonLane::new(sketch_dim, photon_bits),
            shards: Vec::new(),
            by_session: HashMap::new(),
            shard_prefilter: PhotonStore::new(photon_bits, 32, 8),
        }
    }

    pub fn session_len(&self) -> usize {
        self.muon.len()
    }

    pub fn shard_len(&self) -> usize {
        self.shards.len()
    }

    pub fn clear(&mut self) {
        self.muon.clear();
        self.shards.clear();
        self.by_session.clear();
        self.shard_prefilter = PhotonStore::new(self.photon_bits, 32, 8);
    }

    /// Penetrative imprint + episodic fission in one pass per session.
    pub fn imprint(&mut self, session_id: &str, date: &str, body: &str, user_keys: &str) {
        self.muon.imprint(session_id, date, body, user_keys);
        let sketch_dim = self.sketch_dim;
        let hasher = self.hasher.clone();
        fission_session(
            self,
            session_id,
            date,
            body,
            user_keys,
            sketch_dim,
            &hasher,
        );
    }

    pub fn imprint_batch(&mut self, sessions: &[MuonImprintInput]) -> (usize, usize) {
        self.clear();
        for s in sessions {
            self.imprint(&s.session_id, &s.date, &s.body, &s.user_keys);
        }
        (sessions.len(), self.shards.len())
    }

    /// Full episodic recall: Muon session gate + shard rerank.
    pub fn recall(&self, cue: &str, k: usize) -> Vec<TauHit> {
        if self.shards.is_empty() {
            return self
                .muon
                .recall(cue, k)
                .into_iter()
                .map(|h| TauHit {
                    shard_id: format!("muon:{}", h.session_id),
                    session_id: h.session_id,
                    chunk_id: "session".into(),
                    score: h.score,
                    photon: h.photon,
                    lexical: h.lexical,
                    phase: h.phase,
                    date: h.date,
                    role: "session".into(),
                    content: h.snippet,
                })
                .collect();
        }

        let cue_tokens = token_bag(cue, cue);
        let sketch = count_sketch(cue, cue, self.sketch_dim);
        let qc = self.hasher.encode(&sketch);

        // Parent session scores from Muon (top 32 gates fission search).
        let session_hits: HashMap<String, f32> = self
            .muon
            .recall(cue, 32)
            .into_iter()
            .map(|h| (h.session_id, h.score))
            .collect();

        let mut candidate_idxs: HashSet<usize> = HashSet::new();
        for sid in session_hits.keys() {
            if let Some(idxs) = self.by_session.get(sid) {
                candidate_idxs.extend(idxs.iter().copied());
            }
        }
        for (sid, _) in self.shard_prefilter.query(&qc, 128) {
            if let Some(pos) = self
                .shards
                .iter()
                .position(|s| s.shard_id == sid)
            {
                candidate_idxs.insert(pos);
            }
        }
        if candidate_idxs.is_empty() {
            candidate_idxs.extend(0..self.shards.len());
        }

        let mut scored: Vec<TauHit> = candidate_idxs
            .into_iter()
            .filter_map(|i| {
                let shard = self.shards.get(i)?;
                let hamming = qc.hamming(&shard.code);
                let photon = 1.0 - (hamming as f32 / self.photon_bits as f32);
                let lexical = jaccard(&cue_tokens, &shard.tokens);
                let phase = structural_boost(cue, &shard.content, 256);
                let parent = session_hits.get(&shard.session_id).copied().unwrap_or(0.0);
                let mut score =
                    0.20 * photon + 0.30 * lexical + 0.15 * phase + 0.20 * parent;
                if shard.role == "user" {
                    score += 0.08;
                }
                if shard.is_fact {
                    score += 0.12;
                }
                score += rare_token_boost(cue, &shard.content);
                Some(TauHit {
                    shard_id: shard.shard_id.clone(),
                    session_id: shard.session_id.clone(),
                    chunk_id: shard.chunk_id.clone(),
                    score,
                    photon,
                    lexical,
                    phase,
                    date: shard.date.clone(),
                    role: shard.role.clone(),
                    content: shard.content.clone(),
                })
            })
            .collect();

        scored.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Session metric: keep best shard per session in top-k pool.
        let mut by_session: HashMap<String, TauHit> = HashMap::new();
        for hit in scored {
            by_session
                .entry(hit.session_id.clone())
                .and_modify(|prev| {
                    if hit.score > prev.score {
                        *prev = hit.clone();
                    }
                })
                .or_insert(hit);
        }
        let mut merged: Vec<TauHit> = by_session.into_values().collect();
        merged.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        merged.truncate(k);
        merged
    }

    pub fn get_shard(&self, shard_id: &str) -> Option<&TauShard> {
        self.shards.iter().find(|s| s.shard_id == shard_id)
    }
}

fn fission_session(
    lane: &mut TauLane,
    session_id: &str,
    date: &str,
    body: &str,
    user_keys: &str,
    sketch_dim: usize,
    hasher: &SimHasher,
) {
    let prefix = if date.is_empty() {
        String::new()
    } else {
        format!("[{date}] ")
    };

    let mut turn_n = 0u32;
    let mut fact_n = 0u32;

    for line in body.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let (role, content) = if let Some(rest) = line.strip_prefix("user:") {
            ("user", rest.trim())
        } else if let Some(rest) = line.strip_prefix("assistant:") {
            ("assistant", rest.trim())
        } else {
            continue;
        };
        if content.is_empty() {
            continue;
        }
        let full = format!("{prefix}{role}: {content}");
        if role == "user" {
            push_shard(
                lane,
                session_id,
                date,
                role,
                &format!("turn-{turn_n}"),
                &full,
                false,
                sketch_dim,
                hasher,
            );
            turn_n += 1;
            if turn_n <= 12 {
                for fact in extract_atomic_facts(content, date) {
                    push_shard(
                        lane,
                        session_id,
                        date,
                        "user",
                        &format!("fact-{fact_n}"),
                        &fact,
                        true,
                        sketch_dim,
                        hasher,
                    );
                    fact_n += 1;
                }
            }
        } else {
            push_shard(
                lane,
                session_id,
                date,
                role,
                &format!("turn-{turn_n}"),
                &full,
                false,
                sketch_dim,
                hasher,
            );
            turn_n += 1;
        }
    }

    // Session-level + user-key shards (dual-key / pref-facts analog).
    if !user_keys.is_empty() {
        push_shard(
            lane,
            session_id,
            date,
            "user",
            "user_keys",
            user_keys,
            true,
            sketch_dim,
            hasher,
        );
    }
    let session_body = if body.len() > 12000 {
        &body[..12000]
    } else {
        body
    };
    push_shard(
        lane,
        session_id,
        date,
        "session",
        "session",
        session_body,
        false,
        sketch_dim,
        hasher,
    );
}

fn push_shard(
    lane: &mut TauLane,
    session_id: &str,
    date: &str,
    role: &str,
    chunk_id: &str,
    content: &str,
    is_fact: bool,
    sketch_dim: usize,
    hasher: &SimHasher,
) {
    let sketch = count_sketch(content, if is_fact { content } else { "" }, sketch_dim);
    let code = hasher.encode(&sketch);
    let shard_id = format!("{session_id}#{chunk_id}");
    let idx = lane.shards.len();
    lane.shard_prefilter
        .insert(shard_id.clone(), code.clone());
    lane.shards.push(TauShard {
        shard_id,
        session_id: session_id.to_string(),
        chunk_id: chunk_id.to_string(),
        date: date.to_string(),
        role: role.to_string(),
        content: content.to_string(),
        tokens: token_bag(content, ""),
        code,
        is_fact,
    });
    lane.by_session
        .entry(session_id.to_string())
        .or_default()
        .push(idx);
}

fn extract_atomic_facts(content: &str, date: &str) -> Vec<String> {
    let prefix = if date.is_empty() {
        String::new()
    } else {
        format!("[{date}] ")
    };
    let cues = [
        "bought", "purchased", "graduated", "commute", "volunteer", "coupon", "mbps", "yoga",
        "degree", "redeemed", "upgraded", "playlist", "redeemed", "wallet", "tennis", "repainted",
        "name", "favorite", "created", "attended", "occupation", "spent", "recommend",
    ];
    let first_person = [" i ", " i'm ", " i've ", " my ", " we ", " our "];
    let mut out = Vec::new();
    for sent in content.split(|c: char| c == '.' || c == '!' || c == '?') {
        let s = sent.trim();
        if s.len() < 12 {
            continue;
        }
        let sl = format!(" {} ", s.to_lowercase());
        if first_person.iter().any(|p| sl.contains(p))
            || cues.iter().any(|c| sl.contains(&format!(" {c} ")))
        {
            out.push(format!("{prefix}{}.", &s[..s.len().min(380)]));
        }
        if out.len() >= 6 {
            break;
        }
    }
    out
}

fn rare_token_boost(cue: &str, content: &str) -> f32 {
    let stop: HashSet<&str> = [
        "what", "when", "where", "which", "that", "this", "with", "from", "have", "your", "about",
        "the", "and", "for", "did", "was", "were", "are", "you", "user",
    ]
    .into_iter()
    .collect();
    let ct = content.to_lowercase();
    let mut boost = 0.0f32;
    for w in cue.split(|c: char| !c.is_alphanumeric()) {
        let w = w.to_lowercase();
        if w.len() < 4 || stop.contains(w.as_str()) {
            continue;
        }
        if ct.contains(&w) {
            boost += 0.04;
        }
    }
    boost.min(0.24)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fission_produces_turn_and_fact_shards() {
        let mut lane = TauLane::default();
        lane.imprint(
            "gold",
            "2023-05-29",
            "user: I graduated with a degree in Business Administration.\n\
             assistant: Congrats!",
            "user: I graduated with a degree in Business Administration.",
        );
        assert!(lane.shard_len() >= 3);
        assert!(lane.session_len() == 1);
    }

    #[test]
    fn episodic_recall_beats_noise_sessions() {
        let mut lane = TauLane::default();
        lane.imprint(
            "answer_gold",
            "2023-05-29",
            "user: I graduated with a degree in Business Administration.\n\
             assistant: Great!",
            "user: I graduated with a degree in Business Administration.",
        );
        for i in 0..40 {
            lane.imprint(
                &format!("noise_{i}"),
                "2023-01-01",
                &format!("user: random topic {i} nothing relevant here"),
                "",
            );
        }
        let hits = lane.recall(
            "What degree did the user graduate with? Business Administration",
            5,
        );
        assert!(!hits.is_empty());
        assert_eq!(hits[0].session_id, "answer_gold");
    }
}
