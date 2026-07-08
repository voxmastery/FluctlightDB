//! Photon Lane — binary bitcode recall at the speed of XOR + popcount.
//!
//! # Why this exists
//! Float embeddings are the bottleneck in the recall hot path: a 384-d cosine is ~384 multiplies
//! and adds per candidate, and every candidate must be touched. The brain does not compare dense
//! analog vectors to filter memories — it uses **sparse, binary spike patterns** and matches them
//! with cheap coincidence detection. Photon Lane is that filter: it collapses a float embedding to
//! a compact **bitcode** via SimHash (Charikar 2002, random-hyperplane LSH), so similarity becomes
//! `popcount(a XOR b)` — a handful of native 64-bit instructions instead of hundreds of float ops.
//!
//! Two guarantees make this safe as a *prefilter*:
//!   1. **Cosine-preserving.** For random hyperplanes, `P(bit differs) = angle(a,b) / π`, so
//!      Hamming distance is a monotone estimator of angular distance — ordering by Hamming agrees
//!      with ordering by cosine. `estimated_cosine()` inverts it.
//!   2. **Sub-linear candidate generation.** LSH banding groups bitcodes so that near-duplicates
//!      collide in at least one band; a query only rescans colliding buckets, not the whole store.
//!
//! Pipeline: `bitcode prefilter (LSH bands) → exact Hamming top-k → hand survivors to the float /
//! lattice / phase stages`. The expensive analog comparison runs on a short list, not the corpus.
//!
//! Deterministic, pure-`std` (+ `serde`), standalone. Validates the speed physics before wiring.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Default bitcode width in bits (4 × u64 words). Multiple of 64.
pub const DEFAULT_BITS: usize = 256;

/// A packed binary fingerprint: `bits` significant bits stored in u64 words.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PhotonCode {
    pub words: Vec<u64>,
    pub bits: usize,
}

impl PhotonCode {
    /// Hamming distance = number of differing bits (XOR + popcount).
    pub fn hamming(&self, other: &PhotonCode) -> u32 {
        self.words
            .iter()
            .zip(&other.words)
            .map(|(a, b)| (a ^ b).count_ones())
            .sum()
    }

    /// SimHash cosine estimate from Hamming distance: `cos(π · hamming / bits)`.
    pub fn estimated_cosine(&self, other: &PhotonCode) -> f32 {
        if self.bits == 0 {
            return 0.0;
        }
        let frac = self.hamming(other) as f32 / self.bits as f32;
        (std::f32::consts::PI * frac).cos()
    }

    /// Extract the `band`-th block of `rows` bits as an LSH bucket key.
    pub fn band_key(&self, band: usize, rows: usize) -> u64 {
        let start = band * rows;
        let mut key: u64 = 0;
        for r in 0..rows {
            let bit = start + r;
            if bit >= self.bits {
                break;
            }
            let word = bit / 64;
            let off = bit % 64;
            let set = (self.words[word] >> off) & 1;
            key |= set << r;
        }
        // Salt with band index so identical bit-patterns in different bands don't alias.
        hash64(&[band as u64, key])
    }
}

/// SimHash encoder: fixed random hyperplanes (generated deterministically from a seed).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimHasher {
    pub bits: usize,
    pub seed: u64,
}

impl Default for SimHasher {
    fn default() -> Self {
        Self {
            bits: DEFAULT_BITS,
            seed: 0x000F_10C7,
        }
    }
}

impl SimHasher {
    pub fn new(bits: usize, seed: u64) -> Self {
        assert!(bits > 0 && bits.is_multiple_of(64), "bits must be a positive multiple of 64");
        Self { bits, seed }
    }

    /// Collapse a float embedding to a bitcode: bit_b = sign(vector · hyperplane_b).
    pub fn encode(&self, vector: &[f32]) -> PhotonCode {
        let n_words = self.bits / 64;
        let mut words = vec![0u64; n_words];
        for b in 0..self.bits {
            let mut acc = 0.0f32;
            for (i, &v) in vector.iter().enumerate() {
                // Rademacher (±1) hyperplane entry. splitmix64 gives a well-mixed low bit;
                // a plain FNV hash of sequential (b,i) leaves bit-0 correlated → degenerate planes.
                let h = splitmix64(hash64(&[self.seed, b as u64, i as u64]));
                acc += if h & 1 == 0 { v } else { -v };
            }
            if acc >= 0.0 {
                words[b / 64] |= 1u64 << (b % 64);
            }
        }
        PhotonCode {
            words,
            bits: self.bits,
        }
    }
}

