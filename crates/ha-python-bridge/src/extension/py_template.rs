//! Python wrappers for TemplateEngine

use ha_template::TemplateEngine;
use pyo3::prelude::*;
use pyo3::types::PyDict;

use super::py_state_machine::PyStateMachine;
use super::py_types::py_to_json;

/// Python wrapper for TemplateEngine
#[pyclass(name = "Template")]
pub struct PyTemplate {
    template: String,
    engine: TemplateEngine,
}

#[pymethods]
impl PyTemplate {
    /// Create a new template
    ///
    /// Args:
    ///     template: The template string
    ///     state_machine: The state machine to use for state access
    #[new]
    fn new(template: String, state_machine: &PyStateMachine) -> Self {
        let engine = TemplateEngine::new(state_machine.inner().clone());
        Self { template, engine }
    }

    /// Render the template
    ///
    /// Returns:
    ///     The rendered string
    fn async_render(&self) -> PyResult<String> {
        self.engine
            .render(&self.template)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))
    }

    /// Render the template with additional variables
    ///
    /// Args:
    ///     variables: Dictionary of additional variables to make available
    ///
    /// Returns:
    ///     The rendered string
    fn async_render_with_variables(&self, variables: &Bound<'_, PyDict>) -> PyResult<String> {
        let json_vars = py_to_json(variables.as_any())?;
        self.engine
            .render_with_context(&self.template, json_vars)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))
    }

    /// Evaluate the template as an expression and return the result
    ///
    /// Returns:
    ///     The evaluated value
    fn async_evaluate(&self, py: Python<'_>) -> PyResult<PyObject> {
        let value = self
            .engine
            .evaluate(&self.template)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;

        minijinja_value_to_py(py, &value)
    }

    /// Evaluate the template with variables
    fn async_evaluate_with_variables(
        &self,
        py: Python<'_>,
        variables: &Bound<'_, PyDict>,
    ) -> PyResult<PyObject> {
        let json_vars = py_to_json(variables.as_any())?;
        let value = self
            .engine
            .evaluate_with_context(&self.template, json_vars)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;

        minijinja_value_to_py(py, &value)
    }

    /// Check if the template string contains template syntax
    ///
    /// Returns:
    ///     True if the template contains {{ }}, {% %}, or {# #}
    fn is_static(&self) -> bool {
        !TemplateEngine::is_template(&self.template)
    }

    /// Get the template string
    #[getter]
    fn template(&self) -> &str {
        &self.template
    }

    fn __repr__(&self) -> String {
        if self.template.len() > 50 {
            format!("Template('{}...')", &self.template[..50])
        } else {
            format!("Template('{}')", self.template)
        }
    }
}

/// Python wrapper for the TemplateEngine itself (for advanced usage)
#[pyclass(name = "TemplateEngine")]
pub struct PyTemplateEngine {
    inner: TemplateEngine,
}

#[pymethods]
impl PyTemplateEngine {
    /// Create a new template engine
    #[new]
    fn new(state_machine: &PyStateMachine) -> Self {
        Self {
            inner: TemplateEngine::new(state_machine.inner().clone()),
        }
    }

    /// Render a template string
    fn render(&self, template: &str) -> PyResult<String> {
        self.inner
            .render(template)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))
    }

    /// Render a template with context variables
    fn render_with_context(&self, template: &str, context: &Bound<'_, PyDict>) -> PyResult<String> {
        let json_context = py_to_json(context.as_any())?;
        self.inner
            .render_with_context(template, json_context)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))
    }

    /// Evaluate a template expression
    fn evaluate(&self, py: Python<'_>, template: &str) -> PyResult<PyObject> {
        let value = self
            .inner
            .evaluate(template)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;

        minijinja_value_to_py(py, &value)
    }

    /// Evaluate a template expression with context
    fn evaluate_with_context(
        &self,
        py: Python<'_>,
        template: &str,
        context: &Bound<'_, PyDict>,
    ) -> PyResult<PyObject> {
        let json_context = py_to_json(context.as_any())?;
        let value = self
            .inner
            .evaluate_with_context(template, json_context)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;

        minijinja_value_to_py(py, &value)
    }

    /// Check if a string contains template syntax
    #[staticmethod]
    fn is_template(template: &str) -> bool {
        TemplateEngine::is_template(template)
    }

    fn __repr__(&self) -> String {
        "TemplateEngine()".to_string()
    }
}

/// Convert a minijinja Value to a Python object
fn minijinja_value_to_py(py: Python<'_>, value: &ha_template::Value) -> PyResult<PyObject> {
    use pyo3::types::{PyBool, PyFloat, PyList, PyNone, PyString};

    if value.is_undefined() || value.is_none() {
        return Ok(PyNone::get_bound(py).into_py(py));
    }

    // Try bool first (before numeric to avoid converting true->1)
    if let Ok(b) = bool::try_from(value.clone()) {
        return Ok(PyBool::new_bound(py, b).into_py(py));
    }

    if let Some(i) = value.as_i64() {
        return Ok(i.into_py(py));
    }

    if let Ok(f) = f64::try_from(value.clone()) {
        return Ok(PyFloat::new_bound(py, f).into_py(py));
    }

    if let Some(s) = value.as_str() {
        return Ok(PyString::new_bound(py, s).into_py(py));
    }

    if let Ok(iter) = value.try_iter() {
        let mut items: Vec<PyObject> = Vec::new();
        for v in iter {
            items.push(minijinja_value_to_py(py, &v)?);
        }
        return Ok(PyList::new_bound(py, items).into_py(py));
    }

    // Default: convert to string representation
    Ok(PyString::new_bound(py, &value.to_string()).into_py(py))
}
