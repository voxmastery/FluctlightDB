//! Muon Lane — penetrative bulk imprint at the speed of one pass per session.
//!
//! # Why this exists
//! Photon Lane ([`crate::photon`]) makes **read** sub-millisecond: XOR + popcount over a bitcode
//! prefilter. The haystack bottleneck is **write**: hundreds of `experience()` calls, each running
//! dentate separation, graph wiring, WAL, and (often) an embedding HTTP round-trip. That is the
//! opposite physics — stopping at every turn like a particle that collides with everything.
//!
//! **Muons** penetrate bulk matter: they traverse kilometers of rock while interacting with almost
//! nothing. Muon Lane is the write-path analogue: one **penetrative pass** through an entire chat
//! session, leaving a compact **session imprint** (sketch + bitcode + reel) without per-turn
//! hippocampal encode.
//!
//! ## Mechanism: Count-Sketch → SimHash Session Imprint (CSSI)
//! 1. **Count-Sketch** — hash char-trigrams and tokens into a fixed pseudo-vector (no embed server).
//! 2. **SimHash bitcode** — collapse sketch to a [`PhotonCode`] for LSH session retrieval.
//! 3. **Session reel** — full transcript stored once, keyed by `session_id` (lazy hydrate on read).
//! 4. **Penetrative recall** — query sketch → LSH bands → Hamming + lexical Jaccard on user keys.
//!
//! Pipeline: `bulk imprint (O(sessions)) → penetrative recall (O(log N)) → optional lazy crystallize`.
//! Deferred hippocampal split: only top-1 session can be promoted to full engrams on demand.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::photon::{PhotonCode, PhotonStore, SimHasher};
use crate::recall_fabric::structural_boost;

/// Default sketch dimension (feature-hash width).
pub const DEFAULT_SKETCH_DIM: usize = 128;

/// One session's penetrative imprint — the muon trace through bulk history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MuonImprint {
    pub session_id: String,
    pub date: String,
    /// User-turn key block (preference signals) — weighted 2× in sketch.
    pub user_keys: String,
    /// Char-trigram + token count-sketch → SimHash.
    pub code: PhotonCode,
    /// Token bag for lexical rescoring (cheap Jaccard).
    pub tokens: Vec<String>,
    /// Full session body for lazy hydrate / reader context.
    pub reel: String,
}

/// Scored session hit from penetrative recall.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MuonHit {
    pub session_id: String,
    pub score: f32,
    pub photon: f32,
    pub lexical: f32,
    pub phase: f32,
    pub date: String,
    /// Snippet for display / benchmark recalls.
    pub snippet: String,
}

/// Bulk session store + LSH index. Runtime-only (serde for tests).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MuonLane {
    sketch_dim: usize,
    photon_bits: usize,
    hasher: SimHasher,
    prefilter: PhotonStore,
    imprints: HashMap<String, MuonImprint>,
}

impl Default for MuonLane {
    fn default() -> Self {
        Self::new(DEFAULT_SKETCH_DIM, 256)
    }
}