/// LSH-banded bitcode store: near-duplicates collide in ≥1 band → sub-linear candidate scan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhotonStore {
    pub bits: usize,
    pub bands: usize,
    pub rows: usize,
    codes: Vec<(String, PhotonCode)>,
    buckets: Vec<HashMap<u64, Vec<usize>>>,
}

impl PhotonStore {
    /// `bands × rows` should be ≤ bits. More bands → higher recall, more candidates.
    pub fn new(bits: usize, bands: usize, rows: usize) -> Self {
        Self {
            bits,
            bands,
            rows,
            codes: Vec::new(),
            buckets: vec![HashMap::new(); bands],
        }
    }

    pub fn len(&self) -> usize {
        self.codes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.codes.is_empty()
    }

    /// Lookup id + code by store index (for GRG rerank after multi-probe).
    pub fn entry_at(&self, idx: usize) -> Option<(&str, &PhotonCode)> {
        self.codes.get(idx).map(|(id, code)| (id.as_str(), code))
    }

    pub fn insert(&mut self, id: impl Into<String>, code: PhotonCode) {
        let idx = self.codes.len();
        for band in 0..self.bands {
            let key = code.band_key(band, self.rows);
            self.buckets[band].entry(key).or_default().push(idx);
        }
        self.codes.push((id.into(), code));
    }

    /// Candidate indices whose bitcode collides with the cue in at least one band.
    pub fn candidates(&self, cue: &PhotonCode) -> Vec<usize> {
        self.multi_probe_candidates(cue, usize::MAX, 0)
    }

    /// Multi-probe LSH (Lv et al. VLDB 2007): primary buckets + 1-bit key perturbations.
    pub fn multi_probe_candidates(
        &self,
        cue: &PhotonCode,
        max_candidates: usize,
        probe_bits: usize,
    ) -> Vec<usize> {
        if self.codes.is_empty() {
            return Vec::new();
        }
        let cap = if max_candidates == usize::MAX {
            self.codes.len()
        } else {
            max_candidates
        };
        let mut seen = vec![false; self.codes.len()];
        let mut out = Vec::new();

        let mut push_bucket = |band: usize, key: u64| -> bool {
            if let Some(idxs) = self.buckets[band].get(&key) {
                for &i in idxs {
                    if !seen[i] {
                        seen[i] = true;
                        out.push(i);
                        if out.len() >= cap {
                            return true;
                        }
                    }
                }
            }
            false
        };

        for band in 0..self.bands {
            let key = cue.band_key(band, self.rows);
            if push_bucket(band, key) {
                return out;
            }
            for bit in 0..probe_bits.min(self.rows) {
                let probe_key = key ^ (1u64 << bit);
                if push_bucket(band, probe_key) {
                    return out;
                }
            }
        }
        out
    }

    /// IVF-lite coarse gate: band-0 cell + 1-bit neighbor cells (for corpora > exact-scan budget).
    pub fn ivf_coarse_candidates(
        &self,
        cue: &PhotonCode,
        max_candidates: usize,
        neighbor_bits: usize,
    ) -> Vec<usize> {
        if self.codes.is_empty() || self.bands == 0 {
            return Vec::new();
        }
        let cap = max_candidates.min(self.codes.len());
        let band = 0usize;
        let key = cue.band_key(band, self.rows);
        let mut seen = vec![false; self.codes.len()];
        let mut out = Vec::new();

        let mut push_key = |k: u64| -> bool {
            if let Some(idxs) = self.buckets[band].get(&k) {
                for &i in idxs {
                    if !seen[i] {
                        seen[i] = true;
                        out.push(i);
                        if out.len() >= cap {
                            return true;
                        }
                    }
                }
            }
            false
        };

        if push_key(key) {
            return out;
        }
        for bit in 0..neighbor_bits.min(self.rows) {
            if push_key(key ^ (1u64 << bit)) {
                return out;
            }
        }
        out
    }

