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
mod pyclass_wrappers;
mod runtime;
mod service_bridge;

pub use async_bridge::{run_python_async, rust_future_to_python, AsyncBridge, PyFuture};
pub use config_entry::{config_entry_to_python, create_config_entry_instance};
pub use errors::{FallbackError, FallbackResult};
pub use hass_wrapper::{
    call_python_entity_service, create_hass_wrapper, get_python_devices, get_python_entities,
};
pub use integration::{ComponentRegistry, IntegrationLoader, IntegrationManifest};
pub use runtime::{with_gil, PythonRuntime};
pub use service_bridge::ServiceBridge;

use ha_config_entries::ConfigEntry;
use ha_event_bus::EventBus;
use ha_registries::Registries;
use ha_service_registry::ServiceRegistry;
use ha_state_machine::StateMachine;
use pyo3::prelude::*;
use std::path::Path;
use std::sync::Arc;
use tracing::info;

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
    /// Registries for device/entity registration
    pub registries: Arc<Registries>,
}

impl FallbackBridge {
    /// Create a new fallback bridge
    ///
    /// # Arguments
    /// * `ha_path` - Optional path to the Home Assistant Python installation
    /// * `registries` - Rust registries for device/entity registration
    pub fn new(ha_path: Option<&Path>, registries: Arc<Registries>) -> FallbackResult<Self> {
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
            registries,
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
            // Create Python hass wrapper with registries for device/entity registration
            let py_hass = create_hass_wrapper(
                py,
                bus.clone(),
                states.clone(),
                services,
                self.registries.clone(),
            )?;

            // Set the hass reference in config_entries for platform setup
            // This allows async_forward_entry_setups to access hass.states
            let config_entries = py_hass.bind(py).getattr("config_entries")?;
            if let Ok(set_hass) = config_entries.getattr("set_hass") {
                set_hass.call1((&py_hass,))?;
            }

            // Convert config entry to Python
            let py_entry = config_entry_to_python(py, entry)?;

            // Call setup_entry via the integration loader
            // States are now set directly via #[pyclass] StatesWrapper,
            // so no sync step is needed.
            self.integrations
                .setup_entry(domain, &py_hass, &py_entry, &self.async_bridge)
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
            // Create Python hass wrapper with registries
            let py_hass = create_hass_wrapper(py, bus, states, services, self.registries.clone())?;

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
    use tempfile::TempDir;

    fn create_test_registries() -> Arc<Registries> {
        let temp_dir = TempDir::new().unwrap();
        Arc::new(Registries::new(temp_dir.path()))
    }

    #[test]
    fn test_fallback_bridge_creation() {
        let registries = create_test_registries();
        let bridge = FallbackBridge::new(None, registries);
        assert!(bridge.is_ok());

        let bridge = bridge.unwrap();
        assert!(bridge.is_rust_component("event_bus"));
        assert!(bridge.is_python_component("hue"));
    }

    #[test]
    fn test_python_version() {
        let registries = create_test_registries();
        let bridge = FallbackBridge::new(None, registries).unwrap();
        let version = bridge.python_version().unwrap();
        assert!(version.starts_with("3."));
    }
}
