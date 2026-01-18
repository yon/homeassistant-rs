//! Python ConfigEntry wrapper
//!
//! Converts Rust ConfigEntry to Python dict for passing to HA integrations.

use ha_config_entries::{ConfigEntry, ConfigEntrySource, ConfigEntryState};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use pyo3::IntoPy;
use std::collections::HashMap;

use super::errors::FallbackResult;

/// Convert a serde_json::Value to a Python object
fn json_to_py(py: Python<'_>, value: &serde_json::Value) -> PyResult<PyObject> {
    match value {
        serde_json::Value::Null => Ok(py.None()),
        serde_json::Value::Bool(b) => Ok(b.into_py(py)),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(i.into_py(py))
            } else if let Some(f) = n.as_f64() {
                Ok(f.into_py(py))
            } else {
                Ok(py.None())
            }
        }
        serde_json::Value::String(s) => Ok(s.into_py(py)),
        serde_json::Value::Array(arr) => {
            let list: Vec<PyObject> = arr
                .iter()
                .map(|item| json_to_py(py, item))
                .collect::<PyResult<_>>()?;
            Ok(list.into_py(py))
        }
        serde_json::Value::Object(obj) => {
            let dict = PyDict::new_bound(py);
            for (k, v) in obj {
                dict.set_item(k, json_to_py(py, v)?)?;
            }
            Ok(dict.into_any().unbind())
        }
    }
}

/// Convert a HashMap<String, serde_json::Value> to a Python dict
fn hashmap_to_py<'py>(
    py: Python<'py>,
    map: &HashMap<String, serde_json::Value>,
) -> PyResult<Bound<'py, PyDict>> {
    let dict = PyDict::new_bound(py);
    for (k, v) in map {
        dict.set_item(k, json_to_py(py, v)?)?;
    }
    Ok(dict)
}

/// Convert ConfigEntryState to the string format HA expects
fn state_to_str(state: ConfigEntryState) -> &'static str {
    match state {
        ConfigEntryState::FailedUnload => "failed_unload",
        ConfigEntryState::Loaded => "loaded",
        ConfigEntryState::MigrationError => "migration_error",
        ConfigEntryState::NotLoaded => "not_loaded",
        ConfigEntryState::SetupError => "setup_error",
        ConfigEntryState::SetupInProgress => "setup_in_progress",
        ConfigEntryState::SetupRetry => "setup_retry",
        ConfigEntryState::UnloadInProgress => "unload_in_progress",
    }
}

/// Convert ConfigEntrySource to the string format HA expects
fn source_to_str(source: &ConfigEntrySource) -> &'static str {
    match source {
        ConfigEntrySource::Bluetooth => "bluetooth",
        ConfigEntrySource::Dhcp => "dhcp",
        ConfigEntrySource::Discovery => "discovery",
        ConfigEntrySource::Hassio => "hassio",
        ConfigEntrySource::Homekit => "homekit",
        ConfigEntrySource::Ignore => "ignore",
        ConfigEntrySource::Import => "import",
        ConfigEntrySource::Mqtt => "mqtt",
        ConfigEntrySource::Nupnp => "nupnp",
        ConfigEntrySource::Reauth => "reauth",
        ConfigEntrySource::Reconfigure => "reconfigure",
        ConfigEntrySource::Ssdp => "ssdp",
        ConfigEntrySource::System => "system",
        ConfigEntrySource::User => "user",
        ConfigEntrySource::Zeroconf => "zeroconf",
    }
}

/// Convert a Rust ConfigEntry to a Python dict matching HA's ConfigEntry
///
/// This creates a dict with all the fields that Python integrations expect
/// when calling async_setup_entry(hass, entry).
pub fn config_entry_to_python(py: Python<'_>, entry: &ConfigEntry) -> FallbackResult<PyObject> {
    let dict = PyDict::new_bound(py);

    // Required fields
    dict.set_item("entry_id", &entry.entry_id)?;
    dict.set_item("domain", &entry.domain)?;
    dict.set_item("title", &entry.title)?;
    dict.set_item("data", hashmap_to_py(py, &entry.data)?)?;
    dict.set_item("options", hashmap_to_py(py, &entry.options)?)?;

    // Version info
    dict.set_item("version", entry.version)?;
    dict.set_item("minor_version", entry.minor_version)?;

    // Optional unique_id
    match &entry.unique_id {
        Some(uid) => dict.set_item("unique_id", uid)?,
        None => dict.set_item("unique_id", py.None())?,
    }

    // Source and state
    dict.set_item("source", source_to_str(&entry.source))?;
    dict.set_item("state", state_to_str(entry.state))?;

    // Reason for failed states
    match &entry.reason {
        Some(reason) => dict.set_item("reason", reason)?,
        None => dict.set_item("reason", py.None())?,
    }

    // Preferences
    dict.set_item("pref_disable_new_entities", entry.pref_disable_new_entities)?;
    dict.set_item("pref_disable_polling", entry.pref_disable_polling)?;

    // Disabled by
    match &entry.disabled_by {
        Some(_) => dict.set_item("disabled_by", "user")?,
        None => dict.set_item("disabled_by", py.None())?,
    }

    // Discovery keys (convert to dict)
    dict.set_item("discovery_keys", hashmap_to_py(py, &entry.discovery_keys)?)?;

    Ok(dict.into_any().unbind())
}

