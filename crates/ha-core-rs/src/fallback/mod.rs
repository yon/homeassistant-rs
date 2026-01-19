//! Mode 2: Python fallback for embedded interpreter
//!
//! This module provides functionality for embedding Python to delegate
//! unimplemented components to Python Home Assistant.
//!
//! ## Overview
//!
//! When running in fallback mode, the Rust application is the main process
//! and embeds a Python interpreter to run Python Home Assistant code for
//! components that aren't yet implemented in Rust.
//!
//! ## Components
//!
//! - [`PythonRuntime`] - Manages the embedded Python interpreter
//! - [`IntegrationLoader`] - Loads Python integrations dynamically
//! - [`AsyncBridge`] - Bridges Tokio and Python asyncio
//! - [`ServiceBridge`] - Calls Python-registered services from Rust
//! - [`ComponentRegistry`] - Tracks Rust vs Python component ownership
//!
//! ## Example
//!
//! ```ignore
//! use ha_core_rs::fallback::{PythonRuntime, IntegrationLoader, AsyncBridge};
//!
//! // Initialize Python runtime
//! PythonRuntime::initialize(Some(Path::new("/path/to/homeassistant")))?;
//!
//! // Load an integration
//! let loader = IntegrationLoader::new();
//! loader.load("hue")?;
//!
//! // Bridge async calls
//! let bridge = AsyncBridge::new()?;
//! let result = bridge.run_coroutine(some_python_coro)?;
//! ```

mod async_bridge;
mod config_entry;
mod errors;
mod hass_wrapper;
mod integration;
mod runtime;
mod service_bridge;

pub use async_bridge::{run_python_async, rust_future_to_python, AsyncBridge, PyFuture};
pub use config_entry::{config_entry_to_python, create_config_entry_instance};
pub use errors::{FallbackError, FallbackResult};
pub use hass_wrapper::create_hass_wrapper;
pub use integration::{ComponentRegistry, IntegrationLoader, IntegrationManifest};
pub use runtime::{with_gil, PythonRuntime};
pub use service_bridge::ServiceBridge;

use ha_config_entries::ConfigEntry;
use ha_event_bus::EventBus;
use ha_service_registry::ServiceRegistry;
use ha_state_machine::StateMachine;
use pyo3::prelude::*;
use std::path::Path;
use std::sync::Arc;
use tracing::info;

/// Convert a Python value to a serde_json::Value
fn python_to_json_value(value: &Bound<'_, pyo3::PyAny>) -> serde_json::Value {
    // Try to extract various types
    if let Ok(b) = value.extract::<bool>() {
        return serde_json::Value::Bool(b);
    }
    if let Ok(i) = value.extract::<i64>() {
        return serde_json::Value::Number(i.into());
    }
    if let Ok(f) = value.extract::<f64>() {
        if let Some(n) = serde_json::Number::from_f64(f) {
            return serde_json::Value::Number(n);
        }
    }
    if let Ok(s) = value.extract::<String>() {
        return serde_json::Value::String(s);
    }
    if let Ok(list) = value.downcast::<pyo3::types::PyList>() {
        let arr: Vec<serde_json::Value> = list
            .iter()
            .map(|item| python_to_json_value(&item))
            .collect();
        return serde_json::Value::Array(arr);
    }
    if let Ok(dict) = value.downcast::<pyo3::types::PyDict>() {
        let mut map = serde_json::Map::new();
        for (k, v) in dict.iter() {
            if let Ok(key) = k.extract::<String>() {
                map.insert(key, python_to_json_value(&v));
            }
        }
        return serde_json::Value::Object(map);
    }
    if value.is_none() {
        return serde_json::Value::Null;
    }
    // Default to string representation
    serde_json::Value::String(value.to_string())
}

/// Main entry point for fallback mode
///
/// Initializes the Python runtime and sets up the bridge infrastructure.
pub struct FallbackBridge {
    /// Python runtime
    runtime: &'static PythonRuntime,
    /// Integration loader
    pub integrations: IntegrationLoader,
    /// Async bridge
    pub async_bridge: Arc<AsyncBridge>,
    /// Service bridge
    pub services: ServiceBridge,
}

impl FallbackBridge {
    /// Create a new fallback bridge
    ///
    /// # Arguments
    /// * `ha_path` - Optional path to the Home Assistant Python installation
    pub fn new(ha_path: Option<&Path>) -> FallbackResult<Self> {
        // Initialize Python runtime
        PythonRuntime::initialize(ha_path)?;

        let runtime = PythonRuntime::get();
        let async_bridge = Arc::new(AsyncBridge::new()?);
        let services = ServiceBridge::new(async_bridge.clone());

        info!("Fallback bridge initialized");

        Ok(Self {
            runtime,
            integrations: IntegrationLoader::new(),
            async_bridge,
            services,
        })
    }

    /// Connect to a Python Home Assistant instance
    pub fn connect_hass(&mut self, hass: PyObject) {
        self.services.connect(hass);
    }

    /// Check if a component is implemented in Rust
    pub fn is_rust_component(&self, name: &str) -> bool {
        self.integrations.components().is_rust_component(name)
    }

    /// Check if a component should use Python fallback
    pub fn is_python_component(&self, name: &str) -> bool {
        self.integrations.components().is_python_component(name)
    }

