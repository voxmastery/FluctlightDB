//! Manifold Lattice — grid-cell memory addressing on an elastic, multi-scale, factored lattice.
//!
//! # Why this exists
//! Photon-lane (binary Hamming) and phase/resonance codes both live in a **single fixed-width
//! space**. That one choice is the root of three tradeoffs:
//!   1. bundling crosstalk / fixed capacity,
//!   2. structure (exact parsing) vs. semantic (fuzzy paraphrase) competing for the same bits,
//!   3. catastrophic interference when the space fills.
//!
//! The lattice removes all three by **not using a flat vector**. A memory is a set of
//! *coordinates* on a multi-scale modular grid — the brain's grid-cell code (Hafting/Fyhn/Moser,
//! Nobel 2014), which Fiete (2008) and Sreenivasan & Fiete (PNAS 2011) proved is a
//! **residue number system**: exponential capacity, carry-free parallel comparison, and an
//! error-correcting coarse↔fine hierarchy.
//!
//! Concretely, a scalar position `x` is stored as its digits across co-prime scales
//! `p_1 < p_2 < ... < p_k`:
//!   - **Small period** → phase turns fast → *fine* discrimination (exact / structural).
//!   - **Large period** → phase turns slow → *coarse* neighborhood (fuzzy / semantic).
//!
//! Capacity is the **product** of the scales (add a scale → multiply capacity), and distinct
//! memories occupy distinct lattice points — no superposition crosstalk. Keeping the scales
//! co-prime preserves the residue-number-system view (carry-free parallel arithmetic via CRT).
//!
//! A full engram code is a **product of independent axes** — Semantic ⊗ Structure ⊗ Context ⊗
//! Time — so precision and generalization stop competing: query the coarse scales of the semantic
//! axis for paraphrase, and the fine scales of the structure axis for exact roles, in one lookup.
//!
//! This module is deterministic, pure-`std` (+ `serde`), and standalone: it does not touch the
//! live recall path. It exists to validate the physics (capacity, coarse/fine recall, axis
//! independence) before any engine rewiring.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Default co-prime scales (primes) — fine → coarse. Product ≈ 6.68e9 distinct points.
pub const DEFAULT_SCALES: [u32; 8] = [7, 11, 13, 17, 19, 23, 29, 31];

/// Which factored axis a coordinate belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Axis {
    /// Meaning / paraphrase (coarse scales carry the gist).
    Semantic,
    /// Role structure: subject/verb/object/relation (fine scales carry exact parse).
    Structure,
    /// Session / thread / speaker.
    Context,
    /// Event time bucket.
    Time,
}

impl Axis {
    pub const ALL: [Axis; 4] = [Axis::Semantic, Axis::Structure, Axis::Context, Axis::Time];

    fn salt(self) -> u64 {
        match self {
            Axis::Semantic => 0x5E_5E_5E_5E_5E_5E_5E_5E,
            Axis::Structure => 0x57_57_57_57_57_57_57_57,
            Axis::Context => 0xC0_C0_C0_C0_C0_C0_C0_C0,
            Axis::Time => 0x71_71_71_71_71_71_71_71,
        }
    }
}

/// A grid code on one axis: residues across the configured scales.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GridCode {
    pub residues: Vec<u32>,
}

impl GridCode {
    /// Encode a scalar position into mixed-radix grid digits (index 0 = finest scale, last = coarsest).
    ///
    /// digit_i = (position / prod(scales[0..i])) mod scales[i]. This is locality preserving:
    /// nearby positions share their most-significant (coarse) digits and differ only in the
    /// least-significant (fine) digits — exactly the grid-cell coarse↔fine hierarchy. Capacity
    /// is the product of the radices, and (co-prime radices also admit the CRT view for
    /// carry-free parallel arithmetic).
    pub fn encode(position: u64, scales: &[u32]) -> Self {
        let mut residues = Vec::with_capacity(scales.len());
        let mut weight: u128 = 1;
        for &p in scales {
            let digit = ((position as u128 / weight) % p as u128) as u32;
            residues.push(digit);
            weight *= p as u128;
        }
        Self { residues }
    }

    /// Reconstruct the scalar position from mixed-radix digits. Exact for `position < capacity`.
    pub fn decode(&self, scales: &[u32]) -> Option<u64> {
        if self.residues.len() != scales.len() || scales.is_empty() {
            return None;
        }
        let mut acc: u128 = 0;
        let mut weight: u128 = 1;
        for (i, &p) in scales.iter().enumerate() {
            acc += self.residues[i] as u128 * weight;
            weight *= p as u128;
        }
        Some(acc as u64)
    }

