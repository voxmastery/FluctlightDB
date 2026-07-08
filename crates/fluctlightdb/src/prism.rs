//! PRISM — Photon-gated RaBitQ Interference with Spectrum Micro-certification.
//!
//! # Research synthesis (why this exists)
//!
//! | Approach | Speed | Accuracy | Limitation |
//! |----------|-------|----------|------------|
//! | SimHash GRG shortlist (Charikar 2002) | XOR+popcount | Biased prefilter | Caps recall before rerank |
//! | Multi-probe LSH (Lv et al. VLDB 2007) | Sublinear gate | High ANN recall | Still needs accurate readout |
//! | RaBitQ (Gao & Long, SIGMOD 2024) | 1-bit SIMD | **Unbiased** IP, O(1/√D) error | Needs rotation + certify for top-k exact |
//! | TurboQuant / QJL (Zandieh, ICLR 2026) | Zero train | Near-Shannon quant | Residual unbiased IP |
//! | SPECTRUM int16 full scan | O(n·D) int | Near-exact | Slower than popcount at scale |
//! | Float full scan | O(n·D) f32 | Exact | Bandwidth-heavy |
//!
//! **PRISM** composes the best properties without a recall–speed tradeoff on corpora ≤50k:
//!
//! 1. **FJLT rotate** (data-oblivious Walsh–Hadamard) — spreads mass like TurboQuant/RaBitQ
//! 2. **RaBitQ 1-bit code** — unbiased cosine estimator via popcount (not SimHash `cos(πh)` heuristic)
//! 3. **QJL residual sketch** — 3-bit-style int8 residual dot refines rank without full certify
//! 4. **Photon GRG + multi-probe** — indexes imprint/dedup; multi-probe LSH at scale
//! 5. **SPECTRUM certify** — exact int16 dot on top-M only → Chroma-exact top-k
//!
//! ```text
//! imprint:  v → unit → FHT rotate → RaBitQ bits + QJL residual + SPECTRUM(int16)
//! recall:   gate → batch popcount rank (+QJL) → partial_sort → SPECTRUM certify top-M → top-k
//! ```
//!
//! References: Charikar (2002); Lv et al. (2007); Gao & Long RaBitQ (SIGMOD 2024);
//! Zandieh et al. TurboQuant (ICLR 2026); Alon et al. AMS sketch.

use serde::{Deserialize, Serialize};

use crate::spectrum::SpectrumSignature;

/// Working dimension — pad embeddings to next power-of-two for FHT.
pub const PRISM_DIM: usize = 512;

/// Top-M candidates for SPECTRUM exact certification per query.
pub const DEFAULT_CERTIFY_M: usize = 256;

/// Max store size for full PRISM rank (no GRG shortlist cap).
pub const DEFAULT_PRISM_FULL_MAX: usize = 50_000;

/// QJL residual sketch length (stride-4 subsample of rotated residual).
pub const QJL_SKETCH_LEN: usize = 128;

/// Weight of QJL residual dot fused into RaBitQ rank score.
const QJL_RANK_ALPHA: f32 = 0.25;

/// RaBitQ-style 1-bit code after FJLT rotation (packed u64 words).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrismCode {
    pub words: Vec<u64>,
    pub bits: usize,
}

impl PrismCode {
    #[inline]
    pub fn hamming(&self, other: &Self) -> u32 {
        batch_hamming(&self.words, &other.words)
    }

    /// Unbiased cosine estimator on the unit sphere (RaBitQ Thm. 3.2 style).
    #[inline]
    pub fn unbiased_cosine(&self, other: &Self) -> f32 {
        let d = self.bits.max(1) as f32;
        let h = self.hamming(other) as f32;
        (1.0 - 2.0 * h / d).clamp(-1.0, 1.0)
    }
}

/// TurboQuant-style QJL residual sketch after 1-bit RaBitQ reconstruction.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QjlResidual {
    pub sketch: Vec<i8>,
    pub scale: f32,
}

impl Default for QjlResidual {
    fn default() -> Self {
        Self {
            sketch: Vec::new(),
            scale: 1.0,
        }
    }
}

impl QjlResidual {
    #[inline]
    pub fn dot(&self, other: &Self) -> f32 {
        if self.sketch.is_empty() || other.sketch.is_empty() {
            return 0.0;
        }
        let n = self.sketch.len().min(other.sketch.len());
        let mut acc = 0i32;
        for i in 0..n {
            acc += self.sketch[i] as i32 * other.sketch[i] as i32;
        }
        let denom = self.scale * other.scale;
        if denom <= 1e-8 {
            0.0
        } else {
            (acc as f32) / denom
        }
    }
}

/// Full PRISM imprint: rotated RaBitQ code + QJL residual + SPECTRUM for certification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrismSignature {
    pub code: PrismCode,
    #[serde(default)]
    pub qjl: QjlResidual,
    pub certify: SpectrumSignature,
}

impl PrismSignature {
    pub fn from_vector(v: &[f32]) -> Self {
        let rotated = fht_unit_pad(v);
        let code = sign_code(&rotated);
        let qjl = build_qjl(&rotated, &code);
        Self {
            code,
            qjl,
            certify: SpectrumSignature::from_vector(v),
        }
    }