impl MuonLane {
    pub fn new(sketch_dim: usize, photon_bits: usize) -> Self {
        Self {
            sketch_dim,
            photon_bits,
            hasher: SimHasher::new(photon_bits, 0x4D00_114E),
            prefilter: PhotonStore::new(photon_bits, 32, 8),
            imprints: HashMap::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.imprints.len()
    }

    pub fn is_empty(&self) -> bool {
        self.imprints.is_empty()
    }

    /// Penetrative imprint: one call per session (no per-turn hippocampal encode).
    pub fn imprint(
        &mut self,
        session_id: impl Into<String>,
        date: impl Into<String>,
        body: &str,
        user_keys: &str,
    ) {
        let session_id = session_id.into();
        let date = date.into();
        let sketch = count_sketch(body, user_keys, self.sketch_dim);
        let code = self.hasher.encode(&sketch);
        let tokens = token_bag(body, user_keys);
        let imprint = MuonImprint {
            session_id: session_id.clone(),
            date: date.clone(),
            user_keys: user_keys.to_string(),
            code: code.clone(),
            tokens,
            reel: body.to_string(),
        };
        if self.imprints.contains_key(&session_id) {
            // Replace: remove old LSH entry by rebuilding index is expensive; re-insert overwrites map
            // and we append a new photon slot (same id string → query still works via imprints map).
            let idx = self.prefilter.len();
            self.prefilter.insert(session_id.clone(), code);
            let _ = idx;
        } else {
            self.prefilter.insert(session_id.clone(), code);
        }
        self.imprints.insert(session_id, imprint);
    }

    /// Bulk imprint many sessions in one batch (benchmark haystack replacement).
    pub fn imprint_batch(&mut self, sessions: &[MuonImprintInput]) -> usize {
        self.clear();
        for s in sessions {
            self.imprint(&s.session_id, &s.date, &s.body, &s.user_keys);
        }
        sessions.len()
    }

    pub fn clear(&mut self) {
        self.imprints.clear();
        self.prefilter = PhotonStore::new(self.photon_bits, 32, 8);
    }

    /// Penetrative recall: sketch cue → LSH → Hamming + lexical + phase, top-k sessions.
    pub fn recall(&self, cue: &str, k: usize) -> Vec<MuonHit> {
        if self.imprints.is_empty() {
            return Vec::new();
        }
        let sketch = count_sketch(cue, cue, self.sketch_dim);
        let qc = self.hasher.encode(&sketch);
        let cue_tokens = token_bag(cue, cue);

        let score_session = |imp: &MuonImprint| {
            let photon = qc.estimated_cosine(&imp.code).max(0.0);
            let lexical = jaccard(&cue_tokens, &imp.tokens);
            let woverlap = weighted_term_overlap(cue, &imp.reel);
            let phase = structural_boost(cue, &imp.user_keys, 256);
            let keys_lex = jaccard(&cue_tokens, &token_bag(&imp.user_keys, ""));
            let score = 0.15 * photon + 0.25 * lexical + 0.30 * woverlap + 0.15 * phase + 0.15 * keys_lex;
            MuonHit {
                session_id: imp.session_id.clone(),
                score,
                photon,
                lexical: lexical.max(woverlap),
                phase,
                date: imp.date.clone(),
                snippet: if !imp.user_keys.is_empty() {
                    imp.user_keys.chars().take(420).collect()
                } else {
                    imp.reel.chars().take(420).collect()
                },
            }
        };

        // LongMemEval-scale haystacks (~50 sessions): brute all — still sub-ms.
        let mut scored: Vec<MuonHit> = if self.imprints.len() <= 80 {
            self.imprints.values().map(score_session).collect()
        } else {
            self.prefilter
                .query(&qc, self.imprints.len().min(256))
                .into_iter()
                .filter_map(|(sid, hamming)| {
                    let imp = self.imprints.get(&sid)?;
                    let mut hit = score_session(imp);
                    hit.photon = 1.0 - (hamming as f32 / self.photon_bits as f32);
                    hit.score = 0.15 * hit.photon
                        + 0.25 * hit.lexical
                        + 0.30 * weighted_term_overlap(cue, &imp.reel)
                        + 0.15 * hit.phase
                        + 0.15 * jaccard(&cue_tokens, &token_bag(&imp.user_keys, ""));
                    Some(hit)
                })
                .collect()
        };

        if scored.is_empty() {
            scored = self.imprints.values().map(score_session).collect();
        }

        scored.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        scored.truncate(k);
        scored
    }

    pub fn get_reel(&self, session_id: &str) -> Option<&str> {
        self.imprints.get(session_id).map(|i| i.reel.as_str())
    }
}

/// Input for bulk imprint API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MuonImprintInput {
    pub session_id: String,
    pub date: String,
    pub body: String,
    pub user_keys: String,
}

/// Count-Sketch: hash tokens + char-trigrams into a signed sparse vector (no embed server).
pub fn count_sketch(body: &str, user_keys: &str, dim: usize) -> Vec<f32> {
    let dim = dim.max(16);
    let mut v = vec![0.0f32; dim];
    for tok in token_bag(body, "") {
        add_hash(&mut v, &tok, 1.0);
    }
    for tok in token_bag(user_keys, "") {
        add_hash(&mut v, &tok, 2.0);
    }
    let lower: String = body.to_lowercase();
    let chars: Vec<char> = lower
        .chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .collect();
    for win in chars.windows(3) {
        let tri: String = win.iter().collect();
        add_hash(&mut v, &tri, 0.5);
    }
    let mut norm = 0.0f32;
    for x in &v {
        norm += x * x;
    }
    if norm > 1e-8 {
        let inv = 1.0 / norm.sqrt();
        for x in &mut v {
            *x *= inv;
        }
    }
    v
}