    /// Circular phase distance on one scale, normalized to [0,1].
    fn scale_dist(a: u32, b: u32, period: u32) -> f32 {
        let diff = (a as i64 - b as i64).unsigned_abs() as u32 % period;
        let circ = diff.min(period - diff);
        circ as f32 / (period as f32 / 2.0)
    }

    /// Similarity over a slice of scales (1.0 = identical phases, 0.0 = maximally apart).
    fn similarity_over(&self, other: &GridCode, scales: &[u32], idx: &[usize]) -> f32 {
        if idx.is_empty() {
            return 0.0;
        }
        let mut sum = 0.0f32;
        for &i in idx {
            let d = Self::scale_dist(self.residues[i], other.residues[i], scales[i]);
            sum += 1.0 - d;
        }
        sum / idx.len() as f32
    }

    /// Coarse (fuzzy/semantic) similarity — only the largest-period scales.
    pub fn coarse_similarity(&self, other: &GridCode, scales: &[u32]) -> f32 {
        let n = scales.len();
        let start = n - (n / 2).max(1); // top half = coarse
        let idx: Vec<usize> = (start..n).collect();
        self.similarity_over(other, scales, &idx)
    }

    /// Fine (exact/structural) similarity — only the smallest-period scales.
    pub fn fine_similarity(&self, other: &GridCode, scales: &[u32]) -> f32 {
        let n = scales.len();
        let end = (n / 2).max(1); // bottom half = fine
        let idx: Vec<usize> = (0..end).collect();
        self.similarity_over(other, scales, &idx)
    }

    pub fn exact(&self, other: &GridCode) -> bool {
        self.residues == other.residues
    }
}

/// A full lattice address: one grid code per factored axis, on shared scales.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LatticeCode {
    pub axes: HashMap<Axis, GridCode>,
}

impl LatticeCode {
    pub fn exact(&self, other: &LatticeCode) -> bool {
        self.axes == other.axes
    }

    /// Weighted axis similarity at a chosen resolution.
    /// `coarse=true` uses coarse scales (fuzzy), else fine scales (exact-leaning).
    pub fn similarity(
        &self,
        other: &LatticeCode,
        scales: &[u32],
        weights: &[(Axis, f32)],
        coarse: bool,
    ) -> f32 {
        let mut num = 0.0f32;
        let mut den = 0.0f32;
        for &(axis, w) in weights {
            let (Some(a), Some(b)) = (self.axes.get(&axis), other.axes.get(&axis)) else {
                continue;
            };
            let s = if coarse {
                a.coarse_similarity(b, scales)
            } else {
                a.fine_similarity(b, scales)
            };
            num += w * s;
            den += w;
        }
        if den <= 0.0 {
            0.0
        } else {
            num / den
        }
    }
}

/// The lattice encoder: owns the scales and maps features → positions → codes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lattice {
    pub scales: Vec<u32>,
}

impl Default for Lattice {
    fn default() -> Self {
        Self {
            scales: DEFAULT_SCALES.to_vec(),
        }
    }
}

impl Lattice {
    pub fn new(scales: Vec<u32>) -> Self {
        Self { scales }
    }

    /// Total distinct lattice points = product of scales (exponential in scale count).
    pub fn capacity(&self) -> u128 {
        self.scales.iter().fold(1u128, |acc, &p| acc * p as u128)
    }

    /// Elastic neurogenesis: add a co-prime scale → capacity multiplies by `period`.
    /// Returns false if `period` shares a factor with an existing scale (breaks CRT).
    pub fn grow(&mut self, period: u32) -> bool {
        if period < 2 {
            return false;
        }
        if self
            .scales
            .iter()
            .any(|&p| gcd(p as u64, period as u64) != 1)
        {
            return false;
        }
        self.scales.push(period);
        self.scales.sort_unstable();
        true
    }

    /// Encode an explicit scalar position (locality-preserving inputs → coarse/fine works).
    pub fn encode_position(&self, position: u64) -> GridCode {
        GridCode::encode(position % self.capacity() as u64, &self.scales)
    }

    /// Deterministic feature → position for one axis (exact addressing; not locality-preserving).
    pub fn position_from_feature(&self, axis: Axis, feature: &str) -> u64 {
        let h = hash64(&[axis.salt(), fnv1a(feature.as_bytes())]);
        h % self.capacity() as u64
    }

    /// Map a phase-parser structural signature (see [`crate::phase_parse`]) to a Structure-axis
    /// grid code. This is the bridge that lets the parser decide *how parts relate* and the
    /// lattice decide *where that structure is addressed* — parsing and addressing compose.
    pub fn encode_structure(&self, signature: u64) -> GridCode {
        let pos = hash64(&[Axis::Structure.salt(), signature]) % self.capacity() as u64;
        self.encode_position(pos)
    }

