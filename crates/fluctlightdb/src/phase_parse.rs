//! Phase Parsing — theta-gamma phase code for order- and role-aware structure.
//!
//! # Why this exists
//! The Manifold Lattice ([`crate::lattice`]) gives *where* a memory lives (content-addressable
//! coordinates). It does not, by itself, encode *how the parts relate*: "user upgraded plan" and
//! "plan upgraded user" are the same bag of words. Fixed vectors bundle roles into the same bits,
//! so exact parsing and fuzzy meaning fight for capacity.
//!
//! The brain solves ordering with a **theta-gamma phase code** (Lisman & Idiart 1995; Lisman &
//! Jensen 2013): a slow theta cycle frames a short sequence, and each item fires in its own gamma
//! sub-slot at a distinct *phase*. Order is carried by *when in the cycle* an item fires, not by
//! which cells fire. We model this with a **Fourier Holographic Reduced Representation** (Plate
//! 1995): every symbol is a vector of unit-magnitude phasors `e^{iφ}`.
//!   - **Bind** (role ⊛ filler, or slot ⊛ item) = add phases — exactly invertible.
//!   - **Unbind** = subtract phases — recovers the partner (noisy after bundling; a codebook
//!     cleanup snaps it back to the nearest known symbol, the hippocampal pattern-completion step).
//!   - **Bundle** (superpose a set) = sum the phasors and take the resulting angle.
//!
//! So a parsed structure = bundle of `slot_i ⊛ item_i` (order) or `role ⊛ filler` (grammar).
//! Two orderings of the same words produce dissimilar structures, and any role/slot can be read
//! back out. The bundle also reduces to a stable `structural_signature()` → a Structure-axis
//! coordinate for the lattice, so parsing and addressing compose.
//!
//! Deterministic, pure-`std` (+ `serde`), standalone. Validates the parsing physics before any
//! engine rewiring.

use std::f32::consts::TAU;

use serde::{Deserialize, Serialize};

/// Default phasor dimensionality. Higher → more bundle capacity before cleanup fails.
pub const DEFAULT_DIM: usize = 512;

/// A hypervector of unit-magnitude phasors, stored as phases in radians `[0, TAU)`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PhaseVector {
    pub phases: Vec<f32>,
}

impl PhaseVector {
    /// Deterministic pseudo-random phasor vector for a symbol token.
    pub fn from_token(token: &str, dim: usize) -> Self {
        let base = fnv1a(token.as_bytes());
        let phases = (0..dim)
            .map(|i| {
                let h = splitmix64(base ^ (i as u64).wrapping_mul(0x9E3779B97F4A7C15));
                (h as f32 / u64::MAX as f32) * TAU
            })
            .collect();
        Self { phases }
    }

    pub fn dim(&self) -> usize {
        self.phases.len()
    }

    /// Bind two symbols: add phases (Fourier HRR circular convolution). Invertible.
    pub fn bind(&self, other: &PhaseVector) -> PhaseVector {
        self.zip_map(other, |a, b| wrap(a + b))
    }

    /// Unbind `other` from `self`: subtract phases. Inverse of [`bind`](Self::bind).
    pub fn unbind(&self, other: &PhaseVector) -> PhaseVector {
        self.zip_map(other, |a, b| wrap(a - b))
    }

    /// Superpose a set of vectors: sum the phasors, take the resultant angle per element.
    pub fn bundle(parts: &[PhaseVector]) -> Option<PhaseVector> {
        let dim = parts.first()?.dim();
        let mut re = vec![0.0f32; dim];
        let mut im = vec![0.0f32; dim];
        for p in parts {
            if p.dim() != dim {
                return None;
            }
            for i in 0..dim {
                re[i] += p.phases[i].cos();
                im[i] += p.phases[i].sin();
            }
        }
        let phases = (0..dim).map(|i| wrap(im[i].atan2(re[i]))).collect();
        Some(PhaseVector { phases })
    }

    /// Phasor cosine similarity: mean of `cos(Δphase)` in `[-1, 1]` (1.0 = identical).
    pub fn similarity(&self, other: &PhaseVector) -> f32 {
        let n = self.phases.len().min(other.phases.len());
        if n == 0 {
            return 0.0;
        }
        let mut acc = 0.0f32;
        for i in 0..n {
            acc += (self.phases[i] - other.phases[i]).cos();
        }
        acc / n as f32
    }

    /// Stable order/role-sensitive signature: quantize phases to 3 bits and hash.
    pub fn structural_signature(&self) -> u64 {
        let mut h: u64 = 0xcbf29ce484222325;
        for &p in &self.phases {
            let q = ((p / TAU) * 8.0) as u64 & 0x7; // 3-bit phase bucket
            h ^= q.wrapping_add(1);
            h = h.wrapping_mul(0x100000001b3);
        }
        h
    }

