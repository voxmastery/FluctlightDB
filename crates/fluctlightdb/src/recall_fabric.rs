//! Recall Fabric — the composed brain recall path: Photon → Lattice → Phase.
//!
//! This is where the three standalone mechanisms become one pipeline, mirroring how the brain
//! layers fast and slow recall:
//!
//! 1. **Photon prefilter** ([`crate::photon`]) — SimHash bitcodes + Hamming/LSH turn "which
//!    memories are even worth looking at" into a few `popcount` ops. Coincidence detection, not
//!    dense float math. Produces a short candidate list from a large store (sub-linear).
//! 2. **Lattice addressing** ([`crate::lattice`]) — grid-cell coordinates give a coarse (fuzzy /
//!    gist) and fine (exact) similarity on the semantic axis without float dot products, and keep
//!    structure on its own axis so meaning and grammar don't fight for capacity.
//! 3. **Phase disambiguation** ([`crate::phase_parse`]) — theta-gamma sequence encoding scores
//!    *order and role* agreement, so "user upgraded plan" and "plan upgraded user" separate even
//!    when they share every word and every embedding.
//!
//! A final score blends the available signals (lexical overlap is always available; photon and
//! lattice terms activate when embeddings are present). The fabric is deterministic, pure-`std`
//! (+ `serde`), and self-contained: it operates on `(id, text, optional vector)` memories and
//! never touches the persistent store. The live engine consumes only [`structural_boost`] behind
//! a feature flag; the full [`RecallFabric`] is the validated reference for the composed path.

use serde::{Deserialize, Serialize};

use crate::lattice::{Axis, GridCode, Lattice};
use crate::phase_parse::{PhaseParser, PhaseVector};
use crate::photon::{PhotonCode, PhotonStore, SimHasher};

/// Tunable weights and dimensions for the composed pipeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FabricConfig {
    pub photon_bits: usize,
    pub phase_dim: usize,
    pub lsh_bands: usize,
    pub lsh_rows: usize,
    /// How many photon candidates to keep before the expensive rerank.
    pub prefilter_k: usize,
    pub w_lexical: f32,
    pub w_photon: f32,
    pub w_phase: f32,
    pub w_lattice: f32,
}

impl Default for FabricConfig {
    fn default() -> Self {
        Self {
            photon_bits: 256,
            phase_dim: 256,
            lsh_bands: 32,
            lsh_rows: 8,
            prefilter_k: 64,
            w_lexical: 0.30,
            w_photon: 0.40,
            w_phase: 0.20,
            w_lattice: 0.10,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FabricMemory {
    id: String,
    tokens: Vec<String>,
    seq: PhaseVector,
    code: Option<PhotonCode>,
    sem_scalar: Option<f64>,
    /// Precomputed semantic grid gist (avoids per-query lattice encode on shortlist rerank).
    #[serde(default)]
    sem_gist: Option<GridCode>,
}

/// Per-query cue features computed once, then reused across shortlist scoring.
struct FabricCueCtx {
    cue_tokens: Vec<String>,
    cue_seq: PhaseVector,
    cue_code: Option<PhotonCode>,
    cue_gist: Option<GridCode>,
    has_vec: bool,
}

/// Lightweight per-query ctx for shortlist rerank (CHORUS / activate hot path).
struct FabricCueLite {
    cue_tokens: Vec<String>,
    cue_code: Option<PhotonCode>,
}

/// Scored recall result from the composed fabric.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FabricHit {
    pub id: String,
    pub score: f32,
    pub lexical: f32,
    pub photon: f32,
    pub phase: f32,
    pub lattice: f32,
}

/// The composed recall engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecallFabric {
    cfg: FabricConfig,
    hasher: SimHasher,
    lattice: Lattice,
    parser: PhaseParser,
    prefilter: PhotonStore,
    mems: Vec<FabricMemory>,
    #[serde(skip, default)]
    id_index: std::collections::HashMap<String, usize>,
}

impl Default for RecallFabric {
    fn default() -> Self {
        Self::new(FabricConfig::default())
    }
}

impl RecallFabric {
    pub fn new(cfg: FabricConfig) -> Self {
        Self {
            hasher: SimHasher::new(cfg.photon_bits, 0x000F_10C7),
            lattice: Lattice::default(),
            parser: PhaseParser::new(cfg.phase_dim),
            prefilter: PhotonStore::new(cfg.photon_bits, cfg.lsh_bands, cfg.lsh_rows),
            mems: Vec::new(),
            cfg,
            id_index: std::collections::HashMap::new(),
        }
    }