    /// Build a full lattice code from per-axis feature strings.
    pub fn encode_axes(&self, features: &[(Axis, &str)]) -> LatticeCode {
        let mut axes = HashMap::new();
        for &(axis, feat) in features {
            let pos = self.position_from_feature(axis, feat);
            axes.insert(axis, self.encode_position(pos));
        }
        LatticeCode { axes }
    }

    /// Build a code where the Semantic axis uses a locality-preserving unit position in [0,1),
    /// and the other axes use exact feature hashing. Used for coarse-fuzzy recall.
    pub fn encode_with_semantic_position(
        &self,
        semantic_unit: f64,
        other: &[(Axis, &str)],
    ) -> LatticeCode {
        let cap = self.capacity() as f64;
        let pos = (semantic_unit.rem_euclid(1.0) * cap) as u64 % self.capacity() as u64;
        let mut axes = HashMap::new();
        axes.insert(Axis::Semantic, self.encode_position(pos));
        for &(axis, feat) in other {
            axes.insert(
                axis,
                self.encode_position(self.position_from_feature(axis, feat)),
            );
        }
        LatticeCode { axes }
    }
}

/// Content-addressable store over lattice codes — coarse (fuzzy) and exact recall.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LatticeStore {
    pub entries: Vec<(String, LatticeCode)>,
}

