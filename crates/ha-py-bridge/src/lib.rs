//! Home Assistant core in Rust - PyO3 bridge
//!
//! This crate provides the `homeassistant` Python module backed by Rust.
//! Python integrations can import directly from it:
//!
//! ```python
//! from homeassistant.core import HomeAssistant, State, Event
//! from homeassistant.config_entries import ConfigEntry
//! from homeassistant.helpers.entity_registry import EntityRegistry
//! from homeassistant.const import STATE_ON, STATE_OFF
//! ```
//!
//! ## Deployment Modes
//!
//! ### Mode 1: Extension (feature = "extension")
//! Build as a Python extension module that replaces the Python homeassistant package.
//!
//! ### Mode 2: Python Bridge (feature = "py_bridge")
//! Embed Python interpreter to run Python integrations from Rust.

// PyO3 macros trigger false positive clippy warnings about useless conversions
#![allow(clippy::useless_conversion)]

#[cfg(feature = "extension")]
mod extension;

#[cfg(feature = "py_bridge")]
pub mod py_bridge;

#[cfg(feature = "extension")]
use pyo3::prelude::*;

#[cfg(feature = "extension")]
use pyo3::types::PyModule;

// Re-export py_bridge types for convenience
#[cfg(feature = "py_bridge")]
pub use py_bridge::{load_allowlist_from_config, PyBridge, PyBridgeError, PyBridgeResult};

/// Python module initialization - exports as 'ha_core_rs'
///
/// This module provides Rust implementations of HA core types.
/// Types are exported at the top level for easy access:
///   - ha_core_rs.HomeAssistant, ha_core_rs.State, etc.
///   - ha_core_rs.Storage, ha_core_rs.EntityRegistry, etc.
#[cfg(feature = "extension")]
#[pymodule]
fn ha_core_rs(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    use extension::*;

    // Core types - exported at top level for conftest.py compatibility
    m.add_class::<PyHomeAssistant>()?;
    m.add_class::<PyState>()?;
    m.add_class::<PyEvent>()?;
    m.add_class::<PyContext>()?;
    m.add_class::<PyEntityId>()?;
    m.add_class::<PyEventBus>()?;
    m.add_class::<PyStateStore>()?;
    m.add_class::<PyServiceRegistry>()?;
    m.add_class::<PyUnsubscribe>()?;

    // Core functions
    m.add_function(wrap_pyfunction!(split_entity_id, m)?)?;
    m.add_function(wrap_pyfunction!(valid_entity_id, m)?)?;
    m.add_function(wrap_pyfunction!(callback, m)?)?;

    // Storage
    m.add_class::<PyStorage>()?;

    // Registries
    m.add_class::<PyEntityRegistry>()?;
    m.add_class::<PyDeviceRegistry>()?;
    m.add_class::<PyAreaRegistry>()?;
    m.add_class::<PyFloorRegistry>()?;
    m.add_class::<PyLabelRegistry>()?;

    // Registry entries (also available via helpers submodules)
    m.add_class::<PyEntityEntry>()?;
    m.add_class::<PyDeviceEntry>()?;
    m.add_class::<PyAreaEntry>()?;
    m.add_class::<PyFloorEntry>()?;
    m.add_class::<PyLabelEntry>()?;

    // Template
    m.add_class::<PyTemplate>()?;
    m.add_class::<PyTemplateEngine>()?;

    // Config Entries
    m.add_class::<PyConfigEntry>()?;
    m.add_class::<PyConfigEntries>()?;
    m.add_class::<PyConfigEntryState>()?;
    m.add(
        "InvalidStateTransition",
        py.get_type_bound::<InvalidStateTransition>(),
    )?;

    // Also create submodules for alternative import paths
    let core = PyModule::new_bound(py, "core")?;
    register_core_module(py, &core)?;
    m.add_submodule(&core)?;

    let config_entries = PyModule::new_bound(py, "config_entries")?;
    register_config_entries_module(py, &config_entries)?;
    m.add_submodule(&config_entries)?;

    let helpers = PyModule::new_bound(py, "helpers")?;
    register_helpers_module(py, &helpers)?;
    m.add_submodule(&helpers)?;

    let const_mod = PyModule::new_bound(py, "const")?;
    register_const_module(&const_mod)?;
    m.add_submodule(&const_mod)?;

    // Register submodules in sys.modules for proper imports
    let sys = py.import_bound("sys")?;
    let modules = sys.getattr("modules")?;
    modules.set_item("ha_core_rs.core", &core)?;
    modules.set_item("ha_core_rs.config_entries", &config_entries)?;
    modules.set_item("ha_core_rs.helpers", &helpers)?;
    modules.set_item("ha_core_rs.const", &const_mod)?;

    Ok(())
}

