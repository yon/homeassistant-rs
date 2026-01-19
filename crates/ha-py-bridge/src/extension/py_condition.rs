//! Python wrappers for Condition Evaluation

use ha_automation::{Condition, ConditionEvaluator, EvalContext};
use ha_state_machine::StateMachine;
use ha_template::TemplateEngine;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::sync::Arc;

use super::py_types::{json_to_py, py_to_json};

/// Python wrapper for ConditionEvaluator
#[pyclass(name = "ConditionEvaluator")]
pub struct PyConditionEvaluator {
    inner: Arc<ConditionEvaluator>,
}

#[pymethods]
impl PyConditionEvaluator {
    /// Evaluate a condition config dict
    ///
    /// Args:
    ///     condition: A dict representing the condition configuration
    ///     variables: Optional dict of template variables
    ///
    /// Returns:
    ///     bool: Whether the condition passes
    #[pyo3(signature = (condition, variables=None))]
    fn evaluate(
        &self,
        py: Python<'_>,
        condition: &Bound<'_, PyDict>,
        variables: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<bool> {
        let _ = py; // silence unused warning
                    // Convert Python dict to JSON
        let condition_json = py_to_json(condition.as_any())?;

        // Parse condition from JSON
        let condition: Condition = serde_json::from_value(condition_json).map_err(|e| {
            PyErr::new::<pyo3::exceptions::PyValueError, _>(format!(
                "Invalid condition config: {}",
                e
            ))
        })?;

        // Create eval context with variables if provided
        let mut ctx = EvalContext::new();
        if let Some(vars) = variables {
            let vars_json = py_to_json(vars.as_any())?;
            if let serde_json::Value::Object(map) = vars_json {
                for (k, v) in map {
                    ctx = ctx.with_var(k, v);
                }
            }
        }

        // Evaluate the condition
        self.inner.evaluate(&condition, &ctx).map_err(|e| {
            PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                "Condition evaluation error: {}",
                e
            ))
        })
    }

    /// Evaluate multiple conditions with AND logic
    #[pyo3(signature = (conditions, variables=None))]
    fn evaluate_all(
        &self,
        py: Python<'_>,
        conditions: Vec<Bound<'_, PyDict>>,
        variables: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<bool> {
        let _ = py; // silence unused warning
        let mut parsed_conditions = Vec::new();

        for cond in conditions {
            let condition_json = py_to_json(cond.as_any())?;
            let condition: Condition = serde_json::from_value(condition_json).map_err(|e| {
                PyErr::new::<pyo3::exceptions::PyValueError, _>(format!(
                    "Invalid condition config: {}",
                    e
                ))
            })?;
            parsed_conditions.push(condition);
        }

        let mut ctx = EvalContext::new();
        if let Some(vars) = variables {
            let vars_json = py_to_json(vars.as_any())?;
            if let serde_json::Value::Object(map) = vars_json {
                for (k, v) in map {
                    ctx = ctx.with_var(k, v);
                }
            }
        }

        self.inner
            .evaluate_all(&parsed_conditions, &ctx)
            .map_err(|e| {
                PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                    "Condition evaluation error: {}",
                    e
                ))
            })
    }

    /// Evaluate multiple conditions with OR logic
    #[pyo3(signature = (conditions, variables=None))]
    fn evaluate_any(
        &self,
        py: Python<'_>,
        conditions: Vec<Bound<'_, PyDict>>,
        variables: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<bool> {
        let _ = py; // silence unused warning
        let mut parsed_conditions = Vec::new();

        for cond in conditions {
            let condition_json = py_to_json(cond.as_any())?;
            let condition: Condition = serde_json::from_value(condition_json).map_err(|e| {
                PyErr::new::<pyo3::exceptions::PyValueError, _>(format!(
                    "Invalid condition config: {}",
                    e
                ))
            })?;
            parsed_conditions.push(condition);
        }

        let mut ctx = EvalContext::new();
        if let Some(vars) = variables {
            let vars_json = py_to_json(vars.as_any())?;
            if let serde_json::Value::Object(map) = vars_json {
                for (k, v) in map {
                    ctx = ctx.with_var(k, v);
                }
            }
        }

        self.inner
            .evaluate_any(&parsed_conditions, &ctx)
            .map_err(|e| {
                PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                    "Condition evaluation error: {}",
                    e
                ))
            })
    }

    fn __repr__(&self) -> String {
        "ConditionEvaluator()".to_string()
    }
}

impl PyConditionEvaluator {
    pub fn new(state_machine: Arc<StateMachine>, template_engine: Arc<TemplateEngine>) -> Self {
        Self {
            inner: Arc::new(ConditionEvaluator::new(state_machine, template_engine)),
        }
    }

    pub fn from_arc(inner: Arc<ConditionEvaluator>) -> Self {
        Self { inner }
    }
}

/// Python wrapper for EvalContext
#[pyclass(name = "EvalContext")]
#[derive(Clone)]
pub struct PyEvalContext {
    inner: EvalContext,
}

#[pymethods]
impl PyEvalContext {
    #[new]
    #[pyo3(signature = (variables=None, trigger_data=None))]
    fn new(
        py: Python<'_>,
        variables: Option<&Bound<'_, PyDict>>,
        trigger_data: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<Self> {
        let _ = py; // silence unused warning
        let mut ctx = EvalContext::new();

        if let Some(vars) = variables {
            let vars_json = py_to_json(vars.as_any())?;
            if let serde_json::Value::Object(map) = vars_json {
                for (k, v) in map {
                    ctx.variables.insert(k, v);
                }
            }
        }

        if let Some(trigger) = trigger_data {
            let trigger_json = py_to_json(trigger.as_any())?;
            ctx.trigger = Some(serde_json::from_value(trigger_json).map_err(|e| {
                PyErr::new::<pyo3::exceptions::PyValueError, _>(format!(
                    "Invalid trigger data: {}",
                    e
                ))
            })?);
        }

        Ok(Self { inner: ctx })
    }

    /// Get a variable value
    fn get_variable(&self, py: Python<'_>, name: &str) -> PyResult<PyObject> {
        match self.inner.variables.get(name) {
            Some(value) => json_to_py(py, value),
            None => Ok(py.None()),
        }
    }

    /// Set a variable value
    fn set_variable(&mut self, name: &str, value: &Bound<'_, pyo3::PyAny>) -> PyResult<()> {
        let json_value = py_to_json(value)?;
        self.inner.variables.insert(name.to_string(), json_value);
        Ok(())
    }

    /// Get all variables as a dict
    fn variables(&self, py: Python<'_>) -> PyResult<PyObject> {
        let json_val = serde_json::to_value(&self.inner.variables)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;
        json_to_py(py, &json_val)
    }

    fn __repr__(&self) -> String {
        format!(
            "EvalContext(variables={}, has_trigger={})",
            self.inner.variables.len(),
            self.inner.trigger.is_some()
        )
    }
}

impl PyEvalContext {
    pub fn inner(&self) -> &EvalContext {
        &self.inner
    }
}