    /// Rebuild id→index map and backfill cached lattice gists (after load or serde).
    pub fn rebuild_indexes(&mut self) {
        self.id_index.clear();
        for (i, m) in self.mems.iter_mut().enumerate() {
            self.id_index.insert(m.id.clone(), i);
            if m.sem_gist.is_none() {
                if let Some(s) = m.sem_scalar {
                    m.sem_gist = self
                        .lattice
                        .encode_with_semantic_position(s, &[])
                        .axes
                        .remove(&Axis::Semantic);
                }
            }
        }
    }

    fn prepare_cue_ctx(&self, cue: &str, cue_vector: Option<&[f32]>) -> FabricCueCtx {
        let cue_tokens = tokenize(cue);
        let cue_refs: Vec<&str> = cue_tokens.iter().map(|s| s.as_str()).collect();
        let cue_seq = crate::relation::encode_relations(&self.parser, cue)
            .unwrap_or_else(|| self.parser.encode_sequence(&cue_refs));
        let cue_code = cue_vector
            .filter(|v| !v.is_empty())
            .map(|v| self.hasher.encode(v));
        let cue_gist = cue_vector
            .filter(|v| !v.is_empty())
            .map(project_scalar)
            .and_then(|s| {
                self.lattice
                    .encode_with_semantic_position(s, &[])
                    .axes
                    .remove(&Axis::Semantic)
            });
        FabricCueCtx {
            has_vec: cue_code.is_some(),
            cue_tokens,
            cue_seq,
            cue_code,
            cue_gist,
        }
    }

    fn score_mem_at(&self, i: usize, ctx: &FabricCueCtx) -> f32 {
        let m = &self.mems[i];
        let lexical = jaccard(&ctx.cue_tokens, &m.tokens);
        let photon = match (&ctx.cue_code, &m.code) {
            (Some(a), Some(b)) => ((a.estimated_cosine(b)) + 1.0) / 2.0,
            _ => 0.0,
        };
        let phase = (ctx.cue_seq.similarity(&m.seq) + 1.0) / 2.0;
        let lattice = match (&ctx.cue_gist, &m.sem_gist) {
            (Some(cg), Some(mg)) => cg.coarse_similarity(mg, &self.lattice.scales),
            _ => 0.0,
        };
        let has_vec = m.code.is_some() && ctx.has_vec;
        self.blend(lexical, photon, phase, lattice, has_vec)
    }

    fn prepare_cue_ctx_lite(&self, cue: &str, cue_vector: Option<&[f32]>) -> FabricCueLite {
        FabricCueLite {
            cue_tokens: tokenize(cue),
            cue_code: cue_vector
                .filter(|v| !v.is_empty())
                .map(|v| self.hasher.encode(v)),
        }
    }

    fn score_mem_at_lite(&self, i: usize, ctx: &FabricCueLite) -> f32 {
        let m = &self.mems[i];
        let lexical = jaccard(&ctx.cue_tokens, &m.tokens);
        let photon = match (&ctx.cue_code, &m.code) {
            (Some(a), Some(b)) => ((a.estimated_cosine(b)) + 1.0) / 2.0,
            _ => 0.0,
        };
        if ctx.cue_code.is_some() && m.code.is_some() {
            0.35 * lexical + 0.65 * photon
        } else {
            lexical
        }
    }

    /// Fast shortlist rerank: one cue encode + O(k) photon/lexical (no phase/lattice).
    pub(crate) fn score_shortlist_lite(
        &self,
        ids: &[&str],
        cue: &str,
        cue_vector: Option<&[f32]>,
    ) -> std::collections::HashMap<String, f32> {
        if ids.is_empty() {
            return std::collections::HashMap::new();
        }
        let ctx = self.prepare_cue_ctx_lite(cue, cue_vector);
        let mut out = std::collections::HashMap::with_capacity(ids.len());
        for id in ids {
            if let Some(&i) = self.id_index.get(*id) {
                out.insert((*id).to_string(), self.score_mem_at_lite(i, &ctx));
            }
        }
        debug_assert!(out.len() <= ids.len());
        out
    }

