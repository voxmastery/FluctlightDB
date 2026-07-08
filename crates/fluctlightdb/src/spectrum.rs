//! SPECTRUM — Signed Phase-Encoded Cosine Tensor for Unified Readout Matching.
//!
//! # Why this exists
//! Photon GRG gates candidates at XOR+popcount speed, but handing survivors to float
//! cosine rerank created a structural tradeoff: a capped Hamming shortlist could drop
//! true cosine neighbors before analog readout ran.
//!
//! **SPECTRUM** is the γ-band readout that pairs with GRG without that tradeoff:
//! each imprint stores a **signed int16 quantization** of its unit embedding. Recall
//! scores every gated trace with int16 dot product — cosine-equivalent ranking at
//! half the bandwidth of float32, without GRG shortlist recall loss on corpora
//! ≤ [`DEFAULT_FULL_READOUT_MAX`].
//!
//! Pipeline:
//! ```text
//! imprint:  vector → Photon bitcode (GRG address) + SPECTRUM int16 (readout)
//! recall:   GRG gate → SPECTRUM dot on all gated traces → top-k
//! ```

use serde::{Deserialize, Serialize};

/// Max traces for full SPECTRUM readout (no GRG shortlist cap).
pub const DEFAULT_FULL_READOUT_MAX: usize = 50_000;

const QUANT_MAX: f32 = 32767.0;

/// Quantized unit-vector signature for fast exact-rank readout.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpectrumSignature {
    pub dim: u16,
    pub bytes: Vec<i16>,
}

impl SpectrumSignature {
    /// Build from a (possibly unnormalized) embedding; stores unit direction.
    pub fn from_vector(v: &[f32]) -> Self {
        if v.is_empty() {
            return Self {
                dim: 0,
                bytes: Vec::new(),
            };
        }
        let mut norm = 0.0f32;
        for &x in v {
            norm += x * x;
        }
        let n = norm.sqrt();
        let bytes: Vec<i16> = if n > 1e-8 {
            v.iter()
                .map(|&x| {
                    let u = x / n;
                    (u * QUANT_MAX).round().clamp(-32767.0, 32767.0) as i32 as i16
                })
                .collect()
        } else {
            vec![0i16; v.len()]
        };
        Self {
            dim: bytes.len() as u16,
            bytes,
        }
    }

    /// Cosine similarity between underlying unit vectors (exact within quant error).
    #[inline]
    pub fn dot_similarity(&self, other: &Self) -> f32 {
        let n = self.bytes.len().min(other.bytes.len());
        if n == 0 {
            return 0.0;
        }
        let mut acc = 0i64;
        for i in 0..n {
            acc += (self.bytes[i] as i64) * (other.bytes[i] as i64);
        }
        let denom = QUANT_MAX * QUANT_MAX;
        (acc as f32 / denom).clamp(-1.0, 1.0)
    }

    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unit(seed: u8, dim: usize) -> Vec<f32> {
        (0..dim)
            .map(|i| (((seed as usize * 17 + i * 31) % 100) as f32 - 50.0) / 50.0)
            .collect()
    }

    fn cosine_f32(a: &[f32], b: &[f32]) -> f32 {
        let n = a.len().min(b.len());
        let mut dot = 0.0f32;
        let mut na = 0.0f32;
        let mut nb = 0.0f32;
        for i in 0..n {
            dot += a[i] * b[i];
            na += a[i] * a[i];
            nb += b[i] * b[i];
        }
        let d = (na * nb).sqrt();
        if d <= 1e-8 {
            0.0
        } else {
            (dot / d).clamp(-1.0, 1.0)
        }
    }

    #[test]
    fn spectrum_tracks_float_cosine() {
        let a = unit(3, 384);
        let b = unit(3, 384);
        let c = unit(99, 384);
        let sa = SpectrumSignature::from_vector(&a);
        let sb = SpectrumSignature::from_vector(&b);
        let sc = SpectrumSignature::from_vector(&c);
        let est_ab = sa.dot_similarity(&sb);
        let est_ac = sa.dot_similarity(&sc);
        let true_ab = cosine_f32(&a, &b);
        let true_ac = cosine_f32(&a, &c);
        assert!((est_ab - true_ab).abs() < 0.002, "ab err {}", (est_ab - true_ab).abs());
        assert!((est_ac - true_ac).abs() < 0.002, "ac err {}", (est_ac - true_ac).abs());
        assert!(est_ab > est_ac + 0.1);
    }

    #[test]
    fn spectrum_self_is_one() {
        let v = unit(7, 128);
        let s = SpectrumSignature::from_vector(&v);
        let sim = s.dot_similarity(&s);
        assert!(sim > 0.999, "self sim {sim}");
    }
}
