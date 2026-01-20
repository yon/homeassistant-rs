//! Mode 2: Python bridge for running Python integrations
//!
//! This module provides functionality for embedding Python to run
//! Python Home Assistant integrations from a Rust main process.
//!
//! ## Overview
//!
//! When running in py_bridge mode, the Rust application is the main process
//! and embeds a Python interpreter to run Python Home Assistant integrations.
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
//! use ha_core_rs::py_bridge::{PythonRuntime, IntegrationLoader, AsyncBridge};
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
pub use errors::{PyBridgeError, PyBridgeResult};
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
use serde::Deserialize;
use std::path::Path;
use std::sync::Arc;
use tracing::{info, warn};

/// Configuration structure for the Python integration allowlist
#[derive(Debug, Deserialize, Default)]
pub struct PythonAllowlistConfig {
    /// List of allowed Python integration domains
    #[serde(default)]
    pub integrations: Vec<String>,
}

/// Load the Python integration allowlist from a config file
///
/// Looks for `ha_python_integration_allowlist.yaml` in the given config directory.
/// Returns an empty list if the file doesn't exist or can't be parsed.
pub fn load_allowlist_from_config(config_dir: &Path) -> Vec<String> {
    let allowlist_path = config_dir.join("ha_python_integration_allowlist.yaml");

    if !allowlist_path.exists() {
        info!(
            "No Python allowlist file at {}, all Python integrations blocked",
            allowlist_path.display()
        );
        return Vec::new();
    }

    match std::fs::read_to_string(&allowlist_path) {
        Ok(content) => match serde_yaml::from_str::<PythonAllowlistConfig>(&content) {
            Ok(config) => {
                info!(
                    "Loaded Python allowlist from {}: {:?}",
                    allowlist_path.display(),
                    config.integrations
                );
                config.integrations
            }
            Err(e) => {
                warn!(
                    "Failed to parse Python allowlist {}: {}",
                    allowlist_path.display(),
                    e
                );
                Vec::new()
            }
        },
        Err(e) => {
            warn!(
                "Failed to read Python allowlist {}: {}",
                allowlist_path.display(),
                e
            );
            Vec::new()
        }
    }
}

/// Main entry point for Python bridge mode
///
/// Initializes the Python runtime and sets up the bridge infrastructure.
pub struct PyBridge {
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
    /// Config directory path (for loading registries from disk)
    pub config_dir: Option<std::path::PathBuf>,
}

impl PyBridge {
    /// Create a new Python bridge
    ///
    /// # Arguments
    /// * `ha_path` - Optional path to the Home Assistant Python installation
    /// * `registries` - Rust registries for device/entity registration
    /// * `config_dir` - Optional path to config directory (for loading registries from disk)
    pub fn new(
        ha_path: Option<&Path>,
        registries: Arc<Registries>,
        config_dir: Option<std::path::PathBuf>,
    ) -> PyBridgeResult<Self> {
        // Initialize Python runtime
        PythonRuntime::initialize(ha_path)?;

        let runtime = PythonRuntime::get();
        let async_bridge = Arc::new(AsyncBridge::new()?);
        let services = ServiceBridge::new(async_bridge.clone());

        info!("Python bridge initialized");

        Ok(Self {
            runtime,
            integrations: IntegrationLoader::new(),
            async_bridge,
            services,
            registries,
            config_dir,
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

    /// Check if a component should use Python
    pub fn is_python_component(&self, name: &str) -> bool {
        self.integrations.components().is_python_component(name)
    }

    /// Load a Python integration
    pub fn load_integration(&self, domain: &str) -> PyBridgeResult<()> {
        self.integrations.load(domain)
    }

    /// Set the allowlist of allowed Python integrations
    ///
    /// Only integrations in this allowlist can be loaded via Python.
    /// Integrations implemented in Rust are always blocked regardless of allowlist.
    pub fn set_allowlist(&self, domains: Vec<String>) {
        self.integrations.set_allowlist(domains);
    }

    /// Get the current allowlist
    pub fn get_allowlist(&self) -> Vec<String> {
        self.integrations.get_allowlist()
    }

    /// Get Python version
    pub fn python_version(&self) -> PyBridgeResult<String> {
        self.runtime.python_version()
    }

    /// Execute Python code with the GIL
    pub fn with_python<F, T>(&self, f: F) -> PyBridgeResult<T>
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
    ) -> PyBridgeResult<bool> {
        let domain = &entry.domain;

        Python::with_gil(|py| {
            // Create Python hass wrapper with registries for device/entity registration
            let py_hass = create_hass_wrapper(
                py,
                bus.clone(),
                states.clone(),
                services,
                self.registries.clone(),
                self.config_dir.as_deref(),
            )?;

            // Set the hass reference in config_entries for platform setup
            // This allows async_forward_entry_setups to access hass.states
            let config_entries = py_hass.bind(py).getattr("config_entries")?;
            if let Ok(set_hass) = config_entries.getattr("set_hass") {
                set_hass.call1((&py_hass,))?;
            }

            // Convert config entry to Python with state set to SETUP_IN_PROGRESS
            // Python's coordinator checks that state is SETUP_IN_PROGRESS during setup
            let mut entry_for_setup = entry.clone();
            entry_for_setup.state = ha_config_entries::ConfigEntryState::SetupInProgress;
            let py_entry = config_entry_to_python(py, &entry_for_setup)?;

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
    ) -> PyBridgeResult<bool> {
        let domain = &entry.domain;

        Python::with_gil(|py| {
            // Create Python hass wrapper with registries
            let py_hass = create_hass_wrapper(
                py,
                bus,
                states,
                services,
                self.registries.clone(),
                self.config_dir.as_deref(),
            )?;

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
    fn test_py_bridge_creation() {
        let registries = create_test_registries();
        let bridge = PyBridge::new(None, registries, None);
        assert!(bridge.is_ok());

        let bridge = bridge.unwrap();
        assert!(bridge.is_rust_component("event_bus"));
        assert!(bridge.is_python_component("hue"));
    }

    #[test]
    fn test_python_version() {
        let registries = create_test_registries();
        let bridge = PyBridge::new(None, registries, None).unwrap();
        let version = bridge.python_version().unwrap();
        assert!(version.starts_with("3."));
    }
}
