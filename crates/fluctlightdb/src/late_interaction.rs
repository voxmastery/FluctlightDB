//! Late-interaction retrieval primitives for the CHORUS lane.
//!
//! Two complementary channels, fused by Reciprocal Rank Fusion, mirror the
//! honest LoCoMo recipe (see `benchmarks/locomo_lateinteraction.py`, 96.3% raw):
//!
//! 1. **MaxSim** — token-population late interaction over MiniLM per-token vectors.
//!    A mean-pooled sentence vector collapses the transformer's per-token
//!    population code into one centroid and loses most discriminative signal.
//!    MaxSim keeps the tokens and scores `sum_i max_j cos(q_i, d_j)` — the
//!    distributed-population match (Georgopoulos population vector; hippocampal
//!    ensemble pattern completion) instead of the collapsed mean rate.
//! 2. **BM25** — sparse lexical channel over stored `content`, catching exact
//!    names/dates/quoted phrases the embedder misses.
//!
//! Document token vectors are stored as IEEE-754 **half-precision** (`u16` bit
//! patterns) and **capped** at [`TOKEN_CAP`] tokens/trace to bound the ~35x
//! per-token storage cost. Query token vectors stay f32.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Max tokens stored per trace for MaxSim (bounds the per-token memory blow-up).
/// ~37 tokens/turn on LoCoMo; 48 covers the vast majority with headroom.
pub const TOKEN_CAP: usize = 48;

// ── IEEE-754 half-precision pack/unpack (no external crate) ──────────────────

/// Convert an f32 to its IEEE-754 binary16 bit pattern (round-toward-zero on the
/// mantissa — precision loss is negligible for L2-normalized embedding components).
pub fn f32_to_f16_bits(value: f32) -> u16 {
    let bits = value.to_bits();
    let sign = ((bits >> 16) & 0x8000) as u16;
    let exp = ((bits >> 23) & 0xff) as i32 - 127 + 15;
    let mant = bits & 0x007f_ffff;
    if exp <= 0 {
        // subnormal / underflow to zero
        if exp < -10 {
            return sign;
        }
        let mant = (mant | 0x0080_0000) >> (1 - exp);
        return sign | (mant >> 13) as u16;
    } else if exp >= 0x1f {
        // overflow / inf / nan
        if mant != 0 && ((bits >> 23) & 0xff) == 0xff {
            return sign | 0x7e00; // nan
        }
        return sign | 0x7c00; // inf
    }
    sign | ((exp as u16) << 10) | (mant >> 13) as u16
}

/// Convert an IEEE-754 binary16 bit pattern back to f32.
pub fn f16_bits_to_f32(h: u16) -> f32 {
    let sign = ((h & 0x8000) as u32) << 16;
    let exp = ((h >> 10) & 0x1f) as u32;
    let mant = (h & 0x03ff) as u32;
    let bits = if exp == 0 {
        if mant == 0 {
            sign
        } else {
            // subnormal: normalize
            let mut e = -1i32;
            let mut m = mant;
            while m & 0x0400 == 0 {
                m <<= 1;
                e -= 1;
            }
            m &= 0x03ff;
            let exp32 = (127 - 15 + 1 + e) as u32;
            sign | (exp32 << 23) | (m << 13)
        }
    } else if exp == 0x1f {
        sign | 0x7f80_0000 | (mant << 13) // inf / nan
    } else {
        let exp32 = exp + 127 - 15;
        sign | (exp32 << 23) | (mant << 13)
    };
    f32::from_bits(bits)
}

/// Pack a list of f32 token vectors into capped, half-precision rows.
/// Input vectors are assumed L2-normalized. Keeps at most [`TOKEN_CAP`] tokens.
pub fn pack_tokens(tokens: &[Vec<f32>]) -> Vec<Vec<u16>> {
    tokens
        .iter()
        .take(TOKEN_CAP)
        .map(|t| t.iter().map(|&x| f32_to_f16_bits(x)).collect())
        .collect()
}

// ── MaxSim ───────────────────────────────────────────────────────────────────