    /// Load a Python integration
    pub fn load_integration(&self, domain: &str) -> FallbackResult<()> {
        self.integrations.load(domain)
    }

    /// Get Python version
    pub fn python_version(&self) -> FallbackResult<String> {
        self.runtime.python_version()
    }

    /// Execute Python code with the GIL
    pub fn with_python<F, T>(&self, f: F) -> FallbackResult<T>
    where
        F: FnOnce(Python<'_>) -> PyResult<T>,
    {
        self.runtime.exec(f)
    }

    /// Setup a config entry by calling the Python integration's async_setup_entry
    ///
    /// This method:
    /// 1. Creates a Python hass wrapper with the provided Rust components
    /// 2. Sets the hass reference for platform setup
    /// 3. Converts the config entry to a Python object
    /// 4. Calls the integration's async_setup_entry function
    /// 5. Syncs pending states from Python to Rust StateMachine
    /// 6. Returns Ok(true) if setup succeeded, Ok(false) if integration doesn't support config entries
    pub fn setup_config_entry(
        &self,
        entry: &ConfigEntry,
        bus: Arc<EventBus>,
        states: Arc<StateMachine>,
        services: Arc<ServiceRegistry>,
    ) -> FallbackResult<bool> {
        let domain = &entry.domain;

        Python::with_gil(|py| {
            // Create Python hass wrapper
            let py_hass = create_hass_wrapper(py, bus.clone(), states.clone(), services)?;

            // Set the hass reference in config_entries for platform setup
            // This allows async_forward_entry_setups to access hass.states
            let config_entries = py_hass.bind(py).getattr("config_entries")?;
            if let Ok(set_hass) = config_entries.getattr("set_hass") {
                set_hass.call1((&py_hass,))?;
            }

            // Convert config entry to Python
            let py_entry = config_entry_to_python(py, entry)?;

            // Call setup_entry via the integration loader
            let result =
                self.integrations
                    .setup_entry(domain, &py_hass, &py_entry, &self.async_bridge)?;

            // Sync pending states from Python to Rust StateMachine
            if result {
                if let Ok(states_wrapper) = py_hass.bind(py).getattr("states") {
                    if let Ok(get_pending) = states_wrapper.getattr("get_pending_states") {
                        if let Ok(pending) = get_pending.call0() {
                            if let Ok(pending_dict) = pending.downcast::<pyo3::types::PyDict>() {
                                use ha_core::{Context, EntityId};
                                use std::collections::HashMap;

                                for (entity_id, state_data) in pending_dict.iter() {
                                    if let (Ok(entity_id_str), Ok(state_dict)) = (
                                        entity_id.extract::<String>(),
                                        state_data.downcast::<pyo3::types::PyDict>(),
                                    ) {
                                        // Parse entity_id
                                        let entity_id = match entity_id_str.parse::<EntityId>() {
                                            Ok(id) => id,
                                            Err(_) => continue,
                                        };

                                        // Extract state value
                                        let state_value = state_dict
                                            .get_item("state")
                                            .ok()
                                            .flatten()
                                            .and_then(|s| s.extract::<String>().ok())
                                            .unwrap_or_else(|| "unknown".to_string());

                                        // Extract attributes
                                        let mut attrs: HashMap<String, serde_json::Value> =
                                            HashMap::new();
                                        if let Some(py_attrs) =
                                            state_dict.get_item("attributes").ok().flatten()
                                        {
                                            if let Ok(attrs_dict) =
                                                py_attrs.downcast::<pyo3::types::PyDict>()
                                            {
                                                for (key, value) in attrs_dict.iter() {
                                                    if let Ok(key_str) = key.extract::<String>() {
                                                        // Convert Python value to serde_json::Value
                                                        let json_value =
                                                            python_to_json_value(&value);
                                                        attrs.insert(key_str, json_value);
                                                    }
                                                }
                                            }
                                        }

                                        // Set state in StateMachine
                                        let context = Context::new();
                                        states.set(entity_id.clone(), &state_value, attrs, context);
                                        tracing::debug!(
                                            entity_id = %entity_id,
                                            state = %state_value,
                                            "Synced Python entity state to Rust"
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }

            Ok(result)
        })
    }

    /// Unload a config entry by calling the Python integration's async_unload_entry
    pub fn unload_config_entry(
        &self,
        entry: &ConfigEntry,
        bus: Arc<EventBus>,
        states: Arc<StateMachine>,
        services: Arc<ServiceRegistry>,
    ) -> FallbackResult<bool> {
        let domain = &entry.domain;

        Python::with_gil(|py| {
            // Create Python hass wrapper
            let py_hass = create_hass_wrapper(py, bus, states, services)?;

            // Convert config entry to Python
            let py_entry = config_entry_to_python(py, entry)?;

            // Call unload_entry via the integration loader
            self.integrations
                .unload_entry(domain, &py_hass, &py_entry, &self.async_bridge)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fallback_bridge_creation() {
        let bridge = FallbackBridge::new(None);
        assert!(bridge.is_ok());

        let bridge = bridge.unwrap();
        assert!(bridge.is_rust_component("event_bus"));
        assert!(bridge.is_python_component("hue"));
    }

    #[test]
    fn test_python_version() {
        let bridge = FallbackBridge::new(None).unwrap();
        let version = bridge.python_version().unwrap();
        assert!(version.starts_with("3."));
    }
}