/// Register classes in the `homeassistant.core` submodule
#[cfg(feature = "extension")]
fn register_core_module(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    use extension::*;

    // Main HomeAssistant class
    m.add_class::<PyHomeAssistant>()?;

    // Core types
    m.add_class::<PyState>()?;
    m.add_class::<PyEvent>()?;
    m.add_class::<PyContext>()?;
    m.add_class::<PyEntityId>()?;

    // Core components (also accessible via hass.bus, hass.states, hass.services)
    m.add_class::<PyEventBus>()?;
    m.add_class::<PyStateStore>()?;
    m.add_class::<PyServiceRegistry>()?;

    // Helper for unsubscribing from events
    m.add_class::<PyUnsubscribe>()?;

    // Core functions
    m.add_function(wrap_pyfunction!(split_entity_id, m)?)?;
    m.add_function(wrap_pyfunction!(valid_entity_id, m)?)?;
    m.add_function(wrap_pyfunction!(callback, m)?)?;

    Ok(())
}

/// Split entity_id into domain and object_id
#[cfg(feature = "extension")]
#[pyfunction]
fn split_entity_id(entity_id: &str) -> PyResult<(String, String)> {
    let parts: Vec<&str> = entity_id.splitn(2, '.').collect();
    if parts.len() != 2 {
        return Err(pyo3::exceptions::PyValueError::new_err(format!(
            "Invalid entity ID: {}",
            entity_id
        )));
    }
    Ok((parts[0].to_string(), parts[1].to_string()))
}

/// Check if entity_id is valid
#[cfg(feature = "extension")]
#[pyfunction]
fn valid_entity_id(entity_id: &str) -> bool {
    entity_id.parse::<ha_core::EntityId>().is_ok()
}

/// Decorator to mark a function as safe to call from the event loop
#[cfg(feature = "extension")]
#[pyfunction]
fn callback(func: PyObject) -> PyObject {
    // In Python HA, @callback marks functions as safe to call synchronously
    // We just return the function as-is since our Rust impl handles this differently
    func
}

/// Register classes in the `homeassistant.config_entries` submodule
#[cfg(feature = "extension")]
fn register_config_entries_module(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    use extension::*;

    // Config entry classes
    m.add_class::<PyConfigEntry>()?;
    m.add_class::<PyConfigEntries>()?;
    m.add_class::<PyConfigEntryState>()?;

    // Exceptions
    m.add(
        "InvalidStateTransition",
        py.get_type_bound::<InvalidStateTransition>(),
    )?;

    // Source constants (matching Python HA's SOURCE_* constants)
    m.add("SOURCE_USER", "user")?;
    m.add("SOURCE_IMPORT", "import")?;
    m.add("SOURCE_DISCOVERY", "discovery")?;
    m.add("SOURCE_DHCP", "dhcp")?;
    m.add("SOURCE_SSDP", "ssdp")?;
    m.add("SOURCE_ZEROCONF", "zeroconf")?;
    m.add("SOURCE_BLUETOOTH", "bluetooth")?;
    m.add("SOURCE_MQTT", "mqtt")?;
    m.add("SOURCE_HASSIO", "hassio")?;
    m.add("SOURCE_HOMEKIT", "homekit")?;
    m.add("SOURCE_IGNORE", "ignore")?;
    m.add("SOURCE_REAUTH", "reauth")?;
    m.add("SOURCE_RECONFIGURE", "reconfigure")?;
    m.add("SOURCE_SYSTEM", "system")?;
    m.add("SOURCE_INTEGRATION_DISCOVERY", "integration_discovery")?;
    // Additional sources in Python HA
    m.add("SOURCE_USB", "usb")?;
    m.add("SOURCE_HARDWARE", "hardware")?;
    m.add("SOURCE_ESPHOME", "esphome")?;

    Ok(())
}