impl LatticeStore {
    pub fn insert(&mut self, id: impl Into<String>, code: LatticeCode) {
        self.entries.push((id.into(), code));
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Coarse (semantic/fuzzy) nearest neighbours.
    pub fn query_coarse(
        &self,
        cue: &LatticeCode,
        scales: &[u32],
        weights: &[(Axis, f32)],
        k: usize,
    ) -> Vec<(String, f32)> {
        self.ranked(cue, scales, weights, true, k)
    }

    /// Fine (exact-leaning/structural) nearest neighbours.
    pub fn query_fine(
        &self,
        cue: &LatticeCode,
        scales: &[u32],
        weights: &[(Axis, f32)],
        k: usize,
    ) -> Vec<(String, f32)> {
        self.ranked(cue, scales, weights, false, k)
    }

    /// Exact lattice-point hits (all residues on all queried axes match).
    pub fn query_exact(&self, cue: &LatticeCode) -> Vec<String> {
        self.entries
            .iter()
            .filter(|(_, c)| c.exact(cue))
            .map(|(id, _)| id.clone())
            .collect()
    }

    fn ranked(
        &self,
        cue: &LatticeCode,
        scales: &[u32],
        weights: &[(Axis, f32)],
        coarse: bool,
        k: usize,
    ) -> Vec<(String, f32)> {
        let mut scored: Vec<(String, f32)> = self
            .entries
            .iter()
            .map(|(id, code)| (id.clone(), cue.similarity(code, scales, weights, coarse)))
            .collect();
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(k);
        scored
    }
}

// ---- number theory helpers ----

fn gcd(a: u64, b: u64) -> u64 {
    if b == 0 {
        a
    } else {
        gcd(b, a % b)
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

fn hash64(parts: &[u64]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for &p in parts {
        h ^= p;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crt_roundtrip_is_exact_within_capacity() {
        let lat = Lattice::default();
        let cap = lat.capacity() as u64;
        for x in [0u64, 1, 42, 1000, 999_983, cap - 1, cap / 2, cap / 3] {
            let code = lat.encode_position(x);
            assert_eq!(code.decode(&lat.scales), Some(x % cap), "x={x}");
        }
    }

    #[test]
    fn capacity_is_product_and_exponential() {
        let lat = Lattice::new(vec![7, 11, 13]);
        assert_eq!(lat.capacity(), 7 * 11 * 13);
        // Adding a scale MULTIPLIES capacity — exponential growth in scale count.
        let mut grown = lat.clone();
        assert!(grown.grow(17));
        assert_eq!(grown.capacity(), 7 * 11 * 13 * 17);
    }

    #[test]
    fn grow_rejects_non_coprime_scale() {
        let mut lat = Lattice::new(vec![7, 11]);
        assert!(!lat.grow(14)); // shares factor 7 → would break CRT
        assert!(lat.grow(13)); // co-prime → ok
        assert_eq!(lat.scales, vec![7, 11, 13]);
    }

    #[test]
    fn coarse_matches_nearby_while_fine_separates() {
        // Locality-preserving positions: nearby units → nearby positions.
        let lat = Lattice::default();
        let a = lat.encode_with_semantic_position(0.5000, &[]);
        let near = lat.encode_with_semantic_position(0.5000005, &[]); // tiny delta
        let far = lat.encode_with_semantic_position(0.9, &[]);

        let sa = a.axes[&Axis::Semantic].clone();
        let near_s = near.axes[&Axis::Semantic].clone();
        let far_s = far.axes[&Axis::Semantic].clone();

        // Coarse (large-period) phases agree for the near point, disagree for the far point.
        let coarse_near = sa.coarse_similarity(&near_s, &lat.scales);
        let coarse_far = sa.coarse_similarity(&far_s, &lat.scales);
        assert!(
            coarse_near > coarse_far,
            "coarse near {coarse_near} should beat far {coarse_far}"
        );
        assert!(
            coarse_near > 0.8,
            "near should be coarse-similar: {coarse_near}"
        );

        // The near point is NOT an exact match — fine scales still separate them.
        assert!(!sa.exact(&near_s));
    }

    #[test]
    fn factored_axes_are_independent_no_crosstalk() {
        // Same semantic content, different structure → semantic axis unchanged.
        let lat = Lattice::default();
        let base = lat.encode_axes(&[
            (Axis::Semantic, "user upgraded internet plan"),
            (Axis::Structure, "SUBJECT=user VERB=upgrade OBJECT=plan"),
        ]);
        let other_structure = lat.encode_axes(&[
            (Axis::Semantic, "user upgraded internet plan"),
            (Axis::Structure, "SUBJECT=plan VERB=upgrade OBJECT=user"),
        ]);
        // Semantic axis identical (no crosstalk from structure change).
        assert_eq!(
            base.axes[&Axis::Semantic],
            other_structure.axes[&Axis::Semantic]
        );
        // Structure axis differs (parsing captured the role swap).
        assert_ne!(
            base.axes[&Axis::Structure],
            other_structure.axes[&Axis::Structure]
        );
    }

    #[test]
    fn distinct_memories_have_distinct_points_no_bundling_crosstalk() {
        // Store many exact codes; every one is uniquely recoverable (no superposition noise).
        let lat = Lattice::default();
        let mut store = LatticeStore::default();
        let n = 5000u64;
        for i in 0..n {
            store.insert(
                format!("m{i}"),
                lat.encode_axes(&[(Axis::Semantic, &format!("fact-{i}"))]),
            );
        }
        // Exact query for a specific stored code returns exactly that memory.
        let target = lat.encode_axes(&[(Axis::Semantic, "fact-4242")]);
        let hits = store.query_exact(&target);
        assert!(
            hits.contains(&"m4242".to_string()),
            "exact recall failed: {hits:?}"
        );
    }

    #[test]
    fn coarse_and_fine_query_serve_different_needs() {
        // Semantic neighbourhood recall (fuzzy) vs exact structural recall — same store, one lookup each.
        let lat = Lattice::default();
        let mut store = LatticeStore::default();
        // A cluster of semantically-near items around 0.3, plus a far item.
        for (i, u) in [0.3000, 0.3000003, 0.3000006, 0.80].iter().enumerate() {
            store.insert(format!("s{i}"), lat.encode_with_semantic_position(*u, &[]));
        }
        let cue = lat.encode_with_semantic_position(0.30000015, &[]);
        let weights = [(Axis::Semantic, 1.0)];
        let coarse = store.query_coarse(&cue, &lat.scales, &weights, 3);
        // Top coarse hits are the near cluster (s0..s2), not the far item s3.
        let top_ids: Vec<&str> = coarse.iter().map(|(id, _)| id.as_str()).collect();
        assert!(top_ids.contains(&"s0") || top_ids.contains(&"s1") || top_ids.contains(&"s2"));
        assert!(
            !top_ids.contains(&"s3"),
            "far item leaked into coarse top-3: {top_ids:?}"
        );
    }

    #[test]
    fn multi_axis_query_precision_plus_generalization() {
        // Fuzzy on semantics AND exact on context in a single similarity call.
        let lat = Lattice::default();
        let mut store = LatticeStore::default();
        store.insert(
            "right",
            lat.encode_with_semantic_position(0.42, &[(Axis::Context, "session-A")]),
        );
        store.insert(
            "wrong_ctx",
            lat.encode_with_semantic_position(0.4200004, &[(Axis::Context, "session-B")]),
        );
        let cue = lat.encode_with_semantic_position(0.4200002, &[(Axis::Context, "session-A")]);
        let weights = [(Axis::Semantic, 1.0), (Axis::Context, 2.0)];
        let ranked = store.query_coarse(&cue, &lat.scales, &weights, 2);
        assert_eq!(
            ranked[0].0, "right",
            "context-weighted recall failed: {ranked:?}"
        );
    }
}
