//! Service bridge for calling Python services from Rust
//!
//! Allows Rust code to call services registered in Python Home Assistant.

use super::async_bridge::AsyncBridge;
use super::errors::{PyBridgeError, PyBridgeResult};
use super::py_utils::{json_to_pyobject, pyobject_to_json};
use ha_core::Context;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::sync::Arc;
use tracing::info;

/// Bridge for calling Python-registered services
pub struct ServiceBridge {
    /// Reference to Python Home Assistant instance
    hass: Option<PyObject>,
    /// Async bridge for running coroutines
    async_bridge: Arc<AsyncBridge>,
}

impl ServiceBridge {
    /// Create a new service bridge
    pub fn new(async_bridge: Arc<AsyncBridge>) -> Self {
        Self {
            hass: None,
            async_bridge,
        }
    }

    /// Connect to a Python Home Assistant instance
    pub fn connect(&mut self, hass: PyObject) {
        self.hass = Some(hass);
        info!("ServiceBridge connected to Python Home Assistant");
    }

    /// Check if connected to Python HA
    pub fn is_connected(&self) -> bool {
        self.hass.is_some()
    }

    /// Call a Python service
    pub fn call_service(
        &self,
        domain: &str,
        service: &str,
        service_data: serde_json::Value,
        context: &Context,
    ) -> PyBridgeResult<Option<serde_json::Value>> {
        let hass = self
            .hass
            .as_ref()
            .ok_or_else(|| PyBridgeError::ServiceCall("Not connected to Python HA".to_string()))?;

        Python::with_gil(|py| {
            let hass_bound = hass.bind(py);

            // Get the services object
            let services = hass_bound.getattr("services")?;

            // Convert service_data to Python dict
            let py_data = json_to_pydict(py, &service_data)?;

            // Convert context to Python
            let py_context = context_to_pyobject(py, context)?;

            // Call the async_call method
            let coro =
                services.call_method1("async_call", (domain, service, py_data, py_context))?;

            // Run the coroutine
            let result = self.async_bridge.run_coroutine_py(coro.into_py(py))?;

            // Convert result back to JSON if not None
            if result.bind(py).is_none() {
                Ok(None)
            } else {
                let json_result = pyobject_to_json(result.bind(py))?;
                Ok(Some(json_result))
            }
        })
    }

    /// Check if a service exists in Python
    pub fn has_service(&self, domain: &str, service: &str) -> PyBridgeResult<bool> {
        let hass = self
            .hass
            .as_ref()
            .ok_or_else(|| PyBridgeError::ServiceCall("Not connected to Python HA".to_string()))?;

        Python::with_gil(|py| {
            let hass_bound = hass.bind(py);
            let services = hass_bound.getattr("services")?;
            let result: bool = services
                .call_method1("has_service", (domain, service))?
                .extract()?;
            Ok(result)
        })
    }

    /// Get service description from Python
    pub fn get_service_description(
        &self,
        domain: &str,
        service: &str,
    ) -> PyBridgeResult<Option<String>> {
        let hass = self
            .hass
            .as_ref()
            .ok_or_else(|| PyBridgeError::ServiceCall("Not connected to Python HA".to_string()))?;

        Python::with_gil(|py| {
            let hass_bound = hass.bind(py);
            let services = hass_bound.getattr("services")?;

            // Try to get service info
            match services.call_method1("async_get_service_description", (domain, service)) {
                Ok(coro) => {
                    let result = self.async_bridge.run_coroutine_py(coro.into_py(py))?;
                    if result.bind(py).is_none() {
                        Ok(None)
                    } else {
                        let desc: String = result.bind(py).extract()?;
                        Ok(Some(desc))
                    }
                }
                Err(_) => Ok(None),
            }
        })
    }
}

/// Convert a serde_json::Value to a Python dict
fn json_to_pydict<'py>(py: Python<'py>, value: &serde_json::Value) -> PyResult<Bound<'py, PyDict>> {
    let dict = PyDict::new_bound(py);

    if let serde_json::Value::Object(map) = value {
        for (k, v) in map {
            dict.set_item(k, json_to_pyobject(py, v)?)?;
        }
    }

    Ok(dict)
}

/// Convert a Rust Context to Python object
fn context_to_pyobject(py: Python<'_>, context: &Context) -> PyResult<PyObject> {
    let dict = PyDict::new_bound(py);
    dict.set_item("id", &context.id)?;
    if let Some(ref user_id) = context.user_id {
        dict.set_item("user_id", user_id)?;
    }
    if let Some(ref parent_id) = context.parent_id {
        dict.set_item("parent_id", parent_id)?;
    }
    Ok(dict.into_any().unbind())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn test_service_bridge_creation() {
        let bridge = AsyncBridge::new().unwrap();
        let service_bridge = ServiceBridge::new(Arc::new(bridge));
        assert!(!service_bridge.is_connected());
    }

    #[test]
    fn test_json_to_pydict() {
        Python::with_gil(|py| {
            let json = serde_json::json!({
                "key": "value",
                "number": 42,
                "nested": {"inner": true}
            });

            let dict = json_to_pydict(py, &json).unwrap();
            assert!(dict.contains("key").unwrap());
            assert!(dict.contains("number").unwrap());
            assert!(dict.contains("nested").unwrap());
        });
    }
}