/// Create a mock Python ConfigEntry class instance
///
/// Some integrations may expect an actual ConfigEntry class instance
/// rather than a dict. This creates a proper class instance.
pub fn create_config_entry_instance(
    py: Python<'_>,
    entry: &ConfigEntry,
) -> FallbackResult<PyObject> {
    // Import the ConfigEntry class from homeassistant.config_entries
    let config_entries_module = py.import_bound("homeassistant.config_entries")?;

    let config_entry_class = config_entries_module.getattr("ConfigEntry")?;

    // Create kwargs for ConfigEntry constructor
    let kwargs = PyDict::new_bound(py);
    kwargs.set_item("entry_id", &entry.entry_id)?;
    kwargs.set_item("domain", &entry.domain)?;
    kwargs.set_item("title", &entry.title)?;
    kwargs.set_item("data", hashmap_to_py(py, &entry.data)?)?;
    kwargs.set_item("options", hashmap_to_py(py, &entry.options)?)?;
    kwargs.set_item("version", entry.version)?;
    kwargs.set_item("minor_version", entry.minor_version)?;
    kwargs.set_item("source", source_to_str(&entry.source))?;

    match &entry.unique_id {
        Some(uid) => kwargs.set_item("unique_id", uid)?,
        None => kwargs.set_item("unique_id", py.None())?,
    }

    // Call ConfigEntry(**kwargs)
    let instance = config_entry_class.call((), Some(&kwargs))?;

    Ok(instance.unbind())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_entry_to_python() {
        pyo3::prepare_freethreaded_python();

        Python::with_gil(|py| {
            let entry = ConfigEntry::new("test_domain", "Test Title")
                .with_unique_id("unique-123")
                .with_source(ConfigEntrySource::User);

            let result = config_entry_to_python(py, &entry);
            assert!(result.is_ok());

            let py_obj = result.unwrap();
            let dict = py_obj.bind(py);
            let dict = dict.downcast::<PyDict>().unwrap();

            assert_eq!(
                dict.get_item("entry_id")
                    .unwrap()
                    .unwrap()
                    .extract::<String>()
                    .unwrap(),
                entry.entry_id
            );
            assert_eq!(
                dict.get_item("domain")
                    .unwrap()
                    .unwrap()
                    .extract::<String>()
                    .unwrap(),
                "test_domain"
            );
            assert_eq!(
                dict.get_item("title")
                    .unwrap()
                    .unwrap()
                    .extract::<String>()
                    .unwrap(),
                "Test Title"
            );
            assert_eq!(
                dict.get_item("source")
                    .unwrap()
                    .unwrap()
                    .extract::<String>()
                    .unwrap(),
                "user"
            );
        });
    }

    #[test]
    fn test_json_to_py_primitives() {
        pyo3::prepare_freethreaded_python();

        Python::with_gil(|py| {
            // Test null
            let null = json_to_py(py, &serde_json::Value::Null).unwrap();
            assert!(null.is_none(py));

            // Test bool
            let bool_val = json_to_py(py, &serde_json::json!(true)).unwrap();
            assert!(bool_val.extract::<bool>(py).unwrap());

            // Test int
            let int_val = json_to_py(py, &serde_json::json!(42)).unwrap();
            assert_eq!(int_val.extract::<i64>(py).unwrap(), 42);

            // Test string
            let str_val = json_to_py(py, &serde_json::json!("hello")).unwrap();
            assert_eq!(str_val.extract::<String>(py).unwrap(), "hello");
        });
    }
}