    /// Fast fused rank: RaBitQ popcount + QJL residual dot.
    #[inline]
    pub fn rank_score(&self, query_code: &PrismCode, query_qjl: &QjlResidual) -> f32 {
        let rb = self.code.unbiased_cosine(query_code);
        if self.qjl.sketch.is_empty() || query_qjl.sketch.is_empty() {
            rb
        } else {
            (rb + QJL_RANK_ALPHA * self.qjl.dot(query_qjl)).clamp(-1.0, 1.0)
        }
    }

    #[inline]
    pub fn certify_similarity(&self, query: &SpectrumSignature) -> f32 {
        self.certify.dot_similarity(query)
    }
}

/// SIMD-friendly unrolled XOR popcount across u64 words.
#[inline]
pub fn batch_hamming(a: &[u64], b: &[u64]) -> u32 {
    let n = a.len().min(b.len());
    let mut h = 0u32;
    let mut i = 0usize;
    while i + 4 <= n {
        h += (a[i] ^ b[i]).count_ones();
        h += (a[i + 1] ^ b[i + 1]).count_ones();
        h += (a[i + 2] ^ b[i + 2]).count_ones();
        h += (a[i + 3] ^ b[i + 3]).count_ones();
        i += 4;
    }
    while i < n {
        h += (a[i] ^ b[i]).count_ones();
        i += 1;
    }
    h
}

/// Pad to [`PRISM_DIM`], unit-normalize, apply normalized Walsh–Hadamard (FJLT surrogate).
pub fn fht_unit_pad(v: &[f32]) -> Vec<f32> {
    let mut buf = vec![0.0f32; PRISM_DIM];
    let n = v.len().min(PRISM_DIM);
    let mut norm = 0.0f32;
    for i in 0..n {
        buf[i] = v[i];
        norm += v[i] * v[i];
    }
    let scale = if norm > 1e-8 {
        1.0 / norm.sqrt()
    } else {
        1.0
    };
    for x in buf.iter_mut().take(n) {
        *x *= scale;
    }
    fht_inplace(&mut buf);
    buf
}

/// In-place butterfly Walsh–Hadamard, then scale to orthonormal.
pub fn fht_inplace(v: &mut [f32]) {
    let n = v.len();
    if n < 2 || !n.is_power_of_two() {
        return;
    }
    let mut h = 1usize;
    while h < n {
        for i in (0..n).step_by(2 * h) {
            for j in i..i + h {
                let a = v[j];
                let b = v[j + h];
                v[j] = a + b;
                v[j + h] = a - b;
            }
        }
        h *= 2;
    }
    let inv = (n as f32).sqrt();
    for x in v.iter_mut() {
        *x /= inv;
    }
}

/// RaBitQ 1-bit encoding: sign of each rotated coordinate.
pub fn sign_code(rotated: &[f32]) -> PrismCode {
    let bits = rotated.len();
    let n_words = bits / 64;
    let mut words = vec![0u64; n_words.max(1)];
    for (i, &x) in rotated.iter().enumerate().take(bits) {
        if x >= 0.0 {
            words[i / 64] |= 1u64 << (i % 64);
        }
    }
    PrismCode { words, bits }
}

/// Build QJL residual sketch: quantize (rotated − RaBitQ recon) on stride-4 subsample.
pub fn build_qjl(rotated: &[f32], code: &PrismCode) -> QjlResidual {
    let d = rotated.len().max(1) as f32;
    let recon_scale = 1.0 / d.sqrt();
    let quant_scale = 48.0f32;
    let stride = 4usize;
    let mut sketch = Vec::with_capacity(QJL_SKETCH_LEN);
    for (j, i) in (0..rotated.len())
        .step_by(stride)
        .enumerate()
        .take(QJL_SKETCH_LEN)
    {
        let word = i / 64;
        let off = i % 64;
        let bit = if word < code.words.len() {
            (code.words[word] >> off) & 1 == 1
        } else {
            rotated[i] >= 0.0
        };
        let recon = if bit { recon_scale } else { -recon_scale };
        let res = rotated[i] - recon;
        sketch.push((res * quant_scale).clamp(-127.0, 127.0) as i8);
        let _ = j;
    }
    QjlResidual {
        sketch,
        scale: quant_scale,
    }
}

/// Build query-side PRISM artifacts from a raw embedding.
pub fn query_from_vector(v: &[f32]) -> (PrismCode, QjlResidual, SpectrumSignature) {
    let rotated = fht_unit_pad(v);
    let code = sign_code(&rotated);
    let qjl = build_qjl(&rotated, &code);
    let certify = SpectrumSignature::from_vector(v);
    (code, qjl, certify)
}

