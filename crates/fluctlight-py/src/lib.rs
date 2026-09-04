//! Python extension — direct library calls into FluctlightDB (no HTTP/subprocess).

use fluctlightdb::api_slim;
use fluctlightdb::chorus_runtime::{chorus_fast_enabled, chorus_float_rerank_enabled};
use fluctlightdb::recall_router::RecallMode;
use fluctlightdb::{
    ChorusHit, ChorusRecallOpts, Episode, FluctlightBrain, ProvenanceKind, RetentionPolicy,
    ToolObserveInput,
};
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
use serde::Deserialize;
use uuid::Uuid;

#[pyclass(name = "Brain")]
struct PyBrain {
    inner: FluctlightBrain,
    readonly: bool,
}

#[derive(Deserialize)]
struct ActivateItem {
    cue: String,
    #[serde(default)]
    semantic_vector: Option<Vec<f32>>,
    #[serde(default)]
    agent_id: Option<String>,
}

impl PyBrain {
    fn require_writable(&self) -> PyResult<()> {
        if self.readonly {
            Err(PyRuntimeError::new_err(
                "brain opened readonly — use Brain.open() for writes",
            ))
        } else {
            Ok(())
        }
    }
}

fn parse_provenance_kind(kind: Option<&str>) -> ProvenanceKind {
    match kind.unwrap_or("ledger_verified") {
        "file_observation" => ProvenanceKind::FileObservation,
        "tool_grounded" => ProvenanceKind::ToolGrounded,
        "user_explicit" => ProvenanceKind::UserExplicit,
        "chat_assertion" => ProvenanceKind::ChatAssertion,
        _ => ProvenanceKind::LedgerVerified,
    }
}

#[pymethods]
impl PyBrain {
    #[staticmethod]
    fn new() -> PyResult<Self> {
        Ok(Self {
            inner: FluctlightBrain::new(),
            readonly: false,
        })
    }

    #[staticmethod]
    fn open_readonly(path: &str) -> PyResult<Self> {
        let inner = FluctlightBrain::open_readonly(path)
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        Ok(Self {
            inner,
            readonly: true,
        })
    }

    #[staticmethod]
    fn open(path: &str) -> PyResult<Self> {
        let inner =
            FluctlightBrain::open(path).map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        Ok(Self {
            inner,
            readonly: false,
        })
    }

    #[pyo3(signature = (cue, semantic_vector=None, agent_id=None, limit=None))]
    fn activate(
        &self,
        py: Python<'_>,
        cue: &str,
        semantic_vector: Option<Vec<f32>>,
        agent_id: Option<String>,
        limit: Option<usize>,
    ) -> PyResult<Py<PyAny>> {
        let top_k = limit.unwrap_or(crate::api_slim::DEFAULT_API_RECALL_LIMIT);
        let mut result =
            self.inner
                .activate_scoped(cue, semantic_vector.as_deref(), agent_id.as_deref(), top_k);
        api_slim::slim_activation_for_api(&mut result, limit);
        let dict = PyDict::new(py);
        dict.set_item("recalls", json_val_to_py(py, &result.recalls)?)?;
        dict.set_item("active_neurons", result.active_neurons)?;
        dict.set_item("hops", result.hops)?;
        dict.set_item("myelinated", result.myelinated)?;
        Ok(dict.into())
    }

    #[pyo3(signature = (batch_json, limit=None))]
    fn activate_batch_json(
        &self,
        py: Python<'_>,
        batch_json: &str,
        limit: Option<usize>,
    ) -> PyResult<Py<PyAny>> {
        let items: Vec<ActivateItem> =
            serde_json::from_str(batch_json).map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        let batch: Vec<(String, Option<Vec<f32>>, Option<String>)> = items
            .into_iter()
            .map(|i| (i.cue, i.semantic_vector, i.agent_id))
            .collect();
        let top_k = limit.unwrap_or(crate::api_slim::DEFAULT_API_RECALL_LIMIT);
        let mut results = self.inner.activate_batch(&batch, top_k);
        for r in &mut results {
            api_slim::slim_activation_for_api(r, limit);
        }
        let dict = PyDict::new(py);
        dict.set_item("results", json_val_to_py(py, &results)?)?;
        dict.set_item("count", results.len())?;
        Ok(dict.into())
    }