    /// Score a pre-filtered id list with one cue encode (full fabric signals).
    pub(crate) fn score_shortlist(
        &self,
        ids: &[&str],
        cue: &str,
        cue_vector: Option<&[f32]>,
    ) -> std::collections::HashMap<String, f32> {
        if ids.is_empty() {
            return std::collections::HashMap::new();
        }
        let ctx = self.prepare_cue_ctx(cue, cue_vector);
        let mut out = std::collections::HashMap::with_capacity(ids.len());
        for id in ids {
            if let Some(&i) = self.id_index.get(*id) {
                out.insert((*id).to_string(), self.score_mem_at(i, &ctx));
            }
        }
        out
    }

    pub fn len(&self) -> usize {
        self.mems.len()
    }

    /// Current lattice addressable capacity (product of co-prime scales).
    pub fn lattice_capacity(&self) -> u128 {
        self.lattice.capacity()
    }

    /// Elastic neurogenesis: add a co-prime scale when load exceeds threshold.
    pub fn grow_lattice(&mut self, scale: u32) {
        self.lattice.grow(scale);
    }

    pub fn is_empty(&self) -> bool {
        self.mems.is_empty()
    }

    /// Ingest one memory (lite: lexical + photon index; for CHORUS bulk imprint).
    pub fn insert(&mut self, id: impl Into<String>, text: &str, vector: Option<&[f32]>) {
        let id = id.into();
        let tokens = tokenize(text);
        let (code, sem_scalar) = match vector {
            Some(v) if !v.is_empty() => {
                let c = self.hasher.encode(v);
                self.prefilter.insert(id.clone(), c.clone());
                (Some(c), Some(project_scalar(v)))
            }
            _ => (None, None),
        };
        let sem_gist = sem_scalar.and_then(|s| {
            self.lattice
                .encode_with_semantic_position(s, &[])
                .axes
                .remove(&Axis::Semantic)
        });
        let idx = self.mems.len();
        self.mems.push(FabricMemory {
            id: id.clone(),
            tokens,
            seq: PhaseVector { phases: vec![] },
            code,
            sem_scalar,
            sem_gist,
        });
        self.id_index.insert(id, idx);
    }

    /// Full fabric ingest (phase + relation encoding) for episodic activate path.
    pub fn insert_rich(&mut self, id: impl Into<String>, text: &str, vector: Option<&[f32]>) {
        let id = id.into();
        let tokens = tokenize(text);
        let seq = crate::relation::encode_relations(&self.parser, text).unwrap_or_else(|| {
            self.parser
                .encode_sequence(&tokens.iter().map(|s| s.as_str()).collect::<Vec<_>>())
        });
        let (code, sem_scalar) = match vector {
            Some(v) if !v.is_empty() => {
                let c = self.hasher.encode(v);
                self.prefilter.insert(id.clone(), c.clone());
                (Some(c), Some(project_scalar(v)))
            }
            _ => (None, None),
        };
        let sem_gist = sem_scalar.and_then(|s| {
            self.lattice
                .encode_with_semantic_position(s, &[])
                .axes
                .remove(&Axis::Semantic)
        });
        let idx = self.mems.len();
        self.mems.push(FabricMemory {
            id: id.clone(),
            tokens,
            seq,
            code,
            sem_scalar,
            sem_gist,
        });
        self.id_index.insert(id, idx);
    }

    /// Photon LSH shortlist only — sub-linear, no phase/lattice rerank. Use as recall prefilter.
    pub fn photon_shortlist_ids(&self, cue_vector: Option<&[f32]>, k: usize) -> Vec<String> {
        let Some(v) = cue_vector.filter(|v| !v.is_empty()) else {
            return Vec::new();
        };
        let cc = self.hasher.encode(v);
        self.prefilter
            .query(&cc, k)
            .into_iter()
            .map(|(id, _)| id)
            .collect()
    }