    /// Prefilter by LSH bands, then exact Hamming rerank → top-k `(id, hamming)`.
    pub fn query(&self, cue: &PhotonCode, k: usize) -> Vec<(String, u32)> {
        let mut scored: Vec<(String, u32)> = self
            .candidates(cue)
            .into_iter()
            .map(|i| (self.codes[i].0.clone(), cue.hamming(&self.codes[i].1)))
            .collect();
        scored.sort_by_key(|(_, h)| *h);
        scored.truncate(k);
        scored
    }

    /// Nearest neighbor by Hamming distance (full scan).
    pub fn nearest(&self, cue: &PhotonCode) -> Option<(String, PhotonCode)> {
        self.codes
            .iter()
            .map(|(id, c)| (id.clone(), cue.hamming(c), c.clone()))
            .min_by_key(|(_, h, _)| *h)
            .map(|(id, _, c)| (id, c))
    }

    /// Drop entries whose id is not in `keep`.
    pub fn retain(&mut self, keep: impl Fn(&str) -> bool) {
        let mut next: Vec<(String, PhotonCode)> = Vec::new();
        for (id, code) in self.codes.drain(..) {
            if keep(&id) {
                next.push((id, code));
            }
        }
        self.codes = next;
        self.buckets = vec![HashMap::new(); self.bands];
        for (idx, (_, code)) in self.codes.iter().enumerate() {
            for band in 0..self.bands {
                let key = code.band_key(band, self.rows);
                self.buckets[band].entry(key).or_default().push(idx);
            }
        }
    }

    /// Brute-force exact Hamming top-k (no LSH) — reference for recall measurement.
    pub fn query_exact(&self, cue: &PhotonCode, k: usize) -> Vec<(String, u32)> {
        let mut scored: Vec<(String, u32)> = self
            .codes
            .iter()
            .map(|(id, c)| (id.clone(), cue.hamming(c)))
            .collect();
        scored.sort_by_key(|(_, h)| *h);
        scored.truncate(k);
        scored
    }
}