/// Late-interaction MaxSim: `sum_i max_j cos(q_i, d_j)`.
/// Both sides assumed L2-normalized (dot == cosine). `doc` rows are f16 bits.
pub fn maxsim(query: &[Vec<f32>], doc: &[Vec<u16>]) -> f32 {
    if query.is_empty() || doc.is_empty() {
        return 0.0;
    }
    // Decode doc rows once.
    let doc_f32: Vec<Vec<f32>> = doc
        .iter()
        .map(|row| row.iter().map(|&b| f16_bits_to_f32(b)).collect())
        .collect();
    let mut total = 0.0f32;
    for q in query {
        let mut best = f32::NEG_INFINITY;
        for d in &doc_f32 {
            let mut dot = 0.0f32;
            for (a, b) in q.iter().zip(d.iter()) {
                dot += a * b;
            }
            if dot > best {
                best = dot;
            }
        }
        if best.is_finite() {
            total += best;
        }
    }
    total
}

/// Predictive-coding salience weights for query tokens: each token's divergence
/// from the query centroid. Content tokens diverge (high weight); function tokens
/// and [CLS]/[SEP] cluster near the centroid (low weight). Normalized to mean 1.
pub fn salience_weights(query: &[Vec<f32>]) -> Vec<f32> {
    let n = query.len();
    if n == 0 {
        return Vec::new();
    }
    let dim = query[0].len();
    let mut c = vec![0.0f32; dim];
    for q in query {
        for (a, b) in c.iter_mut().zip(q.iter()) {
            *a += *b;
        }
    }
    let norm = c.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for x in c.iter_mut() {
            *x /= norm;
        }
    }
    let mut w: Vec<f32> = query
        .iter()
        .map(|q| {
            let dot: f32 = q.iter().zip(c.iter()).map(|(a, b)| a * b).sum();
            (1.0 - dot).max(0.0)
        })
        .collect();
    let mean = w.iter().sum::<f32>() / n as f32;
    if mean > 1e-6 {
        for x in w.iter_mut() {
            *x /= mean;
        }
    } else {
        w.iter_mut().for_each(|x| *x = 1.0);
    }
    w
}

/// Salience-gated MaxSim: `sum_i weight_i * max_j cos(q_i, d_j)`.
pub fn maxsim_weighted(query: &[Vec<f32>], weights: &[f32], doc: &[Vec<u16>]) -> f32 {
    if query.is_empty() || doc.is_empty() {
        return 0.0;
    }
    let doc_f32: Vec<Vec<f32>> = doc
        .iter()
        .map(|row| row.iter().map(|&b| f16_bits_to_f32(b)).collect())
        .collect();
    let mut total = 0.0f32;
    for (qi, q) in query.iter().enumerate() {
        let mut best = f32::NEG_INFINITY;
        for d in &doc_f32 {
            let dot: f32 = q.iter().zip(d.iter()).map(|(a, b)| a * b).sum();
            if dot > best {
                best = dot;
            }
        }
        if best.is_finite() {
            total += weights.get(qi).copied().unwrap_or(1.0) * best;
        }
    }
    total
}

// ── BM25 lexical index (incremental, in-memory) ──────────────────────────────

/// Okapi BM25 index over trace content, updated incrementally on imprint.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Bm25Index {
    /// document frequency per term
    df: HashMap<String, u32>,
    /// per-doc term frequencies
    tf: HashMap<String, HashMap<String, u32>>,
    /// per-doc token length
    len: HashMap<String, u32>,
    /// per-doc term positions (for conjunctive proximity binding)
    #[serde(default)]
    pos: HashMap<String, HashMap<String, Vec<u32>>>,
    total_len: u64,
    k1: f32,
    b: f32,
}