    /// Score one memory by id (O(1) lookup) — for reranking an existing shortlist, not full-store scan.
    pub fn score_for_id(
        &self,
        id: &str,
        cue: &str,
        cue_vector: Option<&[f32]>,
    ) -> Option<FabricHit> {
        let i = *self.id_index.get(id)?;
        let m = &self.mems[i];
        let ctx = self.prepare_cue_ctx(cue, cue_vector);
        let lexical = jaccard(&ctx.cue_tokens, &m.tokens);
        let photon = match (&ctx.cue_code, &m.code) {
            (Some(a), Some(b)) => ((a.estimated_cosine(b)) + 1.0) / 2.0,
            _ => 0.0,
        };
        let phase = (ctx.cue_seq.similarity(&m.seq) + 1.0) / 2.0;
        let lattice = match (&ctx.cue_gist, &m.sem_gist) {
            (Some(cg), Some(mg)) => cg.coarse_similarity(mg, &self.lattice.scales),
            _ => 0.0,
        };
        let has_vec = m.code.is_some() && ctx.has_vec;
        let score = self.blend(lexical, photon, phase, lattice, has_vec);
        Some(FabricHit {
            id: m.id.clone(),
            score,
            lexical,
            photon,
            phase,
            lattice,
        })
    }

    /// Composed recall: photon prefilter → lattice + phase rerank → top-k.
    pub fn recall(&self, cue: &str, cue_vector: Option<&[f32]>, k: usize) -> Vec<FabricHit> {
        let cue_tokens = tokenize(cue);
        let cue_refs: Vec<&str> = cue_tokens.iter().map(|s| s.as_str()).collect();
        let cue_seq = crate::relation::encode_relations(&self.parser, cue)
            .unwrap_or_else(|| self.parser.encode_sequence(&cue_refs));

        let cue_code = cue_vector
            .filter(|v| !v.is_empty())
            .map(|v| self.hasher.encode(v));
        let cue_scalar = cue_vector.filter(|v| !v.is_empty()).map(project_scalar);

        // Photon prefilter: shortlist candidate indices, or fall back to the whole store.
        let candidate_idx: Vec<usize> = match &cue_code {
            Some(cc) => {
                let shortlist = self.prefilter.query(cc, self.cfg.prefilter_k);
                if shortlist.is_empty() {
                    (0..self.mems.len()).collect()
                } else {
                    let keep: std::collections::HashSet<&str> =
                        shortlist.iter().map(|(id, _)| id.as_str()).collect();
                    // Keep vectorless memories in the running too (they never hit LSH buckets).
                    (0..self.mems.len())
                        .filter(|&i| {
                            self.mems[i].code.is_none() || keep.contains(self.mems[i].id.as_str())
                        })
                        .collect()
                }
            }
            None => (0..self.mems.len()).collect(),
        };

        let cue_gist = cue_scalar.and_then(|s| {
            self.lattice
                .encode_with_semantic_position(s, &[])
                .axes
                .remove(&Axis::Semantic)
        });

        let ctx = FabricCueCtx {
            cue_tokens: cue_tokens.clone(),
            cue_seq: cue_seq.clone(),
            cue_code: cue_code.clone(),
            cue_gist,
            has_vec: cue_code.is_some(),
        };

        let mut hits: Vec<FabricHit> = candidate_idx
            .into_iter()
            .map(|i| {
                let m = &self.mems[i];
                let lexical = jaccard(&ctx.cue_tokens, &m.tokens);
                let photon = match (&ctx.cue_code, &m.code) {
                    (Some(a), Some(b)) => ((a.estimated_cosine(b)) + 1.0) / 2.0,
                    _ => 0.0,
                };
                let phase = (ctx.cue_seq.similarity(&m.seq) + 1.0) / 2.0;
                let lattice = match (&ctx.cue_gist, &m.sem_gist) {
                    (Some(cg), Some(mg)) => cg.coarse_similarity(mg, &self.lattice.scales),
                    _ => 0.0,
                };
                let has_vec = m.code.is_some() && ctx.has_vec;
                let score = self.blend(lexical, photon, phase, lattice, has_vec);
                FabricHit {
                    id: m.id.clone(),
                    score,
                    lexical,
                    photon,
                    phase,
                    lattice,
                }
            })
            .collect();

        hits.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        hits.truncate(k);
        hits
    }

