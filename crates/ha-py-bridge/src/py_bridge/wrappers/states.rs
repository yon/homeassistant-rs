//! StatesWrapper - wraps Rust StateStore for Python access

use super::util::{json_to_py, py_to_json};
use ha_core::{Context, EntityId};
use ha_state_store::StateStore;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
use std::collections::HashMap;
use std::sync::Arc;

/// Python wrapper for the Rust StateStore
///
/// Provides direct access to state storage without Python intermediaries.
#[pyclass(name = "StatesWrapper")]
pub struct StatesWrapper {
    states: Arc<StateStore>,
}

impl StatesWrapper {
    pub fn new(states: Arc<StateStore>) -> Self {
        Self { states }
    }
}

#[pymethods]
impl StatesWrapper {
    /// Get the state of an entity
    ///
    /// Returns a dict with 'state' and 'attributes', or None if not found.
    fn get(&self, py: Python<'_>, entity_id: &str) -> PyResult<PyObject> {
        match self.states.get(entity_id) {
            Some(state) => {
                let dict = PyDict::new_bound(py);
                dict.set_item("state", &state.state)?;
                dict.set_item("entity_id", state.entity_id.to_string())?;

                // Convert attributes to Python dict
                let attrs = PyDict::new_bound(py);
                for (key, value) in &state.attributes {
                    let py_value = json_to_py(py, value)?;
                    attrs.set_item(key, py_value)?;
                }
                dict.set_item("attributes", attrs)?;

                Ok(dict.into())
            }
            None => Ok(py.None()),
        }
    }

    /// Set the state of an entity (sync version)
    #[pyo3(signature = (entity_id, new_state, attributes=None, _force_update=None, _context=None))]
    fn set(
        &self,
        entity_id: &str,
        new_state: &str,
        attributes: Option<&Bound<'_, PyDict>>,
        _force_update: Option<bool>,
        _context: Option<PyObject>,
    ) -> PyResult<()> {
        let entity_id: EntityId = entity_id
            .parse()
            .map_err(|e| PyValueError::new_err(format!("Invalid entity_id: {}", e)))?;

        let attrs = match attributes {
            Some(dict) => {
                let mut map = HashMap::new();
                for (k, v) in dict.iter() {
                    if let Ok(key) = k.extract::<String>() {
                        map.insert(key, py_to_json(&v));
                    }
                }
                map
            }
            None => HashMap::new(),
        };

        let context = Context::new();
        self.states.set(entity_id, new_state, attrs, context);
        Ok(())
    }

