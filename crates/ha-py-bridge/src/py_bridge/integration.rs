//! Python integration loader
//!
//! Loads and manages Python Home Assistant integrations for components
//! that aren't yet implemented in Rust.

use super::errors::{PyBridgeError, PyBridgeResult};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::collections::{HashMap, HashSet};
use std::sync::{LazyLock, RwLock};
use tracing::{debug, info, warn};

/// Components implemented in Rust - NEVER load from Python
/// These would conflict with or duplicate Rust implementations.
static RUST_BLOCKLIST: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    HashSet::from([
        "automation",
        "homeassistant",
        "input_boolean",
        "input_number",
        "persistent_notification",
        "script",
        "system_log",
    ])
});

/// Tracks which components are implemented in Rust vs Python
pub struct ComponentRegistry {
    /// Components fully implemented in Rust
    rust_components: HashSet<&'static str>,
    /// Components that delegate to Python
    python_components: RwLock<HashSet<String>>,
}

impl ComponentRegistry {
    /// Create a new component registry
    pub fn new() -> Self {
        let mut rust_components = HashSet::new();
        // Core components implemented in Rust
        rust_components.insert("event_bus");
        rust_components.insert("state_machine");
        rust_components.insert("service_registry");

        Self {
            rust_components,
            python_components: RwLock::new(HashSet::new()),
        }
    }

    /// Check if a component is implemented in Rust
    pub fn is_rust_component(&self, name: &str) -> bool {
        self.rust_components.contains(name)
    }

    /// Check if a component should use Python fallback
    pub fn is_python_component(&self, name: &str) -> bool {
        !self.is_rust_component(name)
    }

    /// Register a Python component
    pub fn register_python_component(&self, name: &str) {
        let mut components = self.python_components.write().unwrap();
        components.insert(name.to_string());
    }

    /// Get all registered Python components
    pub fn python_components(&self) -> Vec<String> {
        let components = self.python_components.read().unwrap();
        components.iter().cloned().collect()
    }
}

impl Default for ComponentRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Loads Python Home Assistant integrations
pub struct IntegrationLoader {
    /// Loaded integrations by domain
    loaded: RwLock<HashMap<String, PyObject>>,
    /// Component registry
    components: ComponentRegistry,
    /// Whitelist of allowed Python integrations (empty = none allowed)
    allowlist: RwLock<HashSet<String>>,
}

impl IntegrationLoader {
    /// Create a new integration loader with an empty allowlist
    pub fn new() -> Self {
        Self {
            loaded: RwLock::new(HashMap::new()),
            components: ComponentRegistry::new(),
            allowlist: RwLock::new(HashSet::new()),
        }
    }

    /// Set the allowlist of allowed Python integrations
    ///
    /// Only integrations in this list (and not in RUST_BLOCKLIST) can be loaded.
    pub fn set_allowlist(&self, domains: Vec<String>) {
        let filtered: Vec<String> = domains
            .into_iter()
            .filter(|d| {
                if RUST_BLOCKLIST.contains(d.as_str()) {
                    warn!("Cannot allowlist '{}' - implemented in Rust, ignoring", d);
                    false
                } else {
                    true
                }
            })
            .collect();

        info!("Setting Python allowlist: {:?}", filtered);
        let mut allowlist = self.allowlist.write().unwrap();
        *allowlist = filtered.into_iter().collect();
    }

    /// Check if an integration is allowed to load via Python
    ///
    /// Returns false if:
    /// - The integration is in the RUST_BLOCKLIST (implemented in Rust)
    /// - The allowlist is empty (no Python integrations allowed)
    /// - The integration is not in the allowlist
    pub fn is_allowed(&self, domain: &str) -> bool {
        // Always block Rust-implemented components
        if RUST_BLOCKLIST.contains(domain) {
            debug!("Integration '{}' blocked - implemented in Rust", domain);
            return false;
        }

        // Check allowlist (empty = none allowed, must be explicitly listed)
        let allowlist = self.allowlist.read().unwrap();
        let allowed = !allowlist.is_empty() && allowlist.contains(domain);
        if !allowed {
            debug!("Integration '{}' blocked - not in allowlist", domain);
        }
        allowed
    }