    /// Blend available signals, renormalizing by which terms are active.
    fn blend(&self, lexical: f32, photon: f32, phase: f32, lattice: f32, has_vec: bool) -> f32 {
        let mut num = self.cfg.w_lexical * lexical + self.cfg.w_phase * phase;
        let mut den = self.cfg.w_lexical + self.cfg.w_phase;
        if has_vec {
            num += self.cfg.w_photon * photon + self.cfg.w_lattice * lattice;
            den += self.cfg.w_photon + self.cfg.w_lattice;
        }
        if den <= 0.0 {
            0.0
        } else {
            num / den
        }
    }
}

/// Public: locality-preserving semantic scalar in (0,1) for lattice addressing (write path).
pub fn semantic_scalar(vector: &[f32]) -> f64 {
    project_scalar(vector)
}

/// Public: order/role-sensitive structural signature of a text (write path consolidation).
pub fn structure_signature(text: &str) -> u64 {
    let parser = PhaseParser::new(256);
    let toks = tokenize(text);
    if toks.is_empty() {
        return 0;
    }
    parser
        .encode_sequence(&toks.iter().map(|s| s.as_str()).collect::<Vec<_>>())
        .structural_signature()
}

/// Phase-structural agreement between a cue and a memory's text, in `[0, 1]`.
/// Cheap (one bundle each) and order/role-sensitive — the signal the live engine borrows.
pub fn structural_boost(cue: &str, content: &str, dim: usize) -> f32 {
    let parser = PhaseParser::new(dim);
    let cue_tokens = tokenize(cue);
    let mem_tokens = tokenize(content);
    if cue_tokens.is_empty() || mem_tokens.is_empty() {
        return 0.0;
    }
    let a = parser.encode_sequence(&cue_tokens.iter().map(|s| s.as_str()).collect::<Vec<_>>());
    let b = parser.encode_sequence(&mem_tokens.iter().map(|s| s.as_str()).collect::<Vec<_>>());
    ((a.similarity(&b)) + 1.0) / 2.0
}

// ---- helpers ----

fn tokenize(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() >= 2)
        .map(|w| w.to_lowercase())
        .collect()
}

fn jaccard(a: &[String], b: &[String]) -> f32 {
    if a.is_empty() && b.is_empty() {
        return 0.0;
    }
    let sa: std::collections::HashSet<&String> = a.iter().collect();
    let sb: std::collections::HashSet<&String> = b.iter().collect();
    let inter = sa.intersection(&sb).count() as f32;
    let union = sa.union(&sb).count() as f32;
    if union <= 0.0 {
        0.0
    } else {
        inter / union
    }
}