/// Register classes in the `homeassistant.helpers` submodule (with nested modules)
#[cfg(feature = "extension")]
fn register_helpers_module(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    use extension::*;

    // Entity registry helpers - available as ha_core_rs.entity_registry
    // The Python shim at homeassistant/helpers/entity_registry.py merges
    // these Rust classes with native HA's full module
    let entity_registry = PyModule::new_bound(py, "entity_registry")?;
    entity_registry.add_class::<PyEntityRegistry>()?;
    entity_registry.add_class::<PyEntityEntry>()?;
    m.add_submodule(&entity_registry)?;

    // Device registry helpers - available as ha_core_rs.device_registry
    let device_registry = PyModule::new_bound(py, "device_registry")?;
    device_registry.add_class::<PyDeviceRegistry>()?;
    device_registry.add_class::<PyDeviceEntry>()?;
    m.add_submodule(&device_registry)?;

    // Area registry helpers - available as ha_core_rs.area_registry
    let area_registry = PyModule::new_bound(py, "area_registry")?;
    area_registry.add_class::<PyAreaRegistry>()?;
    area_registry.add_class::<PyAreaEntry>()?;
    m.add_submodule(&area_registry)?;

    // Floor registry helpers - available as ha_core_rs.floor_registry
    let floor_registry = PyModule::new_bound(py, "floor_registry")?;
    floor_registry.add_class::<PyFloorRegistry>()?;
    floor_registry.add_class::<PyFloorEntry>()?;
    m.add_submodule(&floor_registry)?;

    // Label registry helpers - available as ha_core_rs.label_registry
    let label_registry = PyModule::new_bound(py, "label_registry")?;
    label_registry.add_class::<PyLabelRegistry>()?;
    label_registry.add_class::<PyLabelEntry>()?;
    m.add_submodule(&label_registry)?;

    // Storage helpers - available as ha_core_rs.storage
    let storage = PyModule::new_bound(py, "storage")?;
    storage.add_class::<PyStorage>()?;
    m.add_submodule(&storage)?;

    // Template helpers - available as ha_core_rs.template
    let template = PyModule::new_bound(py, "template")?;
    template.add_class::<PyTemplate>()?;
    template.add_class::<PyTemplateEngine>()?;
    m.add_submodule(&template)?;

    // Condition helpers - available as ha_core_rs.condition
    let condition = PyModule::new_bound(py, "condition")?;
    condition.add_class::<PyConditionEvaluator>()?;
    condition.add_class::<PyEvalContext>()?;
    m.add_submodule(&condition)?;

    // Trigger helpers - available as ha_core_rs.trigger
    let trigger = PyModule::new_bound(py, "trigger")?;
    trigger.add_class::<PyTriggerEvaluator>()?;
    trigger.add_class::<PyTriggerData>()?;
    trigger.add_class::<PyTriggerEvalContext>()?;
    m.add_submodule(&trigger)?;

    Ok(())
}

/// Register constants in the `homeassistant.const` submodule
#[cfg(feature = "extension")]
fn register_const_module(m: &Bound<'_, PyModule>) -> PyResult<()> {
    // State values
    m.add("STATE_ON", "on")?;
    m.add("STATE_OFF", "off")?;
    m.add("STATE_HOME", "home")?;
    m.add("STATE_NOT_HOME", "not_home")?;
    m.add("STATE_UNKNOWN", "unknown")?;
    m.add("STATE_UNAVAILABLE", "unavailable")?;
    m.add("STATE_OPEN", "open")?;
    m.add("STATE_CLOSED", "closed")?;
    m.add("STATE_LOCKED", "locked")?;
    m.add("STATE_UNLOCKED", "unlocked")?;

    // Event types
    m.add("EVENT_HOMEASSISTANT_START", "homeassistant_start")?;
    m.add("EVENT_HOMEASSISTANT_STARTED", "homeassistant_started")?;
    m.add("EVENT_HOMEASSISTANT_STOP", "homeassistant_stop")?;
    m.add(
        "EVENT_HOMEASSISTANT_FINAL_WRITE",
        "homeassistant_final_write",
    )?;
    m.add("EVENT_HOMEASSISTANT_CLOSE", "homeassistant_close")?;
    m.add("EVENT_STATE_CHANGED", "state_changed")?;
    m.add("EVENT_STATE_REPORTED", "state_reported")?;
    m.add("EVENT_CORE_CONFIG_UPDATE", "core_config_updated")?;
    m.add("EVENT_SERVICE_REGISTERED", "service_registered")?;
    m.add("EVENT_SERVICE_REMOVED", "service_removed")?;
    m.add("EVENT_CALL_SERVICE", "call_service")?;

    // Configuration keys
    m.add("CONF_HOST", "host")?;
    m.add("CONF_PORT", "port")?;
    m.add("CONF_USERNAME", "username")?;
    m.add("CONF_PASSWORD", "password")?;
    m.add("CONF_API_KEY", "api_key")?;
    m.add("CONF_ACCESS_TOKEN", "access_token")?;
    m.add("CONF_NAME", "name")?;
    m.add("CONF_ENTITY_ID", "entity_id")?;
    m.add("CONF_PLATFORM", "platform")?;
    m.add("CONF_DOMAIN", "domain")?;
    m.add("CONF_UNIQUE_ID", "unique_id")?;

    // Attributes
    m.add("ATTR_ENTITY_ID", "entity_id")?;
    m.add("ATTR_FRIENDLY_NAME", "friendly_name")?;
    m.add("ATTR_UNIT_OF_MEASUREMENT", "unit_of_measurement")?;
    m.add("ATTR_DEVICE_CLASS", "device_class")?;
    m.add("ATTR_ICON", "icon")?;
    m.add("ATTR_SUPPORTED_FEATURES", "supported_features")?;
    m.add("ATTR_STATE", "state")?;
    m.add("ATTR_NAME", "name")?;

    // Service names
    m.add("SERVICE_TURN_ON", "turn_on")?;
    m.add("SERVICE_TURN_OFF", "turn_off")?;
    m.add("SERVICE_TOGGLE", "toggle")?;
    m.add("SERVICE_RELOAD", "reload")?;

    // Misc
    m.add("MATCH_ALL", "*")?;
    m.add("DEVICE_DEFAULT_NAME", "Unnamed Device")?;

    Ok(())
}