/// Lowercase alphanumeric tokenizer (matches the benchmark harness).
pub fn tokenize(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for ch in text.chars() {
        if ch.is_ascii_alphanumeric() {
            cur.push(ch.to_ascii_lowercase());
        } else if !cur.is_empty() {
            out.push(std::mem::take(&mut cur));
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

impl Bm25Index {
    pub fn new() -> Self {
        Self {
            k1: 1.5,
            b: 0.75,
            ..Default::default()
        }
    }

    pub fn len(&self) -> usize {
        self.len.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len.is_empty()
    }

    /// Add or replace a document. Re-adding the same id first removes its old stats.
    pub fn add(&mut self, id: &str, content: &str) {
        if self.len.contains_key(id) {
            self.remove(id);
        }
        let toks = tokenize(content);
        if toks.is_empty() {
            return;
        }
        let mut tf: HashMap<String, u32> = HashMap::new();
        let mut pos: HashMap<String, Vec<u32>> = HashMap::new();
        for (i, t) in toks.iter().enumerate() {
            *tf.entry(t.clone()).or_insert(0) += 1;
            pos.entry(t.clone()).or_default().push(i as u32);
        }
        for term in tf.keys() {
            *self.df.entry(term.clone()).or_insert(0) += 1;
        }
        self.total_len += toks.len() as u64;
        self.len.insert(id.to_string(), toks.len() as u32);
        self.tf.insert(id.to_string(), tf);
        self.pos.insert(id.to_string(), pos);
    }

    pub fn remove(&mut self, id: &str) {
        if let Some(tf) = self.tf.remove(id) {
            for term in tf.keys() {
                if let Some(c) = self.df.get_mut(term) {
                    *c = c.saturating_sub(1);
                    if *c == 0 {
                        self.df.remove(term);
                    }
                }
            }
        }
        self.pos.remove(id);
        if let Some(l) = self.len.remove(id) {
            self.total_len -= l as u64;
        }
    }

    fn avgdl(&self) -> f32 {
        if self.len.is_empty() {
            1.0
        } else {
            self.total_len as f32 / self.len.len() as f32
        }
    }

    fn idf(&self, term: &str) -> f32 {
        let n = self.len.len() as f32;
        let df = *self.df.get(term).unwrap_or(&0) as f32;
        (1.0 + (n - df + 0.5) / (df + 0.5)).ln()
    }

    /// BM25 score for a query against a single document id (0.0 if unknown).
    pub fn score(&self, query_terms: &[String], id: &str) -> f32 {
        let Some(tf) = self.tf.get(id) else {
            return 0.0;
        };
        let dl = *self.len.get(id).unwrap_or(&0) as f32;
        let avgdl = self.avgdl();
        let mut s = 0.0f32;
        for term in query_terms {
            let Some(&f) = tf.get(term) else { continue };
            let f = f as f32;
            s += self.idf(term) * (f * (self.k1 + 1.0))
                / (f + self.k1 * (1.0 - self.b + self.b * dl / avgdl));
        }
        s
    }

    /// Surprisal information content of a term: -log p(term) = ln(N / df).
    fn idf_surprisal(&self, term: &str) -> f32 {
        let n = self.len.len().max(1) as f32;
        let df = (*self.df.get(term).unwrap_or(&0)).max(1) as f32;
        (n / df).ln()
    }

    /// Conjunctive surprisal: information content with a saturating neural response
    /// (Weber–Fechner, no ad-hoc k1/b) plus a proximity-bound bonus for co-occurring
    /// rare query terms (conjunctive binding). `tau` saturates TF; `window` bounds
    /// the proximity kernel.
    pub fn surprisal_conjunctive(
        &self,
        query_terms: &[String],
        id: &str,
        tau: f32,
        window: u32,
    ) -> f32 {
        let Some(tf) = self.tf.get(id) else {
            return 0.0;
        };
        // dedup query terms present in this doc
        let mut present: Vec<&String> = Vec::new();
        let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
        let mut s = 0.0f32;
        for term in query_terms {
            if !seen.insert(term.as_str()) {
                continue;
            }
            if let Some(&f) = tf.get(term) {
                s += self.idf_surprisal(term) * (1.0 - (-(f as f32) / tau).exp());
                present.push(term);
            }
        }
        // conjunctive binding over co-occurring rare pairs
        if present.len() >= 2 {
            if let Some(pmap) = self.pos.get(id) {
                for a in 0..present.len() {
                    for b in (a + 1)..present.len() {
                        let (Some(pa), Some(pb)) =
                            (pmap.get(present[a].as_str()), pmap.get(present[b].as_str()))
                        else {
                            continue;
                        };
                        let mut gap = u32::MAX;
                        for &x in pa {
                            for &y in pb {
                                gap = gap.min(x.abs_diff(y));
                            }
                        }
                        if gap <= window {
                            let bind = (self.idf_surprisal(present[a])
                                * self.idf_surprisal(present[b]))
                            .sqrt()
                                * (-(gap as f32) / window as f32).exp();
                            s += bind;
                        }
                    }
                }
            }
        }
        s
    }
}

// ── Evidence-integration fusion (Ernst–Banks optimal cue combination) ─────────

/// Fuse two channels by z-scoring each over the candidate set and taking a
/// reliability-weighted sum, instead of RRF's rank heuristic. This preserves
/// score magnitude (which RRF discards), sharpening the top ranks. Both maps
/// must cover the same candidate ids; missing entries are treated as the channel
/// minimum. Returns ids sorted by fused score, descending.
pub fn evidence_fuse(
    dense: &HashMap<String, f32>,
    lex: &HashMap<String, f32>,
    w_lex: f32,
) -> Vec<(String, f32)> {
    let mut ids: Vec<String> = dense.keys().cloned().collect();
    for id in lex.keys() {
        if !dense.contains_key(id) {
            ids.push(id.clone());
        }
    }
    if ids.is_empty() {
        return Vec::new();
    }
    let zscore = |vals: &[f32]| -> Vec<f32> {
        let n = vals.len() as f32;
        let mean = vals.iter().sum::<f32>() / n;
        let var = vals.iter().map(|x| (x - mean).powi(2)).sum::<f32>() / n;
        let sd = var.sqrt() + 1e-9;
        vals.iter().map(|x| (x - mean) / sd).collect()
    };
    let dvals: Vec<f32> = ids.iter().map(|id| *dense.get(id).unwrap_or(&0.0)).collect();
    let lvals: Vec<f32> = ids.iter().map(|id| *lex.get(id).unwrap_or(&0.0)).collect();
    let dz = zscore(&dvals);
    let lz = zscore(&lvals);
    let mut out: Vec<(String, f32)> = ids
        .into_iter()
        .enumerate()
        .map(|(i, id)| (id, dz[i] + w_lex * lz[i]))
        .collect();
    out.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    out
}

// ── Reciprocal Rank Fusion ───────────────────────────────────────────────────

/// RRF over per-channel ranked id lists with per-channel weights.
/// Returns ids sorted by fused score, descending. `rrf_k` is the standard 60.
pub fn rrf_fuse(channels: &[(Vec<String>, f32)], rrf_k: f32) -> Vec<(String, f32)> {
    let mut agg: HashMap<String, f32> = HashMap::new();
    for (ranking, weight) in channels {
        for (rank, id) in ranking.iter().enumerate() {
            *agg.entry(id.clone()).or_insert(0.0) += weight / (rrf_k + rank as f32 + 1.0);
        }
    }
    let mut out: Vec<(String, f32)> = agg.into_iter().collect();
    out.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-2
    }

    #[test]
    fn f16_roundtrip_preserves_unit_components() {
        for &x in &[0.0f32, 1.0, -1.0, 0.5, -0.25, 0.123, -0.789, 0.0313] {
            let back = f16_bits_to_f32(f32_to_f16_bits(x));
            assert!((x - back).abs() < 1e-2, "f16 roundtrip {x} -> {back}");
        }
    }

    #[test]
    fn maxsim_identical_vectors_scores_token_count() {
        // Two query tokens, each exactly matching a distinct doc token => score ~2.0
        let q = vec![vec![1.0, 0.0, 0.0], vec![0.0, 1.0, 0.0]];
        let d = pack_tokens(&[vec![1.0, 0.0, 0.0], vec![0.0, 1.0, 0.0], vec![0.0, 0.0, 1.0]]);
        assert!(approx(maxsim(&q, &d), 2.0), "got {}", maxsim(&q, &d));
    }

    #[test]
    fn maxsim_picks_best_doc_token_per_query_token() {
        let q = vec![vec![1.0, 0.0]];
        // best match is the aligned token (cos 1.0), not the orthogonal one
        let d = pack_tokens(&[vec![0.0, 1.0], vec![1.0, 0.0]]);
        assert!(approx(maxsim(&q, &d), 1.0));
    }

    #[test]
    fn maxsim_empty_is_zero() {
        assert_eq!(maxsim(&[], &pack_tokens(&[vec![1.0]])), 0.0);
        assert_eq!(maxsim(&[vec![1.0]], &[]), 0.0);
    }

    #[test]
    fn pack_caps_tokens() {
        let toks: Vec<Vec<f32>> = (0..100).map(|_| vec![1.0, 0.0]).collect();
        assert_eq!(pack_tokens(&toks).len(), TOKEN_CAP);
    }

    #[test]
    fn bm25_rewards_rare_term_match() {
        let mut ix = Bm25Index::new();
        ix.add("d1", "the cat sat on the mat");
        ix.add("d2", "the dog ran in the park");
        ix.add("d3", "the bird flew over the tree");
        let q = tokenize("cat");
        assert!(ix.score(&q, "d1") > ix.score(&q, "d2"));
        assert_eq!(ix.score(&q, "d2"), 0.0);
    }

    #[test]
    fn bm25_remove_updates_stats() {
        let mut ix = Bm25Index::new();
        ix.add("d1", "alpha beta");
        ix.add("d2", "alpha gamma");
        assert_eq!(ix.len(), 2);
        ix.remove("d1");
        assert_eq!(ix.len(), 1);
        // 'beta' df should be gone
        let q = tokenize("beta");
        assert_eq!(ix.score(&q, "d2"), 0.0);
    }

    #[test]
    fn bm25_readd_replaces() {
        let mut ix = Bm25Index::new();
        ix.add("d1", "alpha alpha alpha");
        ix.add("d1", "beta"); // replace
        assert_eq!(ix.len(), 1);
        assert_eq!(ix.score(&tokenize("alpha"), "d1"), 0.0);
        assert!(ix.score(&tokenize("beta"), "d1") > 0.0);
    }

    #[test]
    fn rrf_fuses_channels_by_rank() {
        // doc A tops channel 1, doc B tops channel 2; equal weight => A and B near top
        let ch1 = (vec!["A".to_string(), "B".to_string(), "C".to_string()], 1.0);
        let ch2 = (vec!["B".to_string(), "C".to_string(), "A".to_string()], 1.0);
        let fused = rrf_fuse(&[ch1, ch2], 60.0);
        assert_eq!(fused.len(), 3);
        // B is rank0+rank1 highest combined
        assert_eq!(fused[0].0, "B");
    }

    #[test]
    fn rrf_weight_downweights_channel() {
        let ch1 = (vec!["A".to_string(), "B".to_string()], 1.0);
        let ch2 = (vec!["B".to_string(), "A".to_string()], 0.1);
        let fused = rrf_fuse(&[ch1, ch2], 60.0);
        assert_eq!(fused[0].0, "A"); // strong channel wins
    }

    #[test]
    fn salience_downweights_generic_tokens() {
        // token near the centroid (generic) gets low weight; divergent one high.
        let q = vec![vec![1.0, 0.0], vec![0.0, 1.0], vec![1.0, 0.0]];
        let w = salience_weights(&q);
        assert_eq!(w.len(), 3);
        // mean-normalized => average ~1
        assert!(approx(w.iter().sum::<f32>() / 3.0, 1.0));
        // the lone divergent token (index 1) should weigh more than the aligned pair
        assert!(w[1] > w[0] && w[1] > w[2]);
    }

    #[test]
    fn maxsim_weighted_scales_by_weight() {
        let q = vec![vec![1.0, 0.0], vec![0.0, 1.0]];
        let d = pack_tokens(&[vec![1.0, 0.0], vec![0.0, 1.0]]);
        let plain = maxsim(&q, &d);
        let weighted = maxsim_weighted(&q, &[2.0, 0.0], &d);
        assert!(approx(plain, 2.0));
        assert!(approx(weighted, 2.0)); // 2*1 + 0*1
    }

    #[test]
    fn conjunctive_rewards_proximity() {
        let mut ix = Bm25Index::new();
        ix.add("near", "alpha beta gamma delta"); // rare terms adjacent-ish
        ix.add("far", "alpha zzz yyy xxx www vvv uuu ttt beta"); // rare terms far apart
        // pad corpus so alpha/beta are rare
        for i in 0..20 {
            ix.add(&format!("pad{i}"), "common common common");
        }
        let q = tokenize("alpha beta");
        let near = ix.surprisal_conjunctive(&q, "near", 1.0, 8);
        let far = ix.surprisal_conjunctive(&q, "far", 1.0, 8);
        assert!(near > far, "near={near} far={far}");
    }

    #[test]
    fn evidence_fuse_preserves_magnitude() {
        let mut dense = HashMap::new();
        dense.insert("A".to_string(), 10.0);
        dense.insert("B".to_string(), 1.0);
        dense.insert("C".to_string(), 0.5);
        let mut lex = HashMap::new();
        lex.insert("A".to_string(), 0.0);
        lex.insert("B".to_string(), 5.0);
        lex.insert("C".to_string(), 0.0);
        let fused = evidence_fuse(&dense, &lex, 0.6);
        assert_eq!(fused.len(), 3);
        // A dominates dense strongly => stays on top
        assert_eq!(fused[0].0, "A");
    }
}
