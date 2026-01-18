//! Python wrappers for Trigger Evaluation

use ha_automation::{Trigger, TriggerData, TriggerEvalContext, TriggerEvaluator};
use ha_core::Event;
use ha_state_machine::StateMachine;
use ha_template::TemplateEngine;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::sync::Arc;

use super::py_types::{json_to_py, py_to_json};

/// Python wrapper for TriggerEvaluator
#[pyclass(name = "TriggerEvaluator")]
pub struct PyTriggerEvaluator {
    inner: Arc<TriggerEvaluator>,
}

#[pymethods]
impl PyTriggerEvaluator {
    /// Evaluate a trigger against an event
    ///
    /// Args:
    ///     trigger: A dict representing the trigger configuration
    ///     event: A dict representing the event (with event_type, data, context)
    ///
    /// Returns:
    ///     dict or None: TriggerData if trigger matched, None otherwise
    fn evaluate(
        &self,
        py: Python<'_>,
        trigger: &Bound<'_, PyDict>,
        event: &Bound<'_, PyDict>,
    ) -> PyResult<PyObject> {
        // Convert Python dicts to JSON
        let trigger_json = py_to_json(trigger.as_any())?;
        let event_json = py_to_json(event.as_any())?;

        // Parse trigger from JSON
        let trigger: Trigger = serde_json::from_value(trigger_json).map_err(|e| {
            PyErr::new::<pyo3::exceptions::PyValueError, _>(format!(
                "Invalid trigger config: {}",
                e
            ))
        })?;

        // Parse event from JSON
        let event_type = event_json
            .get("event_type")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();
        let event_data = event_json
            .get("data")
            .cloned()
            .unwrap_or(serde_json::json!({}));
        let context = ha_core::Context::new();
        let event = Event::new(event_type, event_data, context);

        let ctx = TriggerEvalContext::new();

        // Evaluate the trigger
        match self.inner.evaluate(&trigger, &event, &ctx) {
            Ok(Some(trigger_data)) => {
                // Convert TriggerData to Python dict
                let json_val = serde_json::to_value(&trigger_data).map_err(|e| {
                    PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string())
                })?;
                json_to_py(py, &json_val)
            }
            Ok(None) => Ok(py.None()),
            Err(e) => Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                "Trigger evaluation error: {}",
                e
            ))),
        }
    }

    fn __repr__(&self) -> String {
        "TriggerEvaluator()".to_string()
    }
}

impl PyTriggerEvaluator {
    pub fn new(state_machine: Arc<StateMachine>, template_engine: Arc<TemplateEngine>) -> Self {
        Self {
            inner: Arc::new(TriggerEvaluator::new(state_machine, template_engine)),
        }
    }

    pub fn from_arc(inner: Arc<TriggerEvaluator>) -> Self {
        Self { inner }
    }
}

/// Python wrapper for TriggerData
#[pyclass(name = "TriggerData")]
#[derive(Clone)]
pub struct PyTriggerData {
    inner: TriggerData,
}

#[pymethods]
impl PyTriggerData {
    #[new]
    #[pyo3(signature = (platform, trigger_id=None))]
    fn new(platform: &str, trigger_id: Option<&str>) -> Self {
        let mut data = TriggerData::new(platform);
        if let Some(id) = trigger_id {
            data.id = Some(id.to_string());
        }
        Self { inner: data }
    }

    #[getter]
    fn platform(&self) -> &str {
        &self.inner.platform
    }

    #[getter]
    fn id(&self) -> Option<&str> {
        self.inner.id.as_deref()
    }

    #[getter]
    fn triggered_at(&self) -> String {
        self.inner.triggered_at.to_rfc3339()
    }

    /// Get trigger variables as a dict
    fn variables(&self, py: Python<'_>) -> PyResult<PyObject> {
        let json_val = serde_json::to_value(&self.inner.variables)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;
        json_to_py(py, &json_val)
    }

    /// Set a variable on the trigger data
    fn set_variable(&mut self, name: &str, value: &Bound<'_, pyo3::PyAny>) -> PyResult<()> {
        let json_value = py_to_json(value)?;
        self.inner.variables.insert(name.to_string(), json_value);
        Ok(())
    }

    /// Convert to dict
    fn as_dict(&self, py: Python<'_>) -> PyResult<PyObject> {
        let json_val = serde_json::to_value(&self.inner)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;
        json_to_py(py, &json_val)
    }

    fn __repr__(&self) -> String {
        format!(
            "TriggerData(platform='{}', id={:?})",
            self.inner.platform, self.inner.id
        )
    }
}

impl PyTriggerData {
    pub fn from_inner(inner: TriggerData) -> Self {
        Self { inner }
    }

    pub fn inner(&self) -> &TriggerData {
        &self.inner
    }
}

/// Python wrapper for TriggerEvalContext
#[pyclass(name = "TriggerEvalContext")]
#[derive(Clone)]
pub struct PyTriggerEvalContext {
    inner: TriggerEvalContext,
}

#[pymethods]
impl PyTriggerEvalContext {
    #[new]
    fn new() -> Self {
        Self {
            inner: TriggerEvalContext::new(),
        }
    }

    fn __repr__(&self) -> String {
        "TriggerEvalContext()".to_string()
    }
}

impl PyTriggerEvalContext {
    pub fn inner(&self) -> &TriggerEvalContext {
        &self.inner
    }
}