    /// Get the current allowlist
    pub fn get_allowlist(&self) -> Vec<String> {
        let allowlist = self.allowlist.read().unwrap();
        allowlist.iter().cloned().collect()
    }

    /// Load an integration by domain
    pub fn load(&self, domain: &str) -> PyBridgeResult<()> {
        // Check if integration is allowed
        if !self.is_allowed(domain) {
            return Err(PyBridgeError::IntegrationNotAllowed(domain.to_string()));
        }

        // Check if already loaded
        {
            let loaded = self.loaded.read().unwrap();
            if loaded.contains_key(domain) {
                debug!("Integration already loaded: {}", domain);
                return Ok(());
            }
        }

        Python::with_gil(|py| {
            // Try to import the integration
            let module_name = format!("homeassistant.components.{}", domain);

            match py.import_bound(module_name.as_str()) {
                Ok(module) => {
                    info!("Loaded Python integration: {}", domain);
                    let mut loaded = self.loaded.write().unwrap();
                    loaded.insert(domain.to_string(), module.unbind().into());
                    self.components.register_python_component(domain);
                    Ok(())
                }
                Err(e) => {
                    warn!("Failed to load integration {}: {}", domain, e);
                    Err(PyBridgeError::IntegrationLoadFailed {
                        domain: domain.to_string(),
                        reason: e.to_string(),
                    })
                }
            }
        })
    }

    /// Check if an integration is loaded
    pub fn is_loaded(&self, domain: &str) -> bool {
        let loaded = self.loaded.read().unwrap();
        loaded.contains_key(domain)
    }

    /// Get the component registry
    pub fn components(&self) -> &ComponentRegistry {
        &self.components
    }

    /// Get a loaded integration module
    pub fn get(&self, domain: &str) -> Option<PyObject> {
        Python::with_gil(|py| {
            let loaded = self.loaded.read().unwrap();
            loaded.get(domain).map(|obj| obj.clone_ref(py))
        })
    }

    /// Unload an integration
    pub fn unload(&self, domain: &str) -> bool {
        let mut loaded = self.loaded.write().unwrap();
        loaded.remove(domain).is_some()
    }

    /// Get all loaded domain names
    pub fn loaded_domains(&self) -> Vec<String> {
        let loaded = self.loaded.read().unwrap();
        loaded.keys().cloned().collect()
    }

    /// Call a method on a loaded integration
    pub fn call_method(
        &self,
        domain: &str,
        method: &str,
        args: impl IntoPy<Py<pyo3::types::PyTuple>>,
    ) -> PyBridgeResult<PyObject> {
        let loaded = self.loaded.read().unwrap();
        let module = loaded
            .get(domain)
            .ok_or_else(|| PyBridgeError::IntegrationNotFound(domain.to_string()))?;

        Python::with_gil(|py| {
            let module = module.bind(py);
            let result = module.call_method1(method, args)?;
            Ok(result.unbind())
        })
    }

    /// Get an attribute from a loaded integration
    pub fn get_attr(&self, domain: &str, attr: &str) -> PyBridgeResult<PyObject> {
        let loaded = self.loaded.read().unwrap();
        let module = loaded
            .get(domain)
            .ok_or_else(|| PyBridgeError::IntegrationNotFound(domain.to_string()))?;

        Python::with_gil(|py| {
            let module = module.bind(py);
            let value = module.getattr(attr)?;
            Ok(value.unbind())
        })
    }