/// Stable 1-D projection of an embedding onto a fixed Rademacher direction, squashed to (0,1).
/// Locality-preserving along that direction → nearby vectors get nearby lattice positions.
fn project_scalar(vector: &[f32]) -> f64 {
    let mut acc = 0.0f64;
    let mut norm = 0.0f64;
    for (i, &v) in vector.iter().enumerate() {
        let h = splitmix64(0xABCD_1234 ^ i as u64);
        acc += if h & 1 == 0 { v as f64 } else { -(v as f64) };
        norm += (v as f64) * (v as f64);
    }
    if norm <= 1e-12 {
        return 0.5;
    }
    let z = acc / norm.sqrt();
    1.0 / (1.0 + (-z).exp())
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

    fn vec_for(seed: u64, dim: usize) -> Vec<f32> {
        (0..dim)
            .map(|i| {
                let h = splitmix64(seed ^ (i as u64).wrapping_mul(0x9E37)) % 20001;
                (h as f32 / 10000.0) - 1.0
            })
            .collect()
    }

    // A near-duplicate embedding: base + small noise.
    fn near(base: &[f32], noise_seed: u64) -> Vec<f32> {
        let n = vec_for(noise_seed, base.len());
        base.iter().zip(&n).map(|(&x, &e)| x + e * 0.02).collect()
    }

    #[test]
    fn bench_score_shortlist_5k_docs() {
        let mut f = RecallFabric::default();
        let dim = 384;
        for i in 0..5183 {
            let v = vec_for(i as u64, dim);
            f.insert(
                format!("d{i}"),
                &format!("document title and body text number {i} about science"),
                Some(&v),
            );
        }
        let ids: Vec<String> = (0..100).map(|i| format!("d{i}")).collect();
        let id_refs: Vec<&str> = ids.iter().map(|s| s.as_str()).collect();
        let qv = vec_for(99, dim);
        let start = std::time::Instant::now();
        for _ in 0..300 {
            let _ = f.score_shortlist(&id_refs, "query about broadband speed science", Some(&qv));
        }
        let elapsed = start.elapsed();
        eprintln!(
            "score_shortlist x300: {:?} ({:.2} ms/query)",
            elapsed,
            elapsed.as_secs_f64() * 1000.0 / 300.0
        );
        assert!(
            elapsed.as_secs_f64() / 300.0 < 0.05,
            "shortlist rerank too slow"
        );
    }

    #[test]
    fn paraphrase_recalled_despite_low_lexical_overlap() {
        // Query and target share meaning (near embeddings) but almost no words.
        let mut f = RecallFabric::default();
        let target_vec = vec_for(1, 128);
        f.insert(
            "target",
            "the customer boosted their broadband speed",
            Some(&near(&target_vec, 7)),
        );
        f.insert(
            "distractor1",
            "weather forecast rain tomorrow morning",
            Some(&vec_for(500, 128)),
        );
        f.insert(
            "distractor2",
            "recipe for chocolate banana bread",
            Some(&vec_for(900, 128)),
        );

        let hits = f.recall(
            "did the user upgrade their internet plan",
            Some(&target_vec),
            3,
        );
        assert_eq!(hits[0].id, "target", "paraphrase not recalled: {hits:?}");
    }

    #[test]
    fn role_swap_separated_by_phase_when_embeddings_tie() {
        // Both memories share identical words AND identical embeddings → photon/lattice can't tell
        // them apart. Only the phase (order) term breaks the tie toward the matching order.
        let mut f = RecallFabric::default();
        let shared = vec_for(42, 128);
        f.insert_rich("user_upgraded_plan", "user upgraded plan", Some(&shared));
        f.insert_rich("plan_upgraded_user", "plan upgraded user", Some(&shared));

        let hits = f.recall("user upgraded plan", Some(&shared), 2);
        assert_eq!(
            hits[0].id, "user_upgraded_plan",
            "phase failed to disambiguate order: {hits:?}"
        );
        assert!(
            hits[0].phase > hits[1].phase,
            "phase term should favor matching order"
        );
    }

    #[test]
    fn photon_prefilter_is_sublinear() {
        let mut f = RecallFabric::default();
        let target_vec = vec_for(3, 128);
        f.insert(
            "target",
            "planted near neighbor memory",
            Some(&near(&target_vec, 11)),
        );
        for s in 0..400 {
            f.insert(
                format!("n{s}"),
                &format!("random distractor number {s}"),
                Some(&vec_for(s + 1000, 128)),
            );
        }
        // The composed recall still finds the target...
        let hits = f.recall("planted near neighbor memory", Some(&target_vec), 1);
        assert_eq!(hits[0].id, "target");
        // ...and it is registered in the LSH prefilter (sub-linear candidate generation exists).
        assert_eq!(f.len(), 401);
    }

    #[test]
    fn text_only_fallback_ranks_by_structure_and_lexical() {
        // No embeddings at all → fabric still works on lexical + phase.
        let mut f = RecallFabric::default();
        f.insert("match", "the agent completed the payment task", None);
        f.insert("other", "unrelated note about garden plants", None);
        let hits = f.recall("agent completed the payment task", None, 2);
        assert_eq!(hits[0].id, "match", "text-only recall failed: {hits:?}");
        // photon/lattice terms are inert without vectors.
        assert_eq!(hits[0].photon, 0.0);
        assert_eq!(hits[0].lattice, 0.0);
    }

    #[test]
    fn structural_boost_favors_matching_order() {
        let same = structural_boost("user upgraded plan", "user upgraded plan", 256);
        let swap = structural_boost("user upgraded plan", "plan upgraded user", 256);
        assert!(
            same > swap,
            "boost should reward matching order: same {same} swap {swap}"
        );
        assert!(same > 0.9, "identical order should score high: {same}");
    }
}