    fn experience(&mut self, py: Python<'_>, episode_json: &str) -> PyResult<Py<PyAny>> {
        self.require_writable()?;
        let episode: Episode = serde_json::from_str(episode_json)
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        let report = self
            .inner
            .experience(episode)
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        json_val_to_py(py, &report)
    }

    #[pyo3(signature = (engram_id, provenance_kind=None, source_uri=None, confidence=None))]
    fn verify_fact(
        &mut self,
        engram_id: &str,
        provenance_kind: Option<String>,
        source_uri: Option<String>,
        confidence: Option<f32>,
    ) -> PyResult<()> {
        self.require_writable()?;
        let id = Uuid::parse_str(engram_id)
            .map_err(|e| PyRuntimeError::new_err(format!("invalid engram_id: {e}")))?;
        let kind = parse_provenance_kind(provenance_kind.as_deref());
        self.inner
            .verify_fact(id, kind, source_uri, confidence.unwrap_or(0.95))
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))
    }

    fn sleep(&mut self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        self.require_writable()?;
        let report = self
            .inner
            .sleep()
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        json_val_to_py(py, &report)
    }

    #[pyo3(signature = (n=None))]
    fn tick(&mut self, py: Python<'_>, n: Option<u64>) -> PyResult<Py<PyAny>> {
        self.require_writable()?;
        let reports = self
            .inner
            .tick_n(n.unwrap_or(1))
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        json_val_to_py(py, &reports)
    }

    #[pyo3(signature = (goal, steps=None))]
    fn preplay(&self, py: Python<'_>, goal: &str, steps: Option<u32>) -> PyResult<Py<PyAny>> {
        let report = self.inner.preplay(goal, steps.unwrap_or(4));
        json_val_to_py(py, &report)
    }

    fn neurogenesis_pulse(&mut self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        self.require_writable()?;
        let report = self
            .inner
            .neurogenesis_pulse()
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        json_val_to_py(py, &report)
    }

    fn compact(&mut self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        self.require_writable()?;
        let report = self
            .inner
            .compact()
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        json_val_to_py(py, &report)
    }

    #[pyo3(signature = (magnitude=None))]
    fn reward(&mut self, magnitude: Option<f32>) -> PyResult<()> {
        self.require_writable()?;
        self.inner
            .reward(magnitude.unwrap_or(0.5))
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))
    }

    fn mark_core(&mut self, engram_id: &str, key: &str) -> PyResult<()> {
        self.require_writable()?;
        let id = Uuid::parse_str(engram_id)
            .map_err(|e| PyRuntimeError::new_err(format!("invalid engram_id: {e}")))?;
        self.inner
            .mark_core(id, key.to_string())
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))
    }

    #[pyo3(signature = (cause=None))]
    fn death(&mut self, py: Python<'_>, cause: Option<String>) -> PyResult<Py<PyAny>> {
        self.require_writable()?;
        let life_id = self
            .inner
            .death(cause.as_deref().unwrap_or("api"))
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        json_val_to_py(py, &life_id)
    }

    fn status(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        json_val_to_py(py, &self.inner.status())
    }

    /// Agent-lane only: budgeted prompt pack (does not change activate ranking).
    #[pyo3(signature = (cue,))]
    fn activate_for_agent_prompt(&mut self, py: Python<'_>, cue: &str) -> PyResult<Py<PyAny>> {
        json_val_to_py(py, &self.inner.activate_for_agent_prompt(cue))
    }

    /// Boot continuity without transcript paste (agent-lane).
    #[pyo3(signature = (cue=None))]
    fn session_boot_context(&mut self, py: Python<'_>, cue: Option<&str>) -> PyResult<Py<PyAny>> {
        json_val_to_py(py, &self.inner.session_boot_context(cue))
    }

    fn stage_report(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        json_val_to_py(py, &self.inner.stage_report())
    }

    #[pyo3(signature = (limit=None))]
    fn verified_context(&self, py: Python<'_>, limit: Option<usize>) -> PyResult<Py<PyAny>> {
        let ctx = self.inner.verified_context(limit.unwrap_or(12));
        json_val_to_py(py, &ctx)
    }

    fn stage(&self) -> String {
        self.inner.stage().as_str().to_string()
    }

    fn has_sidecar_index(&self) -> bool {
        self.inner.has_sidecar_index()
    }

    fn checkpoint(&mut self) -> PyResult<()> {
        self.require_writable()?;
        self.inner
            .checkpoint()
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))
    }

    #[pyo3(signature = (cue, limit=None))]
    fn complete(
        &self,
        py: Python<'_>,
        cue: &str,
        limit: Option<usize>,
    ) -> PyResult<Option<Py<PyAny>>> {
        let _ = limit;
        match self.inner.complete(cue) {
            Some(engram) => {
                let ep = &engram.episode;
                let dict = PyDict::new(py);
                dict.set_item("engram_id", engram.id.to_string())?;
                dict.set_item("content", &ep.content)?;
                dict.set_item("context", &ep.context)?;
                dict.set_item("salience", engram.salience)?;
                dict.set_item("separation_index", engram.separation_index)?;
                Ok(Some(dict.into()))
            }
            None => Ok(None),
        }
    }

    #[pyo3(signature = (cue, limit=None))]
    fn cortex_facts(&self, py: Python<'_>, cue: &str, limit: Option<usize>) -> PyResult<Py<PyAny>> {
        let facts = self.inner.cortex_facts_for_cue(cue, limit.unwrap_or(24));
        json_val_to_py(py, &facts)
    }

    #[pyo3(signature = (session_id, date, body, user_keys=None))]
    fn muon_imprint(
        &mut self,
        session_id: &str,
        date: &str,
        body: &str,
        user_keys: Option<&str>,
    ) -> PyResult<()> {
        self.require_writable()?;
        self.inner
            .muon_imprint(session_id, date, body, user_keys.unwrap_or(""));
        Ok(())
    }

    fn muon_imprint_batch_json(&mut self, batch_json: &str) -> PyResult<usize> {
        self.require_writable()?;
        let sessions: Vec<fluctlightdb::MuonImprintInput> =
            serde_json::from_str(batch_json).map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        Ok(self.inner.muon_imprint_batch(&sessions))
    }

    #[pyo3(signature = (cue, limit=None))]
    fn muon_recall(&self, py: Python<'_>, cue: &str, limit: Option<usize>) -> PyResult<Py<PyAny>> {
        let hits = self.inner.muon_recall(cue, limit.unwrap_or(8));
        json_val_to_py(py, &hits)
    }

    fn muon_len(&self) -> usize {
        self.inner.muon_len()
    }

    #[pyo3(signature = (cue, limit=None, question_type=None))]
    fn tau_recall(
        &self,
        py: Python<'_>,
        cue: &str,
        limit: Option<usize>,
        question_type: Option<&str>,
    ) -> PyResult<Py<PyAny>> {
        let hits =
            self.inner
                .tau_recall_typed(cue, limit.unwrap_or(8), question_type.unwrap_or(""));
        json_val_to_py(py, &hits)
    }

    #[pyo3(signature = (cues, limit=None, question_type=None))]
    fn tau_recall_rrf(
        &self,
        py: Python<'_>,
        cues: Vec<String>,
        limit: Option<usize>,
        question_type: Option<&str>,
    ) -> PyResult<Py<PyAny>> {
        let refs: Vec<&str> = cues.iter().map(|s| s.as_str()).collect();
        let hits =
            self.inner
                .tau_recall_rrf_typed(&refs, limit.unwrap_or(8), question_type.unwrap_or(""));
        json_val_to_py(py, &hits)
    }

    fn tau_shard_len(&self) -> usize {
        self.inner.tau_shard_len()
    }

    fn chorus_imprint_batch_json(&mut self, batch_json: &str) -> PyResult<usize> {
        self.require_writable()?;
        let batch: Vec<fluctlightdb::ChorusImprintInput> =
            serde_json::from_str(batch_json).map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        Ok(self.inner.chorus_imprint_batch(&batch))
    }

    /// Imprint traces with per-token MiniLM vectors for MaxSim late interaction.
    /// Token vectors are passed flattened (avoids GB-scale JSON); `tok_counts[i]`
    /// is the token count for trace i. The pooled photon vector is derived as the
    /// L2-normalized mean of each trace's tokens.
    #[pyo3(signature = (memory_ids, contents, contexts, tokens_flat, tok_counts, dim, salience=0.62))]
    #[allow(clippy::too_many_arguments)]
    fn chorus_imprint_maxsim_batch(
        &mut self,
        memory_ids: Vec<String>,
        contents: Vec<String>,
        contexts: Vec<String>,
        tokens_flat: Vec<f32>,
        tok_counts: Vec<usize>,
        dim: usize,
        salience: f32,
    ) -> PyResult<usize> {
        self.require_writable()?;
        let n = memory_ids.len();
        let mut batch: Vec<fluctlightdb::ChorusImprintInput> = Vec::with_capacity(n);
        let mut off = 0usize;
        for i in 0..n {
            let cnt = tok_counts.get(i).copied().unwrap_or(0);
            let mut toks: Vec<Vec<f32>> = Vec::with_capacity(cnt);
            for _ in 0..cnt {
                if dim > 0 && off + dim <= tokens_flat.len() {
                    toks.push(tokens_flat[off..off + dim].to_vec());
                    off += dim;
                }
            }
            let pooled: Option<Vec<f32>> = if toks.is_empty() {
                None
            } else {
                let mut m = vec![0.0f32; dim];
                for t in &toks {
                    for (a, b) in m.iter_mut().zip(t.iter()) {
                        *a += *b;
                    }
                }
                let norm = m.iter().map(|x| x * x).sum::<f32>().sqrt();
                if norm > 0.0 {
                    for x in m.iter_mut() {
                        *x /= norm;
                    }
                }
                Some(m)
            };
            batch.push(fluctlightdb::ChorusImprintInput {
                memory_id: memory_ids[i].clone(),
                content: contents[i].clone(),
                context: contexts.get(i).cloned().unwrap_or_default(),
                semantic_vector: pooled,
                token_vectors: Some(toks),
                salience,
                sheath: Default::default(),
            });
        }
        Ok(self.inner.chorus_imprint_batch(&batch))
    }

    #[pyo3(signature = (cue, limit=None, semantic_vector=None, fast=None, tag=None))]
    fn chorus_recall(
        &mut self,
        py: Python<'_>,
        cue: &str,
        limit: Option<usize>,
        semantic_vector: Option<Vec<f32>>,
        fast: Option<bool>,
        tag: Option<bool>,
    ) -> PyResult<Py<PyAny>> {
        let k = limit.unwrap_or(8);
        let fast = fast.unwrap_or_else(chorus_fast_enabled);
        let opts = ChorusRecallOpts {
            fast,
            float_rerank: chorus_float_rerank_enabled(),
        };
        let hits = self
            .inner
            .chorus_recall_with_opts(cue, k, semantic_vector.as_deref(), opts);
        if tag.unwrap_or(false) {
            self.inner.chorus_tag_hits(&hits);
        }
        chorus_hits_to_py(py, &hits, fast)
    }

    #[pyo3(signature = (cues, embeddings_flat, dim, limit=None, fast=None))]
    fn chorus_recall_batch_flat(
        &self,
        py: Python<'_>,
        cues: Vec<String>,
        embeddings_flat: Vec<f32>,
        dim: usize,
        limit: Option<usize>,
        fast: Option<bool>,
    ) -> PyResult<Py<PyAny>> {
        let k = limit.unwrap_or(8);
        let fast = fast.unwrap_or_else(chorus_fast_enabled);
        let opts = ChorusRecallOpts {
            fast,
            float_rerank: chorus_float_rerank_enabled(),
        };
        let flat = embeddings_flat;
        let mut queries: Vec<(&str, Option<&[f32]>)> = Vec::with_capacity(cues.len());
        for (i, cue) in cues.iter().enumerate() {
            let slice = if dim > 0 {
                let start = i * dim;
                let end = start + dim;
                if end <= flat.len() {
                    Some(flat[start..end].as_ref())
                } else {
                    None
                }
            } else {
                None
            };
            queries.push((cue.as_str(), slice));
        }
        let batch = self.inner.chorus_recall_batch(&queries, k, opts);
        chorus_batch_to_py(py, &batch, fast)
    }

    /// Late-interaction batch recall: token-population MaxSim ⊕ BM25 (RRF).
    /// Query token vectors are passed flattened; `tok_counts[i]` gives the number
    /// of tokens for query i. The pooled query vector (mean of its tokens) is
    /// derived here and used only for the photon prefilter on large stores.
    #[pyo3(signature = (cues, tokens_flat, tok_counts, dim, limit=None, w_bm=0.7))]
    #[allow(clippy::too_many_arguments)]
    fn chorus_recall_maxsim_batch(
        &self,
        py: Python<'_>,
        cues: Vec<String>,
        tokens_flat: Vec<f32>,
        tok_counts: Vec<usize>,
        dim: usize,
        limit: Option<usize>,
        w_bm: f32,
    ) -> PyResult<Py<PyAny>> {
        let k = limit.unwrap_or(150);
        let mut batch: Vec<Vec<fluctlightdb::ChorusHit>> = Vec::with_capacity(cues.len());
        let mut offset = 0usize;
        for (i, cue) in cues.iter().enumerate() {
            let cnt = tok_counts.get(i).copied().unwrap_or(0);
            let mut toks: Vec<Vec<f32>> = Vec::with_capacity(cnt);
            for _ in 0..cnt {
                if dim > 0 && offset + dim <= tokens_flat.len() {
                    toks.push(tokens_flat[offset..offset + dim].to_vec());
                    offset += dim;
                }
            }
            // pooled = L2-normalized mean of the query's token vectors
            let pooled: Option<Vec<f32>> = if toks.is_empty() {
                None
            } else {
                let mut m = vec![0.0f32; dim];
                for t in &toks {
                    for (a, b) in m.iter_mut().zip(t.iter()) {
                        *a += *b;
                    }
                }
                let norm = m.iter().map(|x| x * x).sum::<f32>().sqrt();
                if norm > 0.0 {
                    for x in m.iter_mut() {
                        *x /= norm;
                    }
                }
                Some(m)
            };
            let hits = self
                .inner
                .chorus_recall_maxsim(cue, k, &toks, pooled.as_deref(), w_bm);
            batch.push(hits);
        }
        chorus_batch_to_py(py, &batch, true)
    }

    fn chorus_sleep(&mut self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        self.require_writable()?;
        let report = self
            .inner
            .chorus_sleep()
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        json_val_to_py(py, &report)
    }

    fn chorus_tick(&mut self) -> u8 {
        self.inner.chorus_tick()
    }

    fn chorus_len(&self) -> usize {
        self.inner.chorus_len()
    }

    fn turn_begin(&mut self) {
        self.inner.turn_begin();
    }

    #[pyo3(signature = (flush=true))]
    fn turn_end(&mut self, py: Python<'_>, flush: bool) -> PyResult<Py<PyAny>> {
        self.require_writable()?;
        let report = self
            .inner
            .turn_end(flush)
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        json_val_to_py(py, &report)
    }

    #[pyo3(signature = (content, context="turn", salience=0.6, semantic_vector=None))]
    fn wm_push(
        &mut self,
        content: &str,
        context: &str,
        salience: f32,
        semantic_vector: Option<Vec<f32>>,
    ) {
        self.inner
            .wm_push(content, context, salience, semantic_vector);
    }

    fn wm_len(&self) -> usize {
        self.inner.wm_len()
    }

    fn observe_tool_json(&mut self, py: Python<'_>, payload_json: &str) -> PyResult<Py<PyAny>> {
        self.require_writable()?;
        let input: ToolObserveInput = serde_json::from_str(payload_json)
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        let out = self
            .inner
            .observe_tool(&input)
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        json_val_to_py(py, &out)
    }

    #[pyo3(signature = (cue, mode="auto", limit=None, semantic_vector=None, tick_from=None, tick_to=None))]
    fn recall_unified(
        &self,
        py: Python<'_>,
        cue: &str,
        mode: &str,
        limit: Option<usize>,
        semantic_vector: Option<Vec<f32>>,
        tick_from: Option<u64>,
        tick_to: Option<u64>,
    ) -> PyResult<Py<PyAny>> {
        let k = limit.unwrap_or(8);
        let mode = parse_recall_mode(mode);
        let temporal = if tick_from.is_some() || tick_to.is_some() {
            Some(fluctlightdb::TemporalFilter {
                from_tick: tick_from,
                to_tick: tick_to,
            })
        } else {
            None
        };
        let out = self
            .inner
            .recall_unified(cue, semantic_vector.as_deref(), mode, k, temporal);
        json_val_to_py(py, &out)
    }

    #[pyo3(signature = (cue, semantic_vector=None))]
    fn resolve(
        &self,
        py: Python<'_>,
        cue: &str,
        semantic_vector: Option<Vec<f32>>,
    ) -> PyResult<Py<PyAny>> {
        let out = self.inner.resolve(cue, semantic_vector.as_deref());
        json_val_to_py(py, &out)
    }

    #[pyo3(signature = (days=None, unless_verified=true, min_salience=None))]
    fn retain_for(
        &mut self,
        days: Option<u32>,
        unless_verified: bool,
        min_salience: Option<f32>,
    ) -> PyResult<()> {
        self.require_writable()?;
        let mut policy = self.inner.retention_policy().clone();
        policy.retain_days = days;
        policy.unless_verified = unless_verified;
        if let Some(s) = min_salience {
            policy.min_salience = s;
        }
        self.inner.set_retention_policy(policy);
        Ok(())
    }

    fn consolidate(&mut self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        self.require_writable()?;
        let report = self
            .inner
            .consolidate()
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        json_val_to_py(py, &report)
    }

    fn set_auto_consolidate(&mut self, enabled: bool) -> PyResult<()> {
        self.require_writable()?;
        self.inner.agent.auto_consolidate = enabled;
        Ok(())
    }

    fn query_json(&self, py: Python<'_>, payload_json: &str) -> PyResult<Py<PyAny>> {
        let req: fluctlightdb::query::QueryRequest = serde_json::from_str(payload_json)
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        let resp = fluctlightdb::query::execute(&self.inner, req);
        json_val_to_py(py, &resp)
    }

    fn query_mut_json(&mut self, py: Python<'_>, payload_json: &str) -> PyResult<Py<PyAny>> {
        self.require_writable()?;
        let req: fluctlightdb::query::QueryRequest = serde_json::from_str(payload_json)
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        let resp = fluctlightdb::query::execute_mut(&mut self.inner, req);
        json_val_to_py(py, &resp)
    }

    fn export_snapshot_json(&self, py: Python<'_>) -> PyResult<String> {
        fluctlightdb::export_snapshot_json(&self.inner)
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))
    }

    fn import_snapshot_json(&mut self, py: Python<'_>, json: &str) -> PyResult<Py<PyAny>> {
        self.require_writable()?;
        let report = fluctlightdb::import_snapshot_json(&mut self.inner, json)
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        json_val_to_py(py, &report)
    }

    fn scrub_pii(&mut self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        self.require_writable()?;
        let report = self
            .inner
            .scrub_pii()
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        json_val_to_py(py, &report)
    }

    fn delete_by_subject(&mut self, py: Python<'_>, subject: &str) -> PyResult<Py<PyAny>> {
        self.require_writable()?;
        let report = self
            .inner
            .delete_by_subject(subject)
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        json_val_to_py(py, &report)
    }

    fn delete_by_agent_id(&mut self, py: Python<'_>, agent_id: &str) -> PyResult<Py<PyAny>> {
        self.require_writable()?;
        let n = self
            .inner
            .delete_by_agent_id(agent_id)
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        Ok(n.into_pyobject(py)?.into())
    }

    fn audit_log_json(&self, py: Python<'_>, limit: Option<usize>) -> PyResult<Py<PyAny>> {
        let entries = self.inner.audit_log(limit.unwrap_or(50));
        json_val_to_py(py, &entries)
    }

    fn graph_export_json(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let g = self.inner.export_graph();
        json_val_to_py(py, &g)
    }

    #[staticmethod]
    fn replicate_sync(_primary: &str, _replica: &str, _py: Python<'_>) -> PyResult<Py<PyAny>> {
        Err(PyRuntimeError::new_err(
            "filesystem-copy replication is quarantined; use distributed mTLS tenant replication",
        ))
    }

    fn tau_crystallize_shard(&mut self, shard_id: &str) -> PyResult<String> {
        self.require_writable()?;
        let id = self
            .inner
            .tau_crystallize_shard(shard_id)
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        Ok(id.to_string())
    }
}