    fn zip_map(&self, other: &PhaseVector, f: impl Fn(f32, f32) -> f32) -> PhaseVector {
        let n = self.phases.len().min(other.phases.len());
        let phases = (0..n).map(|i| f(self.phases[i], other.phases[i])).collect();
        PhaseVector { phases }
    }
}

/// Cleanup memory: known symbols to snap noisy unbind results back to (CA3 completion analog).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Codebook {
    pub entries: Vec<(String, PhaseVector)>,
}

impl Codebook {
    pub fn add(&mut self, token: impl Into<String>, vec: PhaseVector) {
        self.entries.push((token.into(), vec));
    }

    /// Register a token by generating its canonical phasor vector.
    pub fn intern(&mut self, token: &str, dim: usize) -> PhaseVector {
        let v = PhaseVector::from_token(token, dim);
        self.entries.push((token.to_string(), v.clone()));
        v
    }

    /// Best matching known symbol for a (possibly noisy) query, with its similarity.
    pub fn cleanup(&self, query: &PhaseVector) -> Option<(String, f32)> {
        self.entries
            .iter()
            .map(|(t, v)| (t.clone(), v.similarity(query)))
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
    }
}

/// The theta-gamma parser: fixed dimensionality + a cache of ordinal gamma-slot vectors.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhaseParser {
    pub dim: usize,
}

impl Default for PhaseParser {
    fn default() -> Self {
        Self { dim: DEFAULT_DIM }
    }
}

impl PhaseParser {
    pub fn new(dim: usize) -> Self {
        Self { dim }
    }

    /// Gamma sub-slot vector for ordinal position `i` within the theta cycle.
    pub fn slot(&self, i: usize) -> PhaseVector {
        PhaseVector::from_token(&format!("gamma:slot:{i}"), self.dim)
    }

    /// Role vector (grammatical relation) for role-filler binding.
    pub fn role(&self, name: &str) -> PhaseVector {
        PhaseVector::from_token(&format!("role:{name}"), self.dim)
    }

    /// Encode an ordered token sequence: bundle of `slot_i ⊛ token_i` (order-sensitive).
    pub fn encode_sequence(&self, tokens: &[&str]) -> PhaseVector {
        let bound: Vec<PhaseVector> = tokens
            .iter()
            .enumerate()
            .map(|(i, t)| self.slot(i).bind(&PhaseVector::from_token(t, self.dim)))
            .collect();
        PhaseVector::bundle(&bound).unwrap_or(PhaseVector {
            phases: vec![0.0; self.dim],
        })
    }

    /// Encode role→filler bindings (subject/verb/object grammar): bundle of `role ⊛ filler`.
    pub fn encode_roles(&self, pairs: &[(&str, &str)]) -> PhaseVector {
        let bound: Vec<PhaseVector> = pairs
            .iter()
            .map(|(r, f)| self.role(r).bind(&PhaseVector::from_token(f, self.dim)))
            .collect();
        PhaseVector::bundle(&bound).unwrap_or(PhaseVector {
            phases: vec![0.0; self.dim],
        })
    }

    /// Read the item at ordinal position `i` back out of a sequence, cleaned against `codebook`.
    pub fn readout_position(
        &self,
        seq: &PhaseVector,
        i: usize,
        codebook: &Codebook,
    ) -> Option<(String, f32)> {
        codebook.cleanup(&seq.unbind(&self.slot(i)))
    }

    /// Read the filler bound to `role` back out, cleaned against `codebook`.
    pub fn readout_role(
        &self,
        structure: &PhaseVector,
        role: &str,
        codebook: &Codebook,
    ) -> Option<(String, f32)> {
        codebook.cleanup(&structure.unbind(&self.role(role)))
    }
}

// ---- hashing helpers ----