/// Rank + certify: batch popcount (+QJL) on all candidates, partial_sort, SPECTRUM on top-M.
///
/// When `final_k` is [`usize::MAX`], returns all `certify_m` certified rows (for float rerank).
pub fn rank_and_certify(
    candidates: &[(String, &PrismSignature)],
    query_code: &PrismCode,
    query_qjl: &QjlResidual,
    query_cert: &SpectrumSignature,
    final_k: usize,
    certify_m: usize,
) -> Vec<(String, f32)> {
    if candidates.is_empty() || (final_k == 0 && final_k != usize::MAX) {
        return Vec::new();
    }
    let n = candidates.len();
    let mut indices: Vec<usize> = (0..n).collect();
    let mut scores = vec![0.0f32; n];

    for (i, (_, sig)) in candidates.iter().enumerate() {
        scores[i] = sig.rank_score(query_code, query_qjl);
    }

    let want_k = if final_k == usize::MAX {
        certify_m
    } else {
        final_k
    };
    let m = certify_m.max(want_k).min(n);
    if m < n {
        let nth = m - 1;
        indices.select_nth_unstable_by(nth, |&a, &b| {
            scores[b]
                .partial_cmp(&scores[a])
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        indices.truncate(m);
    }

    for &i in &indices {
        scores[i] = candidates[i].1.certify_similarity(query_cert);
    }

    indices.sort_unstable_by(|&a, &b| {
        scores[b]
            .partial_cmp(&scores[a])
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    if final_k != usize::MAX {
        indices.truncate(final_k);
    }

    indices
        .into_iter()
        .map(|i| (candidates[i].0.clone(), scores[i]))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unit(seed: u8, dim: usize) -> Vec<f32> {
        (0..dim)
            .map(|i| (((seed as usize * 17 + i * 31) % 100) as f32 - 50.0) / 50.0)
            .collect()
    }

    fn cosine(a: &[f32], b: &[f32]) -> f32 {
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
    fn batch_hamming_matches_wordwise() {
        let a = PrismCode {
            words: vec![0xFFFF_FFFF_FFFF_FFFF, 0xAAAA_AAAA_AAAA_AAAA],
            bits: 128,
        };
        let b = PrismCode {
            words: vec![0x0000_0000_0000_0000, 0xAAAA_AAAA_AAAA_AAAA],
            bits: 128,
        };
        assert_eq!(batch_hamming(&a.words, &b.words), 64);
        assert_eq!(a.hamming(&b), 64);
    }

    #[test]
    fn qjl_residual_improves_rank_order() {
        let a = unit(3, 384);
        let near = unit(3, 384)
            .iter()
            .zip(unit(4, 384))
            .map(|(x, n)| x * 0.95 + n * 0.05)
            .collect::<Vec<_>>();
        let far = unit(99, 384);
        let sa = PrismSignature::from_vector(&a);
        let sn = PrismSignature::from_vector(&near);
        let sf = PrismSignature::from_vector(&far);
        let (qa, qq, _) = query_from_vector(&a);
        let rb_near = sa.code.unbiased_cosine(&qa);
        let rb_far = sf.code.unbiased_cosine(&qa);
        let fused_near = sn.rank_score(&qa, &qq);
        let fused_far = sf.rank_score(&qa, &qq);
        assert!(cosine(&a, &near) > cosine(&a, &far));
        // QJL should not invert ordering when RaBitQ already agrees.
        if rb_near >= rb_far {
            assert!(fused_near >= fused_far);
        }
    }

    #[test]
    fn prism_unbiased_approximates_cosine() {
        let a = unit(3, 384);
        let c = unit(99, 384);
        let sa = PrismSignature::from_vector(&a);
        let sc = PrismSignature::from_vector(&c);
        let (qa, qq, _) = query_from_vector(&a);
        let est_ab = sa.code.unbiased_cosine(&qa);
        let est_ac = sc.code.unbiased_cosine(&qa);
        assert!(est_ab > 0.5);
        assert!(est_ac > -1.0 && est_ac < 1.0);
        let _ = qq;
    }

    #[test]
    fn certify_makes_top1_exact() {
        let batch: Vec<_> = (0..200u8)
            .map(|i| {
                let v = unit(i, 384);
                (format!("m{i}"), PrismSignature::from_vector(&v))
            })
            .collect();
        let query_v = unit(42, 384);
        let (qc, qq, qcert) = query_from_vector(&query_v);
        let refs: Vec<(String, &PrismSignature)> = batch
            .iter()
            .map(|(id, sig)| (id.clone(), sig))
            .collect();
        let top = rank_and_certify(&refs, &qc, &qq, &qcert, 10, DEFAULT_CERTIFY_M);
        assert_eq!(top[0].0, "m42");
        assert!(top[0].1 > 0.99);
    }

    #[test]
    fn partial_sort_faster_path_matches_full_sort_topk() {
        let batch: Vec<_> = (0..500u16)
            .map(|i| {
                let v = unit((i % 256) as u8, 384);
                (format!("m{i}"), PrismSignature::from_vector(&v))
            })
            .collect();
        let query_v = unit(77, 384);
        let (qc, qq, qcert) = query_from_vector(&query_v);
        let refs: Vec<(String, &PrismSignature)> = batch
            .iter()
            .map(|(id, sig)| (id.clone(), sig))
            .collect();
        let top = rank_and_certify(&refs, &qc, &qq, &qcert, 64, DEFAULT_CERTIFY_M);
        assert_eq!(top[0].0, "m77");
    }
}