fn parse_recall_mode(mode: &str) -> RecallMode {
    match mode.to_lowercase().as_str() {
        "episodic" => RecallMode::Episodic,
        "corpus" | "chorus" => RecallMode::Corpus,
        "session" | "muon" => RecallMode::Session,
        "hybrid" => RecallMode::Hybrid,
        _ => RecallMode::Auto,
    }
}

fn chorus_hits_to_py(py: Python<'_>, hits: &[ChorusHit], fast: bool) -> PyResult<Py<PyAny>> {
    let list = PyList::empty(py);
    if fast {
        for hit in hits {
            list.append((hit.memory_id.as_str(), hit.score))?;
        }
    } else {
        for hit in hits {
            let dict = PyDict::new(py);
            dict.set_item("memory_id", &hit.memory_id)?;
            dict.set_item("score", hit.score)?;
            dict.set_item("photon", hit.photon)?;
            dict.set_item("field", hit.field)?;
            dict.set_item("lexical", hit.lexical)?;
            dict.set_item("theta", hit.theta)?;
            dict.set_item("lane", &hit.lane)?;
            if !hit.snippet.is_empty() {
                dict.set_item("snippet", &hit.snippet)?;
            }
            list.append(dict)?;
        }
    }
    Ok(list.into())
}

fn chorus_batch_to_py(py: Python<'_>, batch: &[Vec<ChorusHit>], fast: bool) -> PyResult<Py<PyAny>> {
    let outer = PyList::empty(py);
    for hits in batch {
        if fast {
            let list = PyList::empty(py);
            for hit in hits {
                list.append((hit.memory_id.as_str(), hit.score))?;
            }
            outer.append(list)?;
        } else {
            let inner = chorus_hits_to_py(py, hits, false)?;
            outer.append(inner)?;
        }
    }
    Ok(outer.into())
}

fn json_val_to_py<T: serde::Serialize>(py: Python<'_>, val: &T) -> PyResult<Py<PyAny>> {
    let json = serde_json::to_value(val).map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
    let s = serde_json::to_string(&json).map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
    Ok(py.import("json")?.call_method1("loads", (s,))?.into())
}

#[pymodule]
fn fluctlightdb_native(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyBrain>()?;
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    Ok(())
}
