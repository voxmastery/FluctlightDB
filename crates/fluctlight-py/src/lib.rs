//! Python extension — direct library calls into FluctlightDB (no HTTP/subprocess).

use fluctlightdb::api_slim;
use fluctlightdb::FluctlightBrain;
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use serde::Deserialize;

#[pyclass(name = "Brain")]
struct PyBrain {
    inner: FluctlightBrain,
}

#[derive(Deserialize)]
struct ActivateItem {
    cue: String,
    #[serde(default)]
    semantic_vector: Option<Vec<f32>>,
    #[serde(default)]
    agent_id: Option<String>,
}

#[pymethods]
impl PyBrain {
    #[staticmethod]
    fn open_readonly(path: &str) -> PyResult<Self> {
        let inner = FluctlightBrain::open_readonly(path)
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        Ok(Self { inner })
    }

    #[staticmethod]
    fn open(path: &str) -> PyResult<Self> {
        let inner =
            FluctlightBrain::open(path).map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        Ok(Self { inner })
    }

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

    fn status(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        json_val_to_py(py, &self.inner.status())
    }

    fn verified_context(&self, py: Python<'_>, limit: Option<usize>) -> PyResult<Py<PyAny>> {
        let ctx = self.inner.verified_context(limit.unwrap_or(12));
        json_val_to_py(py, &ctx)
    }

    fn has_sidecar_index(&self) -> bool {
        self.inner.has_sidecar_index()
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