fn hash64(parts: &[u64]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for &p in parts {
        h ^= p;
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

    fn cosine(a: &[f32], b: &[f32]) -> f32 {
        let n = a.len().min(b.len());
        let (mut dot, mut na, mut nb) = (0.0f32, 0.0f32, 0.0f32);
        for i in 0..n {
            dot += a[i] * b[i];
            na += a[i] * a[i];
            nb += b[i] * b[i];
        }
        if na <= 1e-8 || nb <= 1e-8 {
            0.0
        } else {
            dot / na.sqrt() / nb.sqrt()
        }
    }

    // Deterministic pseudo-random vector generator for tests.
    // Reduce the hash to a small range BEFORE the f32 cast — a full u64→f32 cast loses
    // precision and clusters near 1.0, which would make every test vector near-identical.
    fn rand_vec(seed: u64, dim: usize) -> Vec<f32> {
        (0..dim)
            .map(|i| {
                let h = hash64(&[seed, i as u64]);
                ((h % 20001) as f32 / 10000.0) - 1.0
            })
            .collect()
    }

    #[test]
    fn encoding_is_stable() {
        let sh = SimHasher::default();
        let v = rand_vec(1, 128);
        assert_eq!(sh.encode(&v), sh.encode(&v));
    }

    #[test]
    fn hamming_is_xor_popcount() {
        let a = PhotonCode { words: vec![0b1011, 0], bits: 128 };
        let b = PhotonCode { words: vec![0b1110, 0], bits: 128 };
        assert_eq!(a.hamming(&b), 2); // bits 0 and 2 differ
        assert_eq!(a.hamming(&a), 0);
    }

    #[test]
    fn near_duplicate_is_closer_than_orthogonal() {
        let sh = SimHasher::new(256, 42);
        let v = rand_vec(7, 128);
        // Near-duplicate: v + tiny noise.
        let near: Vec<f32> = v.iter().enumerate().map(|(i, &x)| x + rand_vec(99, 128)[i] * 0.01).collect();
        let far = rand_vec(8888, 128);

        let cv = sh.encode(&v);
        let cn = sh.encode(&near);
        let cf = sh.encode(&far);
        assert!(
            cv.hamming(&cn) < cv.hamming(&cf),
            "near {} should be < far {}",
            cv.hamming(&cn),
            cv.hamming(&cf)
        );
    }

    #[test]
    fn hamming_order_agrees_with_cosine_order() {
        // Rank candidates by Hamming; must match ranking by true cosine (SimHash guarantee).
        let sh = SimHasher::new(512, 5);
        let query = rand_vec(1, 128);
        let cands: Vec<Vec<f32>> = (0..12).map(|s| rand_vec(1000 + s, 128)).collect();

        let mut by_cos: Vec<usize> = (0..cands.len()).collect();
        by_cos.sort_by(|&i, &j| {
            cosine(&query, &cands[j])
                .partial_cmp(&cosine(&query, &cands[i]))
                .unwrap()
        });

        let qc = sh.encode(&query);
        let codes: Vec<PhotonCode> = cands.iter().map(|c| sh.encode(c)).collect();
        let mut by_ham: Vec<usize> = (0..cands.len()).collect();
        by_ham.sort_by_key(|&i| qc.hamming(&codes[i]));

        // Top-3 nearest by cosine and by Hamming should overlap heavily.
        let cos_top: std::collections::HashSet<_> = by_cos[..3].iter().collect();
        let ham_top: std::collections::HashSet<_> = by_ham[..3].iter().collect();
        let overlap = cos_top.intersection(&ham_top).count();
        assert!(overlap >= 2, "top-3 overlap only {overlap}: cos {by_cos:?} ham {by_ham:?}");
    }

    #[test]
    fn estimated_cosine_tracks_true_cosine() {
        let sh = SimHasher::new(1024, 3);
        let a = rand_vec(11, 128);
        let b = rand_vec(22, 128);
        let est = sh.encode(&a).estimated_cosine(&sh.encode(&b));
        let truth = cosine(&a, &b);
        assert!((est - truth).abs() < 0.15, "est {est} vs truth {truth}");
    }

    #[test]
    fn lsh_prefilter_recalls_near_neighbor_and_reranks_top1() {
        let sh = SimHasher::new(256, 17);
        let mut store = PhotonStore::new(256, 32, 8);

        // Planted target and its near-duplicate; plus many distractors.
        let target = rand_vec(500, 128);
        store.insert("target", sh.encode(&target));
        for s in 0..300 {
            store.insert(format!("d{s}"), sh.encode(&rand_vec(s, 128)));
        }
        // Query = target + small noise.
        let query: Vec<f32> = target.iter().enumerate().map(|(i, &x)| x + rand_vec(7, 128)[i] * 0.02).collect();
        let qc = sh.encode(&query);

        // LSH prefilter must surface the target as a candidate, and rerank it to top-1.
        let cands = store.candidates(&qc);
        assert!(!cands.is_empty(), "LSH returned no candidates");
        let top = store.query(&qc, 1);
        assert_eq!(top[0].0, "target", "top-1 after rerank: {top:?}");

        // Prefilter should scan far fewer than the full corpus (sub-linear win).
        assert!(cands.len() < store.len(), "prefilter scanned everything: {}/{}", cands.len(), store.len());
    }

    #[test]
    fn multi_probe_recalls_more_than_single_probe() {
        let sh = SimHasher::new(256, 17);
        let mut store = PhotonStore::new(256, 32, 8);
        let target = rand_vec(500, 128);
        store.insert("target", sh.encode(&target));
        for s in 0..300 {
            store.insert(format!("d{s}"), sh.encode(&rand_vec(s, 128)));
        }
        let query: Vec<f32> = target
            .iter()
            .enumerate()
            .map(|(i, &x)| x + rand_vec(7, 128)[i] * 0.02)
            .collect();
        let qc = sh.encode(&query);
        let single = store.multi_probe_candidates(&qc, usize::MAX, 0);
        let multi = store.multi_probe_candidates(&qc, usize::MAX, 4);
        assert!(multi.len() >= single.len());
        assert!(multi.iter().any(|&i| store.entry_at(i).unwrap().0 == "target"));
    }

    #[test]
    fn ivf_coarse_surfaces_neighbors() {
        let sh = SimHasher::new(256, 91);
        let mut store = PhotonStore::new(256, 32, 8);
        let anchor = rand_vec(1, 128);
        store.insert("anchor", sh.encode(&anchor));
        for s in 0..200 {
            store.insert(format!("n{s}"), sh.encode(&rand_vec(s, 128)));
        }
        let qc = sh.encode(&anchor);
        let coarse = store.ivf_coarse_candidates(&qc, 64, 3);
        assert!(!coarse.is_empty());
    }
}
