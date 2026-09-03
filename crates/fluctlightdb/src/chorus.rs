//! CHORUS — Coherent Hippocampal Oscillation Unified Substrate.
//!
//! Theoretical memory as phase interference on a θ–γ lattice:
//! - **Imprint** = wavelet injection (sub-ms per item)
//! - **Recall** = GRG (γ Resonance Gate): photon Hamming gate → **SPECTRUM** int8 readout
//! - **π-inhibition** = anti-phase cancellation on overlap
//! - **Eigenmode split** = degeneracy breaking (bit-separation)
//! - **Sleep** = θ-sweep collapse into hippocampal engrams

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::late_interaction::{
    evidence_fuse, maxsim_weighted, pack_tokens, salience_weights, tokenize, Bm25Index,
};
use crate::muon::count_sketch;
use crate::photon::{PhotonCode, PhotonStore, SimHasher, DEFAULT_BITS};
use crate::prism::{self, PrismSignature, DEFAULT_CERTIFY_M, DEFAULT_PRISM_FULL_MAX};
use crate::spectrum::{SpectrumSignature, DEFAULT_FULL_READOUT_MAX};

pub const THETA_BINS: u8 = 8;
pub const GAMMA_SLOTS: u8 = 32;
pub const PHI_BANDS: u16 = 256;
pub const TAU_RINGS: u8 = 16;
pub const WAVELET_TAPS: usize = 16;

/// Fixed-point complex cell (Q15-ish stored as f32 for product v1).
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
pub struct Complex {
    pub re: f32,
    pub im: f32,
}

impl Complex {
    pub fn from_polar(amp: f32, phase: f32) -> Self {
        Self {
            re: amp * phase.cos(),
            im: amp * phase.sin(),
        }
    }

    pub fn norm_sq(self) -> f32 {
        self.re * self.re + self.im * self.im
    }

    pub fn add_assign(&mut self, other: Self) {
        self.re += other.re;
        self.im += other.im;
    }
}

/// Soliton packet on the dendrite bus (sparse edge transport).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SolitonPacket {
    pub from_addr: u32,
    pub to_addr: u32,
    pub amplitude: f32,
    pub phase_delay: f32,
}

/// Holonomic provenance sheath (EPS) — fixed metadata per imprint.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProvenanceSheath {
    pub agent_id: Option<String>,
    pub verified: bool,
    pub provenance_kind: u8,
    pub source_uri: Option<String>,
}

/// One memory trace threaded through the phase field.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChorusTrace {
    pub memory_id: String,
    pub content: String,
    pub context: String,
    pub code: PhotonCode,
    pub vector: Option<Vec<f32>>,
    /// SPECTRUM int16 readout — pairs with GRG for cosine-equivalent rank at int16 bandwidth.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spectrum: Option<SpectrumSignature>,
    /// PRISM RaBitQ+FHT rank + SPECTRUM micro-certify (highest accuracy + speed).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prism: Option<PrismSignature>,
    pub theta: u8,
    pub gamma: u8,
    pub phi: u8,
    pub tau: u8,
    pub amplitude: f32,
    pub replay_tag: u32,
    pub salience: f32,
    pub sheath: ProvenanceSheath,
    pub split_generation: u8,
    /// MiniLM per-token vectors (f16 bits, L2-normalized, capped) for MaxSim
    /// late interaction. Empty = trace predates late interaction; recall falls
    /// back to the pooled photon/spectrum path.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub token_vectors: Vec<Vec<u16>>,
}

/// Scored resonance hit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChorusHit {
    pub memory_id: String,
    pub score: f32,
    #[serde(default)]
    pub photon: f32,
    #[serde(default)]
    pub field: f32,
    #[serde(default)]
    pub lexical: f32,
    #[serde(default)]
    pub theta: u8,
    #[serde(default)]
    pub lane: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub snippet: String,
}

/// Hot-path recall knobs (IR / agent tool calls).
#[derive(Debug, Clone, Copy)]
pub struct ChorusRecallOpts {
    /// Skip field/lexical/snippet work; PRISM/SPECTRUM vector readout on GRG gate.
    pub fast: bool,
    /// Final float32 cosine rerank on certified pool (production gold standard).
    pub float_rerank: bool,
}

impl Default for ChorusRecallOpts {
    fn default() -> Self {
        Self {
            fast: false,
            float_rerank: true,
        }
    }
}

impl ChorusRecallOpts {
    /// IR path: PRISM/SPECTRUM readout on all GRG-gated traces.
    pub fn ir_vector() -> Self {
        Self {
            fast: true,
            float_rerank: true,
        }
    }

    /// Production agent path: fast vector readout + float rerank safety net.
    pub fn production() -> Self {
        Self {
            fast: true,
            float_rerank: true,
        }
    }
}

/// Bulk imprint row.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChorusImprintInput {
    pub memory_id: String,
    pub content: String,
    #[serde(default)]
    pub context: String,
    #[serde(default)]
    pub semantic_vector: Option<Vec<f32>>,
    /// MiniLM per-token vectors (f32, L2-normalized) for MaxSim late interaction.
    /// The pooled `semantic_vector` is still used for the photon prefilter.
    #[serde(default)]
    pub token_vectors: Option<Vec<Vec<f32>>>,
    #[serde(default)]
    pub salience: f32,
    #[serde(default)]
    pub sheath: ProvenanceSheath,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChorusConfig {
    pub photon_bits: usize,
    pub dedup_hamming: u32,
    pub split_threshold: f32,
    pub collapse_threshold: f32,
    pub inhibit_strength: f32,
    pub max_splits_per_imprint: u8,
    /// C-7 GRG: γ-band gate — max parent traces passed to analog cosine rerank.
    pub grg_shortlist_k: usize,
    /// Exact Hamming scan (XOR+popcount, no floats) up to this store size.
    pub grg_exact_hamming_max: usize,
    pub grg_lsh_bands: usize,
    pub grg_lsh_rows: usize,
    /// Max embedding width for cached GRG hyperplanes (QBP).
    pub grg_max_dim: usize,
    /// Full SPECTRUM readout on all traces up to this store size (no GRG shortlist cap).
    pub spectrum_full_readout_max: usize,
    /// PRISM certify top-M after RaBitQ rank (0 = disable PRISM path).
    pub prism_certify_m: usize,
    /// Full PRISM rank (no shortlist) up to this store size.
    pub prism_full_readout_max: usize,
    /// Multi-probe LSH: flip this many low bits per band key (Lv et al. 2007).
    pub grg_multi_probe_bits: usize,
    /// IVF-lite: neighbor-bit probes on coarse band-0 cell at scale.
    pub grg_ivf_neighbor_bits: usize,
    /// Float32 cosine rerank on PRISM certify pool (closes int16 quant gap).
    pub prism_float_rerank: bool,
    /// PRISM fast path only when k ≤ this; higher k uses full SPECTRUM readout (e.g. LoCoMo k=150).
    pub prism_max_k: usize,
}