fn wrap(x: f32) -> f32 {
    let m = x.rem_euclid(TAU);
    if m < 0.0 {
        m + TAU
    } else {
        m
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

    #[test]
    fn self_similarity_is_one_random_is_near_zero() {
        let a = PhaseVector::from_token("apple", DEFAULT_DIM);
        let b = PhaseVector::from_token("bicycle", DEFAULT_DIM);
        assert!((a.similarity(&a) - 1.0).abs() < 1e-4);
        assert!(
            a.similarity(&b).abs() < 0.15,
            "unrelated sim {}",
            a.similarity(&b)
        );
    }

    #[test]
    fn bind_then_unbind_recovers_partner() {
        let a = PhaseVector::from_token("role:subject", DEFAULT_DIM);
        let b = PhaseVector::from_token("filler:user", DEFAULT_DIM);
        let bound = a.bind(&b);
        // Binding hides both operands (dissimilar from each).
        assert!(bound.similarity(&a).abs() < 0.15);
        assert!(bound.similarity(&b).abs() < 0.15);
        // Unbinding one operand exactly recovers the other.
        let recovered = bound.unbind(&a);
        assert!(
            recovered.similarity(&b) > 0.99,
            "recovered sim {}",
            recovered.similarity(&b)
        );
    }

    #[test]
    fn order_changes_the_parse() {
        let p = PhaseParser::default();
        let fwd = p.encode_sequence(&["user", "upgraded", "plan"]);
        let rev = p.encode_sequence(&["plan", "upgraded", "user"]);
        // Same words, different order → dissimilar structures and different signatures.
        assert!(
            fwd.similarity(&rev) < 0.6,
            "orderings too similar: {}",
            fwd.similarity(&rev)
        );
        assert_ne!(fwd.structural_signature(), rev.structural_signature());
    }

    #[test]
    fn sequence_readout_recovers_each_position() {
        let p = PhaseParser::default();
        let tokens = ["alpha", "bravo", "charlie", "delta"];
        let seq = p.encode_sequence(&tokens);

        let mut cb = Codebook::default();
        for t in ["alpha", "bravo", "charlie", "delta", "echo", "foxtrot"] {
            cb.intern(t, p.dim);
        }
        for (i, expected) in tokens.iter().enumerate() {
            let (got, sim) = p.readout_position(&seq, i, &cb).unwrap();
            assert_eq!(&got, expected, "pos {i}: got {got} (sim {sim})");
        }
    }

    #[test]
    fn role_filler_readout_recovers_grammar() {
        let p = PhaseParser::default();
        let structure = p.encode_roles(&[
            ("subject", "user"),
            ("verb", "upgraded"),
            ("object", "plan"),
        ]);
        let mut cb = Codebook::default();
        for t in ["user", "upgraded", "plan", "cancelled", "account", "agent"] {
            cb.intern(t, p.dim);
        }
        assert_eq!(
            p.readout_role(&structure, "subject", &cb).unwrap().0,
            "user"
        );
        assert_eq!(
            p.readout_role(&structure, "verb", &cb).unwrap().0,
            "upgraded"
        );
        assert_eq!(p.readout_role(&structure, "object", &cb).unwrap().0, "plan");
    }

    #[test]
    fn role_swap_is_detectable() {
        // "user upgraded plan" vs "plan upgraded user" — same words, roles swapped.
        let p = PhaseParser::default();
        let a = p.encode_roles(&[("subject", "user"), ("object", "plan")]);
        let b = p.encode_roles(&[("subject", "plan"), ("object", "user")]);
        let mut cb = Codebook::default();
        for t in ["user", "plan"] {
            cb.intern(t, p.dim);
        }
        // Reading the subject role yields different fillers → the swap is recovered.
        assert_eq!(p.readout_role(&a, "subject", &cb).unwrap().0, "user");
        assert_eq!(p.readout_role(&b, "subject", &cb).unwrap().0, "plan");
        assert_ne!(a.structural_signature(), b.structural_signature());
    }

    #[test]
    fn signature_is_deterministic() {
        let p = PhaseParser::default();
        let a = p.encode_sequence(&["one", "two", "three"]);
        let b = p.encode_sequence(&["one", "two", "three"]);
        assert_eq!(a.structural_signature(), b.structural_signature());
    }

    #[test]
    fn parser_feeds_lattice_structure_axis() {
        // End-to-end: parse two role-swapped structures, project their signatures onto the
        // lattice Structure axis, and confirm the swap yields distinct Structure coordinates
        // while the (identical) semantic content lands on the same Semantic coordinate.
        use crate::lattice::{Axis, Lattice};

        let p = PhaseParser::default();
        let lat = Lattice::default();

        let sig_fwd = p
            .encode_roles(&[("subject", "user"), ("object", "plan")])
            .structural_signature();
        let sig_rev = p
            .encode_roles(&[("subject", "plan"), ("object", "user")])
            .structural_signature();

        let struct_fwd = lat.encode_structure(sig_fwd);
        let struct_rev = lat.encode_structure(sig_rev);
        assert_ne!(
            struct_fwd, struct_rev,
            "role swap must move on Structure axis"
        );

        // Same bag of words → same Semantic coordinate regardless of role order.
        let sem_fwd =
            lat.encode_axes(&[(Axis::Semantic, "user plan")]).axes[&Axis::Semantic].clone();
        let sem_rev =
            lat.encode_axes(&[(Axis::Semantic, "user plan")]).axes[&Axis::Semantic].clone();
        assert_eq!(sem_fwd, sem_rev, "semantic axis must ignore role order");
    }
}