fn add_hash(v: &mut [f32], key: &str, weight: f32) {
    let h = fnv1a(key.as_bytes());
    let idx = (splitmix64(h) as usize) % v.len();
    let sign = if splitmix64(h ^ 0x9E37) & 1 == 0 {
        1.0f32
    } else {
        -1.0f32
    };
    v[idx] += sign * weight;
}

pub fn token_bag(body: &str, user_keys: &str) -> Vec<String> {
    let mut set: HashSet<String> = HashSet::new();
    for text in [body, user_keys] {
        for w in text.split(|c: char| !c.is_alphanumeric()) {
            let w = w.to_lowercase();
            if w.len() >= 2 {
                set.insert(w);
            }
        }
    }
    let mut out: Vec<String> = set.into_iter().collect();
    out.sort_unstable();
    out
}

pub fn jaccard(a: &[String], b: &[String]) -> f32 {
    if a.is_empty() && b.is_empty() {
        return 0.0;
    }
    let sa: HashSet<&String> = a.iter().collect();
    let sb: HashSet<&String> = b.iter().collect();
    let inter = sa.intersection(&sb).count() as f32;
    let union = sa.union(&sb).count() as f32;
    if union <= 0.0 {
        0.0
    } else {
        inter / union
    }
}

/// Weighted query-term overlap — stronger than Jaccard for short questions vs long turns.
pub fn weighted_term_overlap(cue: &str, content: &str) -> f32 {
    weighted_term_overlap_lower(cue, &content.to_lowercase())
}

/// Same as [`weighted_term_overlap`] when `content` is already lowercased.
pub fn weighted_term_overlap_lower(cue: &str, content_lower: &str) -> f32 {
    const STOP: &[&str] = &[
        "what", "when", "where", "which", "that", "this", "with", "from", "have", "your", "about",
        "the", "and", "for", "did", "was", "were", "are", "you", "user", "name", "tell", "does",
    ];
    let ct = content_lower;
    let mut weight_sum = 0.0f32;
    let mut hit = 0.0f32;
    for w in cue.split(|c: char| !c.is_alphanumeric()) {
        let w = w.to_lowercase();
        if w.len() < 3 || STOP.contains(&w.as_str()) {
            continue;
        }
        let wt = if w.len() >= 7 {
            1.6
        } else if w.len() >= 5 {
            1.2
        } else {
            1.0
        };
        weight_sum += wt;
        if ct.contains(&w) {
            hit += wt;
        }
    }
    if weight_sum <= 0.0 {
        0.0
    } else {
        hit / weight_sum
    }
}

fn fnv1a(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

fn splitmix64(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9E3779B97F4A7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
    z ^ (z >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session_body() -> &'static str {
        "user: I graduated with a degree in Business Administration.\n\
         assistant: Congratulations! How is the new role?"
    }

    #[test]
    fn imprint_and_recall_finds_planted_session() {
        let mut lane = MuonLane::default();
        lane.imprint(
            "answer_gold",
            "2023-05-29",
            session_body(),
            "user: I graduated with a degree in Business Administration.",
        );
        for i in 0..40 {
            lane.imprint(
                format!("noise_{i}"),
                "2023-01-01",
                &format!("user: random chatter about topic number {i} and nothing relevant"),
                "",
            );
        }
        let hits = lane.recall(
            "What degree did the user graduate with? Business Administration",
            3,
        );
        assert!(!hits.is_empty());
        assert_eq!(hits[0].session_id, "answer_gold");
    }

    #[test]
    fn bulk_imprint_is_one_pass_per_session() {
        let mut lane = MuonLane::default();
        let batch = (0..50)
            .map(|i| MuonImprintInput {
                session_id: format!("s{i}"),
                date: "2023-06-01".into(),
                body: format!("user: session {i} content about item {i}"),
                user_keys: format!("user: item {i}"),
            })
            .collect::<Vec<_>>();
        assert_eq!(lane.imprint_batch(&batch), 50);
        assert_eq!(lane.len(), 50);
    }

    #[test]
    fn count_sketch_is_deterministic() {
        let a = count_sketch("hello world", "hello", 64);
        let b = count_sketch("hello world", "hello", 64);
        assert_eq!(a, b);
    }
}