impl Default for ChorusConfig {
    fn default() -> Self {
        Self {
            photon_bits: DEFAULT_BITS,
            dedup_hamming: 12,
            split_threshold: 4.0,
            collapse_threshold: 1.5,
            inhibit_strength: 0.35,
            max_splits_per_imprint: 3,
            grg_shortlist_k: 2048,
            grg_exact_hamming_max: 50_000,
            grg_lsh_bands: 32,
            grg_lsh_rows: 8,
            grg_max_dim: 512,
            spectrum_full_readout_max: DEFAULT_FULL_READOUT_MAX,
            prism_certify_m: DEFAULT_CERTIFY_M,
            prism_full_readout_max: DEFAULT_PRISM_FULL_MAX,
            grg_multi_probe_bits: 6,
            grg_ivf_neighbor_bits: 4,
            prism_float_rerank: true,
            prism_max_k: 100,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ChorusSleepReport {
    pub theta_sweeps: u8,
    pub collapsed: u32,
    pub pruned: u32,
    pub splits: u32,
}

/// CHORUS phase field + trace registry.
#[derive(Debug, Serialize, Deserialize)]
pub struct ChorusField {
    pub theta_clock: u8,
    pub tick: u64,
    pub config: ChorusConfig,
    cells: HashMap<u32, Complex>,
    traces: HashMap<String, ChorusTrace>,
    trace_order: Vec<String>,
    photon: PhotonStore,
    hasher: SimHasher,
    /// QBP: cached ±1 hyperplanes per embedding width (avoids per-query hash storms).
    #[serde(skip, default = "empty_plane_cache")]
    plane_cache: Mutex<HashMap<usize, Vec<Vec<f32>>>>,
    dendrite: Vec<SolitonPacket>,
    splits_total: u32,
    /// BM25 lexical channel over trace content, fused with MaxSim in recall.
    #[serde(default)]
    bm25: Bm25Index,
}

fn empty_plane_cache() -> Mutex<HashMap<usize, Vec<Vec<f32>>>> {
    Mutex::new(HashMap::new())
}

impl Clone for ChorusField {
    fn clone(&self) -> Self {
        Self {
            theta_clock: self.theta_clock,
            tick: self.tick,
            config: self.config.clone(),
            cells: self.cells.clone(),
            traces: self.traces.clone(),
            trace_order: self.trace_order.clone(),
            photon: self.photon.clone(),
            hasher: self.hasher.clone(),
            plane_cache: Mutex::new(HashMap::new()),
            dendrite: self.dendrite.clone(),
            splits_total: self.splits_total,
            bm25: self.bm25.clone(),
        }
    }
}

impl Default for ChorusField {
    fn default() -> Self {
        Self::new(ChorusConfig::default())
    }
}

impl ChorusField {
    pub fn new(config: ChorusConfig) -> Self {
        let bits = config.photon_bits;
        Self {
            theta_clock: 0,
            tick: 0,
            hasher: SimHasher::new(bits, 0x0C0D_E501_u64),
            photon: PhotonStore::new(bits, config.grg_lsh_bands, config.grg_lsh_rows),
            config,
            plane_cache: Mutex::new(HashMap::new()),
            cells: HashMap::new(),
            traces: HashMap::new(),
            trace_order: Vec::new(),
            dendrite: Vec::new(),
            splits_total: 0,
            bm25: Bm25Index::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.traces.len()
    }

    pub fn is_empty(&self) -> bool {
        self.traces.is_empty()
    }

    pub fn cell_count(&self) -> usize {
        self.cells.len()
    }

    /// Advance θ-bus (real-time theta as product clock).
    pub fn tick_theta(&mut self) -> u8 {
        self.tick = self.tick.wrapping_add(1);
        self.theta_clock = (self.theta_clock + 1) % THETA_BINS;
        self.theta_clock
    }

    fn pack_addr(theta: u8, gamma: u8, phi: u8, tau: u8) -> u32 {
        ((theta as u32) << 17) | ((gamma as u32) << 12) | ((phi as u32) << 4) | (tau as u32)
    }

    fn address_for(
        &self,
        code: &PhotonCode,
        salience: f32,
        sheath: &ProvenanceSheath,
    ) -> (u8, u8, u8, u8) {
        let theta = self.theta_clock;
        let gamma = (code.words[0] % GAMMA_SLOTS as u64) as u8;
        let phi_base = (code.words[0] ^ code.words.get(1).copied().unwrap_or(0)) as u8;
        let phi = phi_base.wrapping_add((sheath.provenance_kind.wrapping_mul(17)) % 32);
        let tau = ((self.tick as f32).ln_1p() as u8 + (salience * 4.0) as u8).min(TAU_RINGS - 1);
        (theta, gamma, phi, tau)
    }

    fn wavelet_taps(content: &str, context: &str, dim: usize) -> Vec<f32> {
        let sketch = count_sketch(content, context, dim.max(32));
        let mut taps: Vec<f32> = sketch.into_iter().take(WAVELET_TAPS).collect();
        while taps.len() < WAVELET_TAPS {
            taps.push(0.0);
        }
        taps
    }

    #[allow(clippy::too_many_arguments)]
    fn inject_wavelet(
        &mut self,
        theta: u8,
        gamma: u8,
        phi: u8,
        tau: u8,
        salience: f32,
        taps: &[f32],
        phase_offset: f32,
    ) {
        let base_amp = salience.max(0.05);
        for (i, &tap) in taps.iter().enumerate() {
            if tap.abs() < 1e-6 {
                continue;
            }
            let phi_i = phi.wrapping_add((i as u8) % 8);
            let addr = Self::pack_addr(theta, gamma, phi_i, tau);
            let phase = phase_offset + tap.atan2(1.0);
            let cell = self.cells.entry(addr).or_default();
            cell.add_assign(Complex::from_polar(base_amp * tap.abs(), phase));
        }
    }

    fn apply_pi_inhibition(&mut self, theta: u8, gamma: u8, phi: u8, tau: u8, strength: f32) {
        for dphi in [-2i8, -1, 1, 2] {
            let phi_n = (phi as i16 + dphi as i16).clamp(0, 255) as u8;
            let addr = Self::pack_addr(theta, gamma, phi_n, tau);
            if let Some(exc) = self.cells.get(&addr).copied() {
                let inhib = Complex {
                    re: -exc.re * strength,
                    im: -exc.im * strength,
                };
                let inhib_addr = Self::pack_addr(theta, gamma, phi_n, tau.wrapping_add(1));
                self.cells.entry(inhib_addr).or_default().add_assign(inhib);
            }
        }
    }

    fn maybe_split(&mut self, trace: &ChorusTrace) -> Option<ChorusTrace> {
        let addr = Self::pack_addr(trace.theta, trace.gamma, trace.phi, trace.tau);
        let norm = self.cells.get(&addr).copied().unwrap_or_default().norm_sq();
        if norm < self.config.split_threshold
            || trace.split_generation >= self.config.max_splits_per_imprint
        {
            return None;
        }
        self.splits_total += 1;
        let sep_id = format!("{}#s{}", trace.memory_id, trace.split_generation + 1);
        let mut sep_code = trace.code.clone();
        if !sep_code.words.is_empty() {
            sep_code.words[0] ^= hash_str(&sep_id);
        }
        let (theta, gamma, mut phi, tau) = (
            trace.theta,
            trace.gamma,
            trace.phi.wrapping_add(8),
            trace.tau,
        );
        phi = phi.wrapping_add(trace.split_generation.wrapping_add(1) * 3);
        let mut split = trace.clone();
        split.memory_id = sep_id;
        split.code = sep_code;
        split.phi = phi;
        split.split_generation = trace.split_generation + 1;
        split.amplitude *= 0.5;
        let taps = Self::wavelet_taps(&split.content, &split.context, 32);
        let phase = provenance_phase(&split.sheath);
        self.inject_wavelet(theta, gamma, phi, tau, split.salience, &taps, phase);
        Some(split)
    }

    /// C-2 wavelet imprint + C-3 DBSG dedup + C-4 π-inhibition + C-5 eigenmode split.
    pub fn imprint(&mut self, input: &ChorusImprintInput) -> bool {
        let salience = if input.salience > 0.0 {
            input.salience
        } else {
            0.55
        };
        let code = if let Some(ref v) = input.semantic_vector {
            if !v.is_empty() {
                self.encode_vector(v)
            } else {
                self.hasher
                    .encode(&Self::wavelet_taps(&input.content, &input.context, 64))
            }
        } else {
            self.hasher
                .encode(&Self::wavelet_taps(&input.content, &input.context, 64))
        };

        // Pooled-photon dedup collapses traces whose mean-pooled centroids collide —
        // but token-level (MaxSim) traces stay distinct even at close centroids, so
        // skip the dedup gate when per-token vectors are provided.
        let has_tokens = input.token_vectors.as_ref().is_some_and(|t| !t.is_empty());
        if !has_tokens {
            if let Some((_, nearest)) = self.photon.nearest(&code) {
                if nearest.hamming(&code) < self.config.dedup_hamming {
                    return false;
                }
            }
        }

        let (theta, gamma, phi, tau) = self.address_for(&code, salience, &input.sheath);
        let taps = Self::wavelet_taps(&input.content, &input.context, 64);
        let phase = provenance_phase(&input.sheath);
        self.inject_wavelet(theta, gamma, phi, tau, salience, &taps, phase);
        self.apply_pi_inhibition(theta, gamma, phi, tau, self.config.inhibit_strength);

        let norm_vec = input
            .semantic_vector
            .as_ref()
            .filter(|v| !v.is_empty())
            .map(|v| normalize_vector(v));
        let spectrum = norm_vec.as_ref().map(|v| SpectrumSignature::from_vector(v));
        let prism = norm_vec.as_ref().map(|v| PrismSignature::from_vector(v));

        let trace = ChorusTrace {
            memory_id: input.memory_id.clone(),
            content: input.content.clone(),
            context: if input.context.is_empty() {
                input.memory_id.clone()
            } else {
                input.context.clone()
            },
            code: code.clone(),
            vector: norm_vec,
            spectrum,
            prism,
            theta,
            gamma,
            phi,
            tau,
            amplitude: salience,
            replay_tag: 0,
            salience,
            sheath: input.sheath.clone(),
            split_generation: 0,
            token_vectors: input
                .token_vectors
                .as_ref()
                .filter(|t| !t.is_empty())
                .map(|t| pack_tokens(t))
                .unwrap_or_default(),
        };

        if let Some(split) = self.maybe_split(&trace) {
            let split_id = split.memory_id.clone();
            self.traces.insert(split_id.clone(), split);
            self.trace_order.push(split_id);
        }

        self.bm25.add(&input.memory_id, &input.content);
        self.photon.insert(input.memory_id.clone(), code);
        self.traces.insert(input.memory_id.clone(), trace);
        self.trace_order.push(input.memory_id.clone());
        true
    }

    pub fn imprint_batch(&mut self, batch: &[ChorusImprintInput]) -> usize {
        batch.iter().filter(|row| self.imprint(row)).count()
    }

    fn field_coherence(&self, trace: &ChorusTrace, query_theta: u8) -> f32 {
        let mut sum = Complex::default();
        let dtheta = circular_delta(trace.theta, query_theta);
        let coherence_gate = 1.0 - (dtheta as f32 / THETA_BINS as f32);
        for i in 0..4 {
            let phi_i = trace.phi.wrapping_add(i);
            let addr = Self::pack_addr(trace.theta, trace.gamma, phi_i, trace.tau);
            if let Some(c) = self.cells.get(&addr) {
                sum.add_assign(*c);
            }
        }
        let raw = sum.norm_sq().sqrt() * coherence_gate.max(0.0);
        (raw / (1.0 + raw)).min(1.0)
    }

    fn lexical_overlap(cue: &str, content: &str) -> f32 {
        let cue_l = cue.to_lowercase();
        let words: Vec<&str> = cue_l.split_whitespace().filter(|w| w.len() > 2).collect();
        if words.is_empty() {
            return 0.0;
        }
        let body = content.to_lowercase();
        let hits = words.iter().filter(|w| body.contains(*w)).count() as f32;
        hits / words.len() as f32
    }

    fn encode_vector(&self, vector: &[f32]) -> PhotonCode {
        if vector.is_empty() {
            return self.hasher.encode(vector);
        }
        let dim = vector.len().min(self.config.grg_max_dim);
        let bits = self.config.photon_bits;
        let mut cache = self.plane_cache.lock().unwrap_or_else(|e| e.into_inner());
        let planes = cache.entry(dim).or_insert_with(|| {
            let seed = self.hasher.seed;
            (0..bits)
                .map(|b| {
                    (0..dim)
                        .map(|i| {
                            let h = splitmix64_plane(hash64_parts(&[seed, b as u64, i as u64]));
                            if h & 1 == 0 {
                                1.0f32
                            } else {
                                -1.0f32
                            }
                        })
                        .collect()
                })
                .collect()
        });
        let n_words = bits / 64;
        let mut words = vec![0u64; n_words];
        for b in 0..bits {
            let mut acc = 0.0f32;
            let row = &planes[b];
            for (i, &v) in vector.iter().take(dim).enumerate() {
                acc += row[i] * v;
            }
            if acc >= 0.0 {
                words[b / 64] |= 1u64 << (b % 64);
            }
        }
        PhotonCode { words, bits }
    }

    /// C-7 GRG — γ Resonance Gate: binary Hamming coincidence opens analog cosine lane.
    ///
    /// Hippocampal γ oscillations gate which spikes reach readout; here photon XOR+popcount
    /// gates which traces receive expensive float cosine. Sub-ms at ≤50k traces.
    fn grg_shortlist(&self, cue: &PhotonCode, k: usize) -> Vec<String> {
        let k = k.max(64).min(self.photon.len().max(1));
        let mut entries = if self.photon.len() <= self.config.grg_exact_hamming_max {
            self.photon.query_exact(cue, k)
        } else {
            let cap = k.saturating_mul(4);
            let probe_bits = self.config.grg_multi_probe_bits;
            let mut cand_idx =
                self.photon
                    .ivf_coarse_candidates(cue, cap, self.config.grg_ivf_neighbor_bits);
            if cand_idx.len() < k {
                cand_idx = self.photon.multi_probe_candidates(cue, cap, probe_bits);
            }
            if cand_idx.len() < k / 2 {
                let exact = self.photon.query_exact(cue, k);
                let mut scored: Vec<(String, u32)> = cand_idx
                    .into_iter()
                    .filter_map(|i| {
                        self.photon
                            .entry_at(i)
                            .map(|(id, code)| (id.to_string(), cue.hamming(code)))
                    })
                    .collect();
                for (id, ham) in exact {
                    if !scored.iter().any(|(eid, _)| eid == &id) {
                        scored.push((id, ham));
                    }
                }
                scored.sort_by_key(|(_, h)| *h);
                scored.truncate(k.saturating_mul(2));
                scored
            } else {
                let mut scored: Vec<(String, u32)> = cand_idx
                    .into_iter()
                    .filter_map(|i| {
                        self.photon
                            .entry_at(i)
                            .map(|(id, code)| (id.to_string(), cue.hamming(code)))
                    })
                    .collect();
                scored.sort_by_key(|(_, h)| *h);
                scored.truncate(k.saturating_mul(2));
                scored
            }
        };
        if entries.len() > k.saturating_mul(2) {
            entries.sort_by_key(|(_, ham)| *ham);
            entries.truncate(k.saturating_mul(2));
        }
        let mut best: HashMap<String, u32> = HashMap::new();
        for (id, ham) in entries {
            let parent = parent_memory_id(&id).to_string();
            best.entry(parent)
                .and_modify(|h| *h = (*h).min(ham))
                .or_insert(ham);
        }
        let mut ranked: Vec<(String, u32)> = best.into_iter().collect();
        ranked.sort_by_key(|(_, ham)| *ham);
        ranked.truncate(k);
        ranked.into_iter().map(|(id, _)| id).collect()
    }

    /// C-6 resonance recall — GRG shortlist + vector cosine rerank + field tie-break.
    pub fn recall(&self, cue: &str, k: usize, cue_vector: Option<&[f32]>) -> Vec<ChorusHit> {
        self.recall_with_opts(cue, k, cue_vector, ChorusRecallOpts::default())
    }

    /// Late-interaction recall: token-population MaxSim ⊕ BM25, fused by RRF.
    ///
    /// `query_tokens` are the query's per-token MiniLM vectors (f32). `cue_vector`
    /// is the pooled query vector, used only for the photon prefilter on large
    /// stores. `w_bm` weights the BM25 channel (0 => default 0.7). Falls back to
    /// scoring all traces when the store fits the full-readout budget.
    pub fn recall_maxsim(
        &self,
        cue: &str,
        k: usize,
        query_tokens: &[Vec<f32>],
        cue_vector: Option<&[f32]>,
        w_bm: f32,
    ) -> Vec<ChorusHit> {
        if self.traces.is_empty() || query_tokens.is_empty() {
            return Vec::new();
        }
        // 1. Prefilter candidates: full readout if it fits, else photon shortlist.
        let candidate_ids: Vec<String> =
            if self.traces.len() <= self.config.spectrum_full_readout_max {
                self.trace_order.clone()
            } else if let Some(v) = cue_vector.filter(|v| !v.is_empty()) {
                let qcode = self.encode_vector(&as_unit_vector(v));
                let gate_k = self
                    .config
                    .grg_shortlist_k
                    .max(k.saturating_mul(32))
                    .min(self.photon.len().max(1));
                self.grg_shortlist(&qcode, gate_k)
            } else {
                self.trace_order.clone()
            };

        let q: Vec<Vec<f32>> = query_tokens
            .iter()
            .map(|t| as_unit_vector(t).into_owned())
            .collect();
        // Predictive-coding salience weights for the query tokens (computed once).
        let weights = salience_weights(&q);
        let terms = tokenize(cue);
        const TAU: f32 = 1.0;
        const WINDOW: u32 = 8;

        // 2+3. Score both channels, keyed by parent id (max over any split children).
        //   dense: salience-gated MaxSim over per-token vectors
        //   lex:   conjunctive surprisal (proximity-bound co-occurring rare terms)
        let mut dense_by_id: HashMap<String, f32> = HashMap::new();
        let mut lex_by_id: HashMap<String, f32> = HashMap::new();
        for id in &candidate_ids {
            let Some(trace) = self
                .traces
                .get(id)
                .or_else(|| self.find_trace_by_parent(id))
            else {
                continue;
            };
            let pid = parent_memory_id(&trace.memory_id).to_string();
            let ms = if trace.token_vectors.is_empty() {
                0.0
            } else {
                maxsim_weighted(&q, &weights, &trace.token_vectors)
            };
            let e = dense_by_id.entry(pid.clone()).or_insert(f32::NEG_INFINITY);
            if ms > *e {
                *e = ms;
            }
            // Lexical keyed by the trace's own id (unique per row); roll up to parent.
            let lx = self
                .bm25
                .surprisal_conjunctive(&terms, &trace.memory_id, TAU, WINDOW);
            let e2 = lex_by_id.entry(pid).or_insert(0.0);
            if lx > *e2 {
                *e2 = lx;
            }
        }
        // clean sentinel (candidates always get a finite MaxSim, but be safe)
        for v in dense_by_id.values_mut() {
            if !v.is_finite() {
                *v = 0.0;
            }
        }

        // 4+5. Evidence-integration fusion (Ernst–Banks): z-score each channel,
        // reliability-weighted sum. w_bm is reused as the lexical reliability weight.
        let wb = if w_bm > 0.0 { w_bm } else { 0.6 };
        let fused = evidence_fuse(&dense_by_id, &lex_by_id, wb);

        // 6. Emit hits.
        fused
            .into_iter()
            .take(k)
            .map(|(memory_id, score)| ChorusHit {
                memory_id,
                score,
                photon: 0.0,
                field: 0.0,
                lexical: 0.0,
                theta: 0,
                lane: "grg-salience-conjunctive-evidence".into(),
                snippet: String::new(),
            })
            .collect()
    }

    /// Batch recall — one GRG pass per query, amortizes Python/FFI overhead.
    pub fn recall_batch(
        &self,
        queries: &[(&str, Option<&[f32]>)],
        k: usize,
        opts: ChorusRecallOpts,
    ) -> Vec<Vec<ChorusHit>> {
        queries
            .iter()
            .map(|(cue, vec)| self.recall_with_opts(cue, k, *vec, opts))
            .collect()
    }

    pub fn recall_with_opts(
        &self,
        cue: &str,
        k: usize,
        cue_vector: Option<&[f32]>,
        opts: ChorusRecallOpts,
    ) -> Vec<ChorusHit> {
        if self.traces.is_empty() {
            return Vec::new();
        }
        let cue_unit = cue_vector.filter(|v| !v.is_empty()).map(as_unit_vector);

        // PRISM: RaBitQ popcount rank on all gated traces + SPECTRUM certify top-M only.
        if opts.fast
            && self.config.prism_certify_m > 0
            && cue_unit.is_some()
            && k <= self.config.prism_max_k
        {
            let qv = cue_unit.as_deref().unwrap();
            let (query_prism, query_qjl, query_cert) = prism::query_from_vector(qv);
            let query_code = self.encode_vector(qv);
            let full_prism = self.traces.len() <= self.config.prism_full_readout_max;
            let candidate_ids: Vec<String> = if full_prism {
                self.trace_order.clone()
            } else {
                let gate_k = self
                    .config
                    .grg_shortlist_k
                    .max(k.saturating_mul(32))
                    .min(self.photon.len().max(1));
                self.grg_shortlist(&query_code, gate_k)
            };
            let mut refs: Vec<(String, &PrismSignature)> = Vec::with_capacity(candidate_ids.len());
            for id in &candidate_ids {
                if let Some(trace) = self
                    .traces
                    .get(id)
                    .or_else(|| self.find_trace_by_parent(id))
                {
                    if let Some(ref ps) = trace.prism {
                        refs.push((parent_memory_id(&trace.memory_id).to_string(), ps));
                    }
                }
            }
            if !refs.is_empty() {
                let certify_m = self
                    .config
                    .prism_certify_m
                    .max(k.saturating_mul(12))
                    .max(if k > 64 { 1024 } else { 256 })
                    .min(refs.len());
                let float_rerank = opts.float_rerank && self.config.prism_float_rerank;
                let final_k = if float_rerank { usize::MAX } else { k };
                let mut ranked = prism::rank_and_certify(
                    &refs,
                    &query_prism,
                    &query_qjl,
                    &query_cert,
                    final_k,
                    certify_m,
                );
                if float_rerank {
                    self.float_rerank_pool(&mut ranked, qv);
                    ranked.truncate(k);
                }
                return ranked
                    .into_iter()
                    .map(|(memory_id, score)| ChorusHit {
                        memory_id,
                        score,
                        photon: 0.0,
                        field: 0.0,
                        lexical: 0.0,
                        theta: 0,
                        lane: if full_prism {
                            if float_rerank {
                                "grg-prism-exact".into()
                            } else {
                                "grg-prism".into()
                            }
                        } else if float_rerank {
                            "grg-prism-gated-exact".into()
                        } else {
                            "grg-prism-gated".into()
                        },
                        snippet: String::new(),
                    })
                    .collect();
            }
        }

        let query_code = match cue_unit.as_deref() {
            Some(v) => self.encode_vector(v),
            None => self.hasher.encode(&Self::wavelet_taps(cue, "", 64)),
        };
        let query_spectrum = cue_unit.as_deref().map(SpectrumSignature::from_vector);

        // GRG gate: photon addressing. SPECTRUM readout scores all gated traces — no shortlist
        // recall loss on IR corpora that fit the full-readout budget.
        let spectrum_readout = query_spectrum.is_some();
        let full_spectrum =
            spectrum_readout && self.traces.len() <= self.config.spectrum_full_readout_max;
        let candidate_ids: Vec<String> = if full_spectrum {
            self.trace_order.clone()
        } else if spectrum_readout {
            let gate_k = self
                .config
                .grg_shortlist_k
                .max(k.saturating_mul(16))
                .min(self.photon.len().max(1));
            self.grg_shortlist(&query_code, gate_k)
        } else {
            let shortlist_k = self
                .config
                .grg_shortlist_k
                .max(k.saturating_mul(4))
                .min(self.photon.len().max(1));
            self.grg_shortlist(&query_code, shortlist_k)
        };

        let query_theta = self.theta_clock;
        let lane = if full_spectrum && opts.fast {
            "grg-spectrum"
        } else if spectrum_readout && opts.fast {
            "grg-spectrum-gated"
        } else if opts.fast {
            "grg-fast"
        } else {
            "grg"
        };
        let mut hits: Vec<ChorusHit> = candidate_ids
            .iter()
            .filter_map(|id| {
                self.traces
                    .get(id)
                    .or_else(|| self.find_trace_by_parent(id))
            })
            .map(|trace| {
                let spectrum_sim = match (query_spectrum.as_ref(), trace.spectrum.as_ref()) {
                    (Some(qs), Some(ts)) if !ts.is_empty() => Some(qs.dot_similarity(ts)),
                    _ => None,
                };
                let (vector_sim, has_vector) = match (cue_unit.as_deref(), trace.vector.as_deref())
                {
                    (Some(qv), Some(tv)) if !tv.is_empty() => (dot_similarity(qv, tv), true),
                    _ => (0.0, false),
                };
                if opts.fast {
                    if let Some(sim) = spectrum_sim {
                        return ChorusHit {
                            memory_id: parent_memory_id(&trace.memory_id).to_string(),
                            score: sim,
                            photon: 0.0,
                            field: 0.0,
                            lexical: 0.0,
                            theta: trace.theta,
                            lane: lane.into(),
                            snippet: String::new(),
                        };
                    }
                    if has_vector {
                        return ChorusHit {
                            memory_id: parent_memory_id(&trace.memory_id).to_string(),
                            score: vector_sim,
                            photon: 0.0,
                            field: 0.0,
                            lexical: 0.0,
                            theta: trace.theta,
                            lane: lane.into(),
                            snippet: String::new(),
                        };
                    }
                }
                let photon = ((query_code.estimated_cosine(&trace.code)) + 1.0) / 2.0;
                let field = self.field_coherence(trace, query_theta);
                let lexical = Self::lexical_overlap(cue, &trace.content);
                let verified_boost = if trace.sheath.verified { 0.04 } else { 0.0 };
                let replay_boost = (trace.replay_tag as f32 * 0.01).min(0.08);
                let score = if let Some(sim) = spectrum_sim {
                    sim + 0.01 * field + 0.01 * lexical + verified_boost + replay_boost
                } else if has_vector {
                    vector_sim
                        + 0.02 * photon
                        + 0.01 * field
                        + 0.01 * lexical
                        + verified_boost
                        + replay_boost
                } else {
                    0.55 * photon + 0.30 * field + 0.15 * lexical + verified_boost + replay_boost
                };
                ChorusHit {
                    memory_id: parent_memory_id(&trace.memory_id).to_string(),
                    score,
                    photon,
                    field,
                    lexical,
                    theta: trace.theta,
                    lane: lane.into(),
                    snippet: if opts.fast {
                        String::new()
                    } else {
                        trace.content.chars().take(280).collect()
                    },
                }
            })
            .collect();

        hits.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        hits = dedupe_hits_by_id(hits);
        hits.truncate(k);
        hits
    }

    /// SWR-TAG: tag traces that participated in a recall for sleep triage.
    pub fn tag_recall_hits(&mut self, hits: &[ChorusHit]) {
        for hit in hits {
            let parent = parent_memory_id(&hit.memory_id);
            if let Some(trace) = self.traces.get_mut(parent) {
                trace.replay_tag = trace.replay_tag.saturating_add(1);
                continue;
            }
            for (id, trace) in self.traces.iter_mut() {
                if parent_memory_id(id) == parent {
                    trace.replay_tag = trace.replay_tag.saturating_add(1);
                }
            }
        }
    }

    /// θ-sweep sleep — collapse high-amplitude / high-replay traces to promotion queue.
    pub fn sleep_sweep(&mut self) -> (ChorusSleepReport, Vec<ChorusTrace>) {
        let mut report = ChorusSleepReport {
            theta_sweeps: THETA_BINS,
            ..Default::default()
        };
        let mut promote: Vec<ChorusTrace> = Vec::new();
        for _ in 0..THETA_BINS {
            self.theta_clock = (self.theta_clock + 1) % THETA_BINS;
            let theta = self.theta_clock;
            let mut scored: Vec<(f32, String)> = self
                .traces
                .values()
                .map(|t| {
                    let verified = if t.sheath.verified { 0.35 } else { 0.0 };
                    let priority = t.amplitude * (1.0 + t.replay_tag as f32 * 0.35)
                        + self.field_coherence(t, theta)
                        + verified;
                    (priority, t.memory_id.clone())
                })
                .collect();
            scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
            for (priority, id) in scored.into_iter().take(32) {
                if priority < self.config.collapse_threshold {
                    continue;
                }
                if let Some(trace) = self.traces.get(&id).cloned() {
                    if !promote.iter().any(|p| p.memory_id == id) {
                        promote.push(trace);
                        report.collapsed += 1;
                    }
                }
            }
        }
        report.splits = self.splits_total;
        (report, promote)
    }

    pub fn decay_untagged(&mut self, _min_replay: u32) -> u32 {
        let mut pruned = 0u32;
        let ids: Vec<String> = self.traces.keys().cloned().collect();
        for id in ids {
            let Some(trace) = self.traces.get_mut(&id) else {
                continue;
            };
            if trace.replay_tag > 0 || trace.sheath.verified {
                continue;
            }
            trace.amplitude *= 0.85;
            if trace.amplitude < 0.08 {
                self.traces.remove(&id);
                self.bm25.remove(&id);
                pruned += 1;
            }
        }
        self.trace_order.retain(|id| self.traces.contains_key(id));
        self.photon.retain(|id| self.traces.contains_key(id));
        pruned
    }

    /// Remove traces matching predicate (governance / GDPR delete).
    pub fn remove_matching<F>(&mut self, pred: F) -> u32
    where
        F: Fn(&ChorusTrace) -> bool,
    {
        let ids: Vec<String> = self
            .traces
            .iter()
            .filter_map(|(id, t)| if pred(t) { Some(id.clone()) } else { None })
            .collect();
        let n = ids.len() as u32;
        for id in ids {
            self.traces.remove(&id);
            self.bm25.remove(&id);
        }
        self.trace_order.retain(|id| self.traces.contains_key(id));
        self.photon.retain(|id| self.traces.contains_key(id));
        n
    }

    pub fn get_trace(&self, memory_id: &str) -> Option<&ChorusTrace> {
        self.traces.get(memory_id)
    }

    pub fn find_trace_by_parent(&self, parent_id: &str) -> Option<&ChorusTrace> {
        if let Some(t) = self.traces.get(parent_id) {
            return Some(t);
        }
        self.traces
            .values()
            .find(|t| parent_memory_id(&t.memory_id) == parent_id)
    }

    /// Float32 gold rerank on PRISM/SPECTRUM certify pool — exact cosine order.
    fn float_rerank_pool(&self, ranked: &mut [(String, f32)], query: &[f32]) {
        for slot in ranked.iter_mut() {
            if let Some(trace) = self.find_trace_by_parent(&slot.0) {
                if let Some(ref v) = trace.vector {
                    if !v.is_empty() {
                        slot.1 = dot_similarity(query, v);
                    }
                }
            }
        }
        ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    }
}

fn normalize_vector(v: &[f32]) -> Vec<f32> {
    let mut norm = 0.0f32;
    for &x in v {
        norm += x * x;
    }
    let n = norm.sqrt();
    if n <= 1e-8 {
        return vec![0.0; v.len()];
    }
    v.iter().map(|x| x / n).collect()
}

fn as_unit_vector(v: &[f32]) -> Cow<'_, [f32]> {
    let mut norm = 0.0f32;
    for &x in v {
        norm += x * x;
    }
    if (norm - 1.0).abs() < 0.02 {
        return Cow::Borrowed(v);
    }
    Cow::Owned(normalize_vector(v))
}

fn dot_similarity(a: &[f32], b: &[f32]) -> f32 {
    let n = a.len().min(b.len());
    if n == 0 {
        return 0.0;
    }
    let mut dot = 0.0f32;
    for i in 0..n {
        dot += a[i] * b[i];
    }
    dot.clamp(-1.0, 1.0)
}

pub(crate) fn parent_memory_id(id: &str) -> &str {
    id.split("#s").next().unwrap_or(id)
}

fn dedupe_hits_by_id(hits: Vec<ChorusHit>) -> Vec<ChorusHit> {
    let mut best: HashMap<String, ChorusHit> = HashMap::new();
    for hit in hits {
        best.entry(hit.memory_id.clone())
            .and_modify(|prev| {
                if hit.score > prev.score {
                    *prev = hit.clone();
                }
            })
            .or_insert(hit);
    }
    let mut out: Vec<ChorusHit> = best.into_values().collect();
    out.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    out
}

fn provenance_phase(sheath: &ProvenanceSheath) -> f32 {
    let base = sheath.provenance_kind as f32 * 0.4;
    if sheath.verified {
        base + std::f32::consts::FRAC_PI_2
    } else {
        base
    }
}

fn circular_delta(a: u8, b: u8) -> u8 {
    let d = a.abs_diff(b);
    d.min(THETA_BINS - d)
}

fn hash_str(s: &str) -> u64 {
    let mut h = 0xcbf29ce484222325u64;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

fn hash64_parts(parts: &[u64]) -> u64 {
    let mut h = 0x9E37_79B1_85EB_CA87u64;
    for &p in parts {
        h ^= p;
        h = h.wrapping_mul(0xBF58476D1CE4E5B9);
    }
    h
}

fn splitmix64_plane(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9E3779B97F4A7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
    z ^ (z >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vec_unit(seed: u8, dim: usize) -> Vec<f32> {
        (0..dim)
            .map(|i| {
                let h = hash_str(&format!("vec-{seed}-{i}"));
                ((h % 20001) as f32 / 10000.0) - 1.0
            })
            .collect()
    }

    #[test]
    fn imprint_and_recall_top1() {
        let mut field = ChorusField::default();
        let target = ChorusImprintInput {
            memory_id: "doc-a".into(),
            content: "magnesium supplementation improves sleep quality".into(),
            context: "doc-a".into(),
            semantic_vector: Some(vec_unit(1, 32)),
            token_vectors: None,
            salience: 0.8,
            sheath: Default::default(),
        };
        let other = ChorusImprintInput {
            memory_id: "doc-b".into(),
            content: "quantum chromodynamics lattice gauge theory".into(),
            context: "doc-b".into(),
            semantic_vector: Some(vec_unit(99, 32)),
            token_vectors: None,
            salience: 0.8,
            sheath: Default::default(),
        };
        assert!(field.imprint(&target));
        assert!(field.imprint(&other));
        let hits = field.recall("magnesium sleep", 3, target.semantic_vector.as_deref());
        assert!(!hits.is_empty());
        assert_eq!(hits[0].memory_id, "doc-a");
    }

    #[test]
    fn recall_maxsim_ranks_token_and_lexical_match() {
        let mut field = ChorusField::default();
        // Two token vectors per doc; doc-a shares a token with the query.
        let a = ChorusImprintInput {
            memory_id: "doc-a".into(),
            content: "caroline visited the lgbtq support group in may".into(),
            context: "doc-a".into(),
            semantic_vector: Some(vec![1.0, 0.0, 0.0]),
            token_vectors: Some(vec![vec![1.0, 0.0, 0.0], vec![0.0, 1.0, 0.0]]),
            salience: 0.7,
            sheath: Default::default(),
        };
        let b = ChorusImprintInput {
            memory_id: "doc-b".into(),
            content: "quantum chromodynamics lattice gauge theory".into(),
            context: "doc-b".into(),
            semantic_vector: Some(vec![0.0, 0.0, 1.0]),
            token_vectors: Some(vec![vec![0.0, 0.0, 1.0], vec![0.0, 0.0, -1.0]]),
            salience: 0.7,
            sheath: Default::default(),
        };
        assert!(field.imprint(&a));
        assert!(field.imprint(&b));
        // Query token aligns with doc-a's first token AND lexical "caroline lgbtq".
        let qtok = vec![vec![1.0, 0.0, 0.0]];
        let hits = field.recall_maxsim("caroline lgbtq", 2, &qtok, Some(&[1.0, 0.0, 0.0]), 0.7);
        assert!(!hits.is_empty());
        assert_eq!(
            hits[0].memory_id, "doc-a",
            "invented stack should rank doc-a first"
        );
        assert_eq!(hits[0].lane, "grg-salience-conjunctive-evidence");
    }

    #[test]
    fn dedup_skips_near_duplicate() {
        let mut field = ChorusField::default();
        let row = ChorusImprintInput {
            memory_id: "x1".into(),
            content: "same text".into(),
            context: "c".into(),
            semantic_vector: Some(vec_unit(5, 32)),
            token_vectors: None,
            salience: 0.7,
            sheath: Default::default(),
        };
        assert!(field.imprint(&row));
        let dup = ChorusImprintInput {
            memory_id: "x2".into(),
            ..row.clone()
        };
        assert!(!field.imprint(&dup));
        assert_eq!(field.len(), 1);
    }

    #[test]
    fn batch_imprint_count() {
        let mut field = ChorusField::new(ChorusConfig {
            dedup_hamming: 0,
            split_threshold: 1000.0,
            ..Default::default()
        });
        let batch: Vec<_> = (0..50)
            .map(|i| ChorusImprintInput {
                memory_id: format!("m{i}"),
                content: format!("content topic {i} alpha beta"),
                context: format!("ctx{i}"),
                semantic_vector: Some(vec_unit(i as u8, 32)),
                token_vectors: None,
                salience: 0.6,
                sheath: Default::default(),
            })
            .collect();
        let n = field.imprint_batch(&batch);
        assert_eq!(n, 50);
        assert_eq!(field.len(), 50);
    }

    #[test]
    fn sleep_sweep_promotes_high_replay() {
        let mut field = ChorusField::new(ChorusConfig {
            split_threshold: 1000.0,
            collapse_threshold: 1.2,
            ..Default::default()
        });
        let row = ChorusImprintInput {
            memory_id: "promote-me".into(),
            content: "user prefers dark mode in the IDE".into(),
            context: "settings".into(),
            semantic_vector: Some(vec_unit(7, 32)),
            token_vectors: None,
            salience: 0.9,
            sheath: ProvenanceSheath {
                verified: true,
                ..Default::default()
            },
        };
        field.imprint(&row);
        let hits = field.recall("dark mode", 1, row.semantic_vector.as_deref());
        field.tag_recall_hits(&hits);
        let (report, queue) = field.sleep_sweep();
        assert!(report.collapsed > 0);
        assert!(!queue.is_empty());
    }

    #[test]
    fn grg_recall_latency_budget() {
        let mut field = ChorusField::new(ChorusConfig {
            split_threshold: 1000.0,
            ..Default::default()
        });
        let batch: Vec<_> = (0..5000)
            .map(|i| ChorusImprintInput {
                memory_id: format!("m{i}"),
                content: format!("topic {i}"),
                context: format!("c{i}"),
                semantic_vector: Some(vec_unit(i as u8, 32)),
                token_vectors: None,
                salience: 0.6,
                sheath: Default::default(),
            })
            .collect();
        field.imprint_batch(&batch);
        let cue = vec_unit(42, 32);
        let start = std::time::Instant::now();
        for _ in 0..200 {
            let _ = field.recall("topic 42", 100, Some(&cue));
        }
        let elapsed = start.elapsed().as_secs_f64() * 1000.0 / 200.0;
        eprintln!("GRG recall avg: {elapsed:.3} ms (5k traces, k=100)");
        assert!(elapsed < 12.0, "GRG recall too slow: {elapsed} ms");
    }

    #[test]
    fn spectrum_readout_finds_exact_match_at_5k() {
        let mut field = ChorusField::new(ChorusConfig {
            split_threshold: 1000.0,
            grg_shortlist_k: 64,
            ..Default::default()
        });
        let batch: Vec<_> = (0..5000)
            .map(|i| ChorusImprintInput {
                memory_id: format!("m{i}"),
                content: format!("topic {i}"),
                context: format!("c{i}"),
                semantic_vector: Some(vec_unit(i as u8, 32)),
                token_vectors: None,
                salience: 0.6,
                sheath: Default::default(),
            })
            .collect();
        field.imprint_batch(&batch);
        let cue = vec_unit(42, 32);
        let hits =
            field.recall_with_opts("topic 42", 100, Some(&cue), ChorusRecallOpts::ir_vector());
        assert!(!hits.is_empty());
        assert_eq!(hits[0].memory_id, "m42");
        assert_eq!(hits[0].lane, "grg-prism-exact");
    }

    #[test]
    fn grg_recall_uses_gamma_gate_lane() {
        let mut field = ChorusField::new(ChorusConfig {
            split_threshold: 1000.0,
            grg_shortlist_k: 64,
            ..Default::default()
        });
        let batch: Vec<_> = (0..200)
            .map(|i| ChorusImprintInput {
                memory_id: format!("m{i}"),
                content: format!("topic {i}"),
                context: format!("c{i}"),
                semantic_vector: Some(vec_unit(i as u8, 32)),
                token_vectors: None,
                salience: 0.6,
                sheath: Default::default(),
            })
            .collect();
        field.imprint_batch(&batch);
        let hits = field.recall("topic 7", 8, Some(&vec_unit(7, 32)));
        assert!(!hits.is_empty());
        assert_eq!(hits[0].lane, "grg");
    }

    #[test]
    fn theta_tick_cycles() {
        let mut field = ChorusField::default();
        assert_eq!(field.tick_theta(), 1);
        assert_eq!(field.tick_theta(), 2);
    }
}
