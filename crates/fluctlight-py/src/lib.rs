//! Python extension — direct library calls into FluctlightDB (no HTTP/subprocess).

use fluctlightdb::api_slim;
use fluctlightdb::{Episode, FluctlightBrain, ProvenanceKind};
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::types::PyDict;
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
        cue: &str,
        semantic_vector: Option<Vec<f32>>,
        agent_id: Option<String>,
        limit: Option<usize>,
    ) -> PyResult<Py<PyAny>> {
        let mut result =
            self.inner
                .activate_scoped(cue, semantic_vector.as_deref(), agent_id.as_deref());
        api_slim::slim_activation_for_api(&mut result, limit);
        Python::with_gil(|py| {
            let dict = PyDict::new(py);
            dict.set_item("recalls", json_val_to_py(py, &result.recalls)?)?;
            dict.set_item("active_neurons", result.active_neurons)?;
            dict.set_item("hops", result.hops)?;
            dict.set_item("myelinated", result.myelinated)?;
            Ok(dict.into())
        })
    }

    #[pyo3(signature = (batch_json, limit=None))]
    fn activate_batch_json(&self, batch_json: &str, limit: Option<usize>) -> PyResult<Py<PyAny>> {
        let items: Vec<ActivateItem> =
            serde_json::from_str(batch_json).map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        let batch: Vec<(String, Option<Vec<f32>>, Option<String>)> = items
            .into_iter()
            .map(|i| (i.cue, i.semantic_vector, i.agent_id))
            .collect();
        let mut results = self.inner.activate_batch(&batch);
        for r in &mut results {
            api_slim::slim_activation_for_api(r, limit);
        }
        Python::with_gil(|py| {
            let dict = PyDict::new(py);
            dict.set_item("results", json_val_to_py(py, &results)?)?;
            dict.set_item("count", results.len())?;
            Ok(dict.into())
        })
    }

    fn experience(&mut self, episode_json: &str) -> PyResult<Py<PyAny>> {
        self.require_writable()?;
        let episode: Episode = serde_json::from_str(episode_json)
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        let report = self
            .inner
            .experience(episode)
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        Python::with_gil(|py| json_val_to_py(py, &report))
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

    fn sleep(&mut self) -> PyResult<Py<PyAny>> {
        self.require_writable()?;
        let report = self
            .inner
            .sleep()
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        Python::with_gil(|py| json_val_to_py(py, &report))
    }

    #[pyo3(signature = (n=None))]
    fn tick(&mut self, n: Option<u64>) -> PyResult<Py<PyAny>> {
        self.require_writable()?;
        let reports = self
            .inner
            .tick_n(n.unwrap_or(1))
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        Python::with_gil(|py| json_val_to_py(py, &reports))
    }

    #[pyo3(signature = (goal, steps=None))]
    fn preplay(&self, goal: &str, steps: Option<u32>) -> PyResult<Py<PyAny>> {
        let report = self.inner.preplay(goal, steps.unwrap_or(4));
        Python::with_gil(|py| json_val_to_py(py, &report))
    }

    fn neurogenesis_pulse(&mut self) -> PyResult<Py<PyAny>> {
        self.require_writable()?;
        let report = self
            .inner
            .neurogenesis_pulse()
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        Python::with_gil(|py| json_val_to_py(py, &report))
    }

    fn compact(&mut self) -> PyResult<Py<PyAny>> {
        self.require_writable()?;
        let report = self
            .inner
            .compact()
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        Python::with_gil(|py| json_val_to_py(py, &report))
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
    fn death(&mut self, cause: Option<String>) -> PyResult<Py<PyAny>> {
        self.require_writable()?;
        let life_id = self
            .inner
            .death(cause.as_deref().unwrap_or("api"))
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        Python::with_gil(|py| json_val_to_py(py, &life_id))
    }

    fn status(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        json_val_to_py(py, &self.inner.status())
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