    /// Set the state of an entity (async version)
    ///
    /// Note: This is actually sync since Rust StateStore operations are fast.
    /// The async interface is for compatibility with HA's API.
    #[pyo3(signature = (entity_id, new_state, attributes=None, force_update=None, context=None))]
    fn async_set<'py>(
        &self,
        py: Python<'py>,
        entity_id: &str,
        new_state: &str,
        attributes: Option<&Bound<'py, PyDict>>,
        force_update: Option<bool>,
        context: Option<PyObject>,
    ) -> PyResult<Bound<'py, PyAny>> {
        // Perform the sync operation
        self.set(entity_id, new_state, attributes, force_update, context)?;

        // Return a completed coroutine
        let asyncio = py.import_bound("asyncio")?;
        let future = asyncio.call_method0("Future")?;
        future.call_method1("set_result", (py.None(),))?;
        Ok(future)
    }

    /// Get all entity IDs
    #[pyo3(signature = (domain_filter=None))]
    fn async_entity_ids<'py>(
        &self,
        py: Python<'py>,
        domain_filter: Option<&str>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let entity_ids: Vec<String> = if let Some(domain) = domain_filter {
            self.states
                .all()
                .iter()
                .filter(|s| s.entity_id.domain() == domain)
                .map(|s| s.entity_id.to_string())
                .collect()
        } else {
            self.states
                .all()
                .iter()
                .map(|s| s.entity_id.to_string())
                .collect()
        };

        // Return as completed future
        let asyncio = py.import_bound("asyncio")?;
        let future = asyncio.call_method0("Future")?;
        let list = PyList::new_bound(py, entity_ids);
        future.call_method1("set_result", (list,))?;
        Ok(future)
    }

    /// Check if an entity_id is available (not already in use)
    ///
    /// Returns True if the entity_id is available, False if already taken.
    fn async_available(&self, entity_id: &str) -> bool {
        self.states.get(entity_id).is_none()
    }

    /// Reserve an entity_id so nothing else can use it
    ///
    /// In HA, this sets the state to STATE_UNAVAILABLE to reserve the id.
    fn async_reserve(&self, entity_id: &str) -> PyResult<()> {
        let entity_id: EntityId = entity_id
            .parse()
            .map_err(|e| PyValueError::new_err(format!("Invalid entity_id: {}", e)))?;

        // Reserve by setting to "unavailable"
        let context = Context::new();
        self.states
            .set(entity_id, "unavailable", HashMap::new(), context);
        Ok(())
    }

    /// Remove an entity's state
    #[pyo3(signature = (entity_id, _context=None))]
    fn async_remove<'py>(
        &self,
        py: Python<'py>,
        entity_id: &str,
        _context: Option<PyObject>,
    ) -> PyResult<Bound<'py, PyAny>> {
        // Try to remove the state (if it exists)
        if let Ok(eid) = entity_id.parse() {
            let context = Context::new();
            self.states.remove(&eid, context);
        }

        // Return completed future
        let asyncio = py.import_bound("asyncio")?;
        let future = asyncio.call_method0("Future")?;
        future.call_method1("set_result", (true,))?;
        Ok(future)
    }

    /// Internal set method used by Entity._async_write_ha_state
    ///
    /// This is the method that entities call to write their state.
    /// It has more parameters than async_set for internal use.
    #[pyo3(signature = (entity_id, new_state, attributes=None, force_update=None, context=None, state_info=None, timestamp=None))]
    fn async_set_internal<'py>(
        &self,
        py: Python<'py>,
        entity_id: &str,
        new_state: &str,
        attributes: Option<&Bound<'py, PyDict>>,
        force_update: Option<bool>,
        context: Option<PyObject>,
        state_info: Option<PyObject>,
        timestamp: Option<f64>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let _ = (state_info, timestamp); // Suppress unused warnings
                                         // Just delegate to async_set - internal details handled by Rust
        self.set(entity_id, new_state, attributes, force_update, context)?;

        let asyncio = py.import_bound("asyncio")?;
        let future = asyncio.call_method0("Future")?;
        future.call_method1("set_result", (py.None(),))?;
        Ok(future)
    }

    /// Get all states with optional domain filter
    #[pyo3(signature = (domain_filter=None))]
    fn async_all<'py>(
        &self,
        py: Python<'py>,
        domain_filter: Option<&str>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let states: Vec<_> = if let Some(domain) = domain_filter {
            self.states
                .all()
                .into_iter()
                .filter(|s| s.entity_id.domain() == domain)
                .collect()
        } else {
            self.states.all()
        };

        // Convert to list of dicts
        let list = PyList::empty_bound(py);
        for state in states {
            let dict = PyDict::new_bound(py);
            dict.set_item("entity_id", state.entity_id.to_string())?;
            dict.set_item("state", &state.state)?;

            let attrs = PyDict::new_bound(py);
            for (k, v) in &state.attributes {
                attrs.set_item(k, json_to_py(py, v)?)?;
            }
            dict.set_item("attributes", attrs)?;
            dict.set_item("last_changed", state.last_changed.to_rfc3339())?;
            dict.set_item("last_updated", state.last_updated.to_rfc3339())?;

            list.append(dict)?;
        }

        let asyncio = py.import_bound("asyncio")?;
        let future = asyncio.call_method0("Future")?;
        future.call_method1("set_result", (list,))?;
        Ok(future)
    }

    /// Count entities with optional domain filter
    fn async_entity_ids_count(&self, domain_filter: Option<&str>) -> usize {
        if let Some(domain) = domain_filter {
            self.states
                .all()
                .iter()
                .filter(|s| s.entity_id.domain() == domain)
                .count()
        } else {
            self.states.all().len()
        }
    }

    /// Check if entity is in specific state
    fn is_state(&self, entity_id: &str, state: &str) -> bool {
        match self.states.get(entity_id) {
            Some(s) => s.state == state,
            None => false,
        }
    }

    /// Sync version of entity_ids
    fn entity_ids(&self, domain_filter: Option<&str>) -> Vec<String> {
        if let Some(domain) = domain_filter {
            self.states
                .all()
                .iter()
                .filter(|s| s.entity_id.domain() == domain)
                .map(|s| s.entity_id.to_string())
                .collect()
        } else {
            self.states
                .all()
                .iter()
                .map(|s| s.entity_id.to_string())
                .collect()
        }
    }

    /// Sync version of all
    fn all<'py>(
        &self,
        py: Python<'py>,
        domain_filter: Option<&str>,
    ) -> PyResult<Bound<'py, PyList>> {
        let states: Vec<_> = if let Some(domain) = domain_filter {
            self.states
                .all()
                .into_iter()
                .filter(|s| s.entity_id.domain() == domain)
                .collect()
        } else {
            self.states.all()
        };

        let list = PyList::empty_bound(py);
        for state in states {
            let dict = PyDict::new_bound(py);
            dict.set_item("entity_id", state.entity_id.to_string())?;
            dict.set_item("state", &state.state)?;

            let attrs = PyDict::new_bound(py);
            for (k, v) in &state.attributes {
                attrs.set_item(k, json_to_py(py, v)?)?;
            }
            dict.set_item("attributes", attrs)?;
            list.append(dict)?;
        }
        Ok(list)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ha_event_bus::EventBus;

    #[test]
    fn test_states_wrapper() {
        pyo3::prepare_freethreaded_python();

        Python::with_gil(|py| {
            let bus = Arc::new(EventBus::new());
            let states = Arc::new(StateStore::new(bus));
            let wrapper = StatesWrapper::new(states);

            // Test set and get
            let attrs = PyDict::new_bound(py);
            attrs.set_item("friendly_name", "Test Light").unwrap();

            wrapper
                .set("light.test", "on", Some(&attrs), None, None)
                .unwrap();

            let result = wrapper.get(py, "light.test").unwrap();
            assert!(!result.is_none(py));

            let dict = result.bind(py).downcast::<PyDict>().unwrap();
            let state: String = dict.get_item("state").unwrap().unwrap().extract().unwrap();
            assert_eq!(state, "on");
        });
    }
}
