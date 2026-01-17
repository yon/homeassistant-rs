//! Python wrappers for Automation

use ha_automation::{Automation, AutomationConfig, AutomationManager, ExecutionMode};
use pyo3::prelude::*;
use pyo3::types::PyList;
use std::sync::Arc;

use super::py_types::json_to_py;

fn mode_to_str(mode: &ExecutionMode) -> &'static str {
    match mode {
        ExecutionMode::Parallel { .. } => "parallel",
        ExecutionMode::Queued { .. } => "queued",
        ExecutionMode::Restart => "restart",
        ExecutionMode::Single => "single",
    }
}

/// Python wrapper for Automation
#[pyclass(name = "Automation")]
#[derive(Clone)]
pub struct PyAutomation {
    inner: Automation,
}

#[pymethods]
impl PyAutomation {
    #[getter]
    fn id(&self) -> &str {
        &self.inner.id
    }

    #[getter]
    fn alias(&self) -> Option<&str> {
        self.inner.alias.as_deref()
    }

    #[getter]
    fn description(&self) -> Option<&str> {
        self.inner.description.as_deref()
    }

    #[getter]
    fn enabled(&self) -> bool {
        self.inner.enabled
    }

    #[getter]
    fn mode(&self) -> &str {
        mode_to_str(&self.inner.mode)
    }

    #[getter]
    fn last_triggered(&self) -> Option<String> {
        self.inner.last_triggered.map(|dt| dt.to_rfc3339())
    }

    #[getter]
    fn current_runs(&self) -> usize {
        self.inner.current_runs
    }

    #[getter]
    fn triggers(&self, py: Python<'_>) -> PyResult<PyObject> {
        let json_val = serde_json::to_value(&self.inner.triggers)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;
        json_to_py(py, &json_val)
    }

    #[getter]
    fn conditions(&self, py: Python<'_>) -> PyResult<PyObject> {
        let json_val = serde_json::to_value(&self.inner.conditions)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;
        json_to_py(py, &json_val)
    }

    #[getter]
    fn actions(&self, py: Python<'_>) -> PyResult<PyObject> {
        let json_val = serde_json::to_value(&self.inner.actions)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;
        json_to_py(py, &json_val)
    }

    #[getter]
    fn variables(&self, py: Python<'_>) -> PyResult<PyObject> {
        json_to_py(py, &self.inner.variables)
    }

    fn can_run(&self) -> bool {
        self.inner.can_run()
    }

    fn __repr__(&self) -> String {
        format!(
            "Automation(id='{}', alias={:?}, enabled={})",
            self.inner.id, self.inner.alias, self.inner.enabled
        )
    }

    fn __eq__(&self, other: &Self) -> bool {
        self.inner.id == other.inner.id
    }

    fn __hash__(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.inner.id.hash(&mut hasher);
        hasher.finish()
    }
}

impl PyAutomation {
    pub fn from_inner(inner: Automation) -> Self {
        Self { inner }
    }

    pub fn inner(&self) -> &Automation {
        &self.inner
    }
}

/// Python wrapper for AutomationManager
#[pyclass(name = "AutomationManager")]
pub struct PyAutomationManager {
    inner: Arc<AutomationManager>,
}

#[pymethods]
impl PyAutomationManager {
    #[new]
    fn new() -> Self {
        Self {
            inner: Arc::new(AutomationManager::new()),
        }
    }

    /// Load automations from configs (list of dicts)
    fn async_load(&self, configs: &Bound<'_, PyList>) -> PyResult<()> {
        let mut parsed_configs = Vec::new();

        for item in configs.iter() {
            let json_str = item
                .call_method0("__str__")
                .and_then(|s| s.extract::<String>())
                .or_else(|_| -> PyResult<String> {
                    // Try to use json.dumps if __str__ doesn't work
                    let json_module = PyModule::import_bound(item.py(), "json")?;
                    let json_str = json_module.call_method1("dumps", (item.clone(),))?;
                    json_str.extract()
                })?;

            let config: AutomationConfig = serde_json::from_str(&json_str).map_err(|e| {
                PyErr::new::<pyo3::exceptions::PyValueError, _>(format!(
                    "Invalid automation config: {}",
                    e
                ))
            })?;
            parsed_configs.push(config);
        }

        self.inner
            .load(parsed_configs)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))
    }

    /// Reload automations
    fn async_reload(&self, configs: &Bound<'_, PyList>) -> PyResult<()> {
        let mut parsed_configs = Vec::new();

        for item in configs.iter() {
            let json_module = PyModule::import_bound(item.py(), "json")?;
            let json_str: String = json_module
                .call_method1("dumps", (item.clone(),))?
                .extract()?;

            let config: AutomationConfig = serde_json::from_str(&json_str).map_err(|e| {
                PyErr::new::<pyo3::exceptions::PyValueError, _>(format!(
                    "Invalid automation config: {}",
                    e
                ))
            })?;
            parsed_configs.push(config);
        }

        self.inner
            .reload(parsed_configs)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))
    }

    /// Get an automation by ID
    fn async_get(&self, automation_id: &str) -> Option<PyAutomation> {
        self.inner.get(automation_id).map(PyAutomation::from_inner)
    }

    /// Get all automations
    fn async_all(&self) -> Vec<PyAutomation> {
        self.inner
            .all()
            .into_iter()
            .map(PyAutomation::from_inner)
            .collect()
    }

    /// Enable an automation
    fn async_enable(&self, automation_id: &str) -> PyResult<()> {
        self.inner
            .enable(automation_id)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))
    }

    /// Disable an automation
    fn async_disable(&self, automation_id: &str) -> PyResult<()> {
        self.inner
            .disable(automation_id)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))
    }

    /// Toggle an automation
    fn async_toggle(&self, automation_id: &str) -> PyResult<bool> {
        self.inner
            .toggle(automation_id)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))
    }

    /// Remove an automation
    fn async_remove(&self, automation_id: &str) -> PyResult<PyAutomation> {
        self.inner
            .remove(automation_id)
            .map(PyAutomation::from_inner)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))
    }

    /// Mark automation as triggered
    fn mark_triggered(&self, automation_id: &str) {
        self.inner.mark_triggered(automation_id);
    }

    /// Increment run count
    fn increment_runs(&self, automation_id: &str) {
        self.inner.increment_runs(automation_id);
    }

    /// Decrement run count
    fn decrement_runs(&self, automation_id: &str) {
        self.inner.decrement_runs(automation_id);
    }

    fn __len__(&self) -> usize {
        self.inner.count()
    }

    fn __repr__(&self) -> String {
        format!("AutomationManager(count={})", self.inner.count())
    }
}

impl PyAutomationManager {
    pub fn from_arc(inner: Arc<AutomationManager>) -> Self {
        Self { inner }
    }
}