    /// Setup a config entry for an integration
    ///
    /// Calls the Python integration's `async_setup_entry(hass, entry)` function.
    /// Returns `Ok(true)` if setup succeeded, `Ok(false)` if the integration
    /// doesn't support config entries.
    pub fn setup_entry(
        &self,
        domain: &str,
        hass: &PyObject,
        entry: &PyObject,
        async_bridge: &super::AsyncBridge,
    ) -> PyBridgeResult<bool> {
        // Load integration if not already loaded
        self.load(domain)?;

        Python::with_gil(|py| {
            let module = self
                .get(domain)
                .ok_or_else(|| PyBridgeError::IntegrationNotFound(domain.to_string()))?;

            let module = module.bind(py);

            // Check if integration has async_setup_entry
            if !module.hasattr("async_setup_entry")? {
                debug!("Integration {} doesn't have async_setup_entry", domain);
                return Ok(false);
            }

            // Call async_setup_entry(hass, entry)
            let coro = module.call_method1("async_setup_entry", (hass, entry))?;

            // Run the coroutine to completion
            let result: bool = async_bridge.run_coroutine(coro.unbind())?;

            info!("Setup entry for integration {}: {}", domain, result);
            Ok(result)
        })
    }

    /// Unload a config entry for an integration
    ///
    /// Calls the Python integration's `async_unload_entry(hass, entry)` function.
    /// Returns `Ok(true)` if unload succeeded, `Ok(false)` if the integration
    /// doesn't support unloading.
    pub fn unload_entry(
        &self,
        domain: &str,
        hass: &PyObject,
        entry: &PyObject,
        async_bridge: &super::AsyncBridge,
    ) -> PyBridgeResult<bool> {
        // Check if integration is loaded
        if !self.is_loaded(domain) {
            return Err(PyBridgeError::IntegrationNotFound(domain.to_string()));
        }

        Python::with_gil(|py| {
            let module = self
                .get(domain)
                .ok_or_else(|| PyBridgeError::IntegrationNotFound(domain.to_string()))?;

            let module = module.bind(py);

            // Check if integration has async_unload_entry
            if !module.hasattr("async_unload_entry")? {
                debug!("Integration {} doesn't have async_unload_entry", domain);
                return Ok(false);
            }

            // Call async_unload_entry(hass, entry)
            let coro = module.call_method1("async_unload_entry", (hass, entry))?;

            // Run the coroutine to completion
            let result: bool = async_bridge.run_coroutine(coro.unbind())?;

            info!("Unload entry for integration {}: {}", domain, result);
            Ok(result)
        })
    }
}

impl Default for IntegrationLoader {
    fn default() -> Self {
        Self::new()
    }
}

/// Manifest for a Python integration
#[derive(Debug, Clone)]
pub struct IntegrationManifest {
    /// Integration domain
    pub domain: String,
    /// Human-readable name
    pub name: String,
    /// Integration version
    pub version: Option<String>,
    /// Required dependencies
    pub dependencies: Vec<String>,
    /// Required Python packages
    pub requirements: Vec<String>,
    /// Whether this is a config flow integration
    pub config_flow: bool,
}

impl IntegrationManifest {
    /// Load manifest from a Python integration
    pub fn from_domain(domain: &str) -> PyBridgeResult<Self> {
        Python::with_gil(|py| {
            let _manifest_module = format!("homeassistant.components.{}.manifest", domain);

            // Try to load manifest.json via the loader
            let loader = py.import_bound("homeassistant.loader")?;
            let manifest_dict: Bound<'_, PyDict> = loader
                .call_method1("async_get_integration", (domain,))?
                .extract()?;

            let get_str = |key: &str| -> Option<String> {
                manifest_dict
                    .get_item(key)
                    .ok()
                    .flatten()
                    .and_then(|v: Bound<'_, PyAny>| v.extract().ok())
            };

            let get_vec = |key: &str| -> Vec<String> {
                manifest_dict
                    .get_item(key)
                    .ok()
                    .flatten()
                    .and_then(|v: Bound<'_, PyAny>| v.extract().ok())
                    .unwrap_or_default()
            };

            let get_bool = |key: &str| -> bool {
                manifest_dict
                    .get_item(key)
                    .ok()
                    .flatten()
                    .and_then(|v: Bound<'_, PyAny>| v.extract().ok())
                    .unwrap_or(false)
            };

            Ok(IntegrationManifest {
                domain: domain.to_string(),
                name: get_str("name").unwrap_or_else(|| domain.to_string()),
                version: get_str("version"),
                dependencies: get_vec("dependencies"),
                requirements: get_vec("requirements"),
                config_flow: get_bool("config_flow"),
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_component_registry() {
        let registry = ComponentRegistry::new();
        assert!(registry.is_rust_component("event_bus"));
        assert!(registry.is_rust_component("state_machine"));
        assert!(!registry.is_rust_component("hue"));
        assert!(registry.is_python_component("hue"));
    }

    #[test]
    fn test_integration_loader() {
        let loader = IntegrationLoader::new();
        assert!(!loader.is_loaded("demo"));
        assert!(loader.loaded_domains().is_empty());
    }

    #[test]
    fn test_rust_blocklist_contains_expected_components() {
        // Verify that Rust-implemented components are in the blocklist
        assert!(RUST_BLOCKLIST.contains("automation"));
        assert!(RUST_BLOCKLIST.contains("homeassistant"));
        assert!(RUST_BLOCKLIST.contains("input_boolean"));
        assert!(RUST_BLOCKLIST.contains("input_number"));
        assert!(RUST_BLOCKLIST.contains("persistent_notification"));
        assert!(RUST_BLOCKLIST.contains("script"));
        assert!(RUST_BLOCKLIST.contains("system_log"));

        // Verify Python-only components are NOT in the blocklist
        assert!(!RUST_BLOCKLIST.contains("hue"));
        assert!(!RUST_BLOCKLIST.contains("sun"));
        assert!(!RUST_BLOCKLIST.contains("homekit_controller"));
    }

    #[test]
    fn test_is_allowed_empty_allowlist() {
        let loader = IntegrationLoader::new();
        // With empty allowlist, nothing should be allowed
        assert!(!loader.is_allowed("sun"));
        assert!(!loader.is_allowed("hue"));
        assert!(!loader.is_allowed("homekit_controller"));
    }

    #[test]
    fn test_is_allowed_rust_blocked_always() {
        let loader = IntegrationLoader::new();
        // Even with allowlist, Rust components should be blocked
        loader.set_allowlist(vec![
            "automation".to_string(),
            "script".to_string(),
            "system_log".to_string(),
        ]);
        assert!(!loader.is_allowed("automation"));
        assert!(!loader.is_allowed("script"));
        assert!(!loader.is_allowed("system_log"));
    }

    #[test]
    fn test_is_allowed_with_allowlist() {
        let loader = IntegrationLoader::new();
        loader.set_allowlist(vec!["sun".to_string(), "hue".to_string()]);

        // Allowed integrations should pass
        assert!(loader.is_allowed("sun"));
        assert!(loader.is_allowed("hue"));

        // Non-allowed integrations should be blocked
        assert!(!loader.is_allowed("homekit_controller"));
        assert!(!loader.is_allowed("apple_tv"));
    }

    #[test]
    fn test_get_allowlist() {
        let loader = IntegrationLoader::new();
        assert!(loader.get_allowlist().is_empty());

        loader.set_allowlist(vec!["sun".to_string(), "hue".to_string()]);
        let allowlist = loader.get_allowlist();
        assert_eq!(allowlist.len(), 2);
        assert!(allowlist.contains(&"sun".to_string()));
        assert!(allowlist.contains(&"hue".to_string()));
    }

    #[test]
    fn test_set_allowlist_filters_rust_components() {
        let loader = IntegrationLoader::new();
        // Trying to allowlist Rust components should be filtered out
        loader.set_allowlist(vec![
            "sun".to_string(),
            "automation".to_string(), // Should be filtered
            "hue".to_string(),
            "script".to_string(), // Should be filtered
        ]);

        let allowlist = loader.get_allowlist();
        assert_eq!(allowlist.len(), 2);
        assert!(allowlist.contains(&"sun".to_string()));
        assert!(allowlist.contains(&"hue".to_string()));
        assert!(!allowlist.contains(&"automation".to_string()));
        assert!(!allowlist.contains(&"script".to_string()));
    }
}
