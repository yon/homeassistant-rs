//! Translation strings loader
//!
//! Loads and caches translation strings from Home Assistant integrations.
//! Supports resolving key references like `[%key:common::config_flow::abort::no_devices_found%]`.

use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::OnceLock;
use tracing::{debug, warn};

/// Cached common strings - loaded once on first access
static COMMON_STRINGS: OnceLock<Value> = OnceLock::new();

/// Cached integration strings - domain -> parsed JSON
static INTEGRATION_STRINGS: OnceLock<HashMap<String, Value>> = OnceLock::new();

/// Find the Home Assistant core directory (parent of components)
fn find_ha_core_dir() -> Option<PathBuf> {
    // Check HA_CORE_PATH first
    if let Ok(path) = std::env::var("HA_CORE_PATH") {
        let ha_dir = PathBuf::from(&path).join("homeassistant");
        if ha_dir.is_dir() {
            return Some(ha_dir);
        }
    }

    // Try relative path from current directory (development)
    let dev_path = PathBuf::from("vendor/ha-core/homeassistant");
    if dev_path.is_dir() {
        return Some(dev_path);
    }

    // Parse PYTHONPATH
    if let Ok(pythonpath) = std::env::var("PYTHONPATH") {
        for path in pythonpath.split(':') {
            if path.contains("ha-core") || path.contains("homeassistant-core") {
                let ha_dir = PathBuf::from(path).join("homeassistant");
                if ha_dir.is_dir() {
                    return Some(ha_dir);
                }
            }
        }
    }

    None
}

/// Load the common strings.json
fn load_common_strings() -> Value {
    let Some(ha_dir) = find_ha_core_dir() else {
        warn!("Could not find Home Assistant core directory for translations");
        return Value::Object(Default::default());
    };

    let strings_path = ha_dir.join("strings.json");
    match std::fs::read_to_string(&strings_path) {
        Ok(content) => match serde_json::from_str(&content) {
            Ok(json) => {
                debug!("Loaded common strings from {:?}", strings_path);
                json
            }
            Err(e) => {
                warn!("Failed to parse common strings.json: {}", e);
                Value::Object(Default::default())
            }
        },
        Err(e) => {
            warn!("Failed to read common strings.json: {}", e);
            Value::Object(Default::default())
        }
    }
}

/// Load all integration strings.json files
fn load_all_integration_strings() -> HashMap<String, Value> {
    let mut strings = HashMap::new();

    let Some(ha_dir) = find_ha_core_dir() else {
        return strings;
    };

    let components_dir = ha_dir.join("components");
    let entries = match std::fs::read_dir(&components_dir) {
        Ok(e) => e,
        Err(_) => return strings,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let strings_path = path.join("strings.json");
        if !strings_path.exists() {
            continue;
        }

        let domain = path.file_name().unwrap().to_string_lossy().to_string();
        if let Ok(content) = std::fs::read_to_string(&strings_path) {
            if let Ok(json) = serde_json::from_str(&content) {
                strings.insert(domain, json);
            }
        }
    }

    debug!("Loaded strings for {} integrations", strings.len());
    strings
}

/// Get common strings (lazily loaded)
fn get_common_strings() -> &'static Value {
    COMMON_STRINGS.get_or_init(load_common_strings)
}

/// Get integration strings (lazily loaded)
fn get_integration_strings() -> &'static HashMap<String, Value> {
    INTEGRATION_STRINGS.get_or_init(load_all_integration_strings)
}

/// Resolve a key reference like `[%key:common::config_flow::abort::no_devices_found%]`
fn resolve_key_reference(reference: &str, common: &Value) -> Option<String> {
    // Reference format: [%key:path::to::value%]
    let inner = reference.strip_prefix("[%key:")?.strip_suffix("%]")?;

    // Split path by ::
    let parts: Vec<&str> = inner.split("::").collect();
    if parts.is_empty() {
        return None;
    }

    // Navigate the JSON
    let mut current = common;
    for part in parts {
        current = current.get(part)?;
    }

    current.as_str().map(String::from)
}

/// Resolve all key references in a string value
fn resolve_string_value(value: &str, common: &Value) -> String {
    if value.starts_with("[%key:") && value.ends_with("%]") {
        resolve_key_reference(value, common).unwrap_or_else(|| value.to_string())
    } else {
        value.to_string()
    }
}

/// Get translations for config flow
///
/// Returns translations in the format:
/// ```json
/// {
///   "resources": {
///     "component.wemo.config.abort.no_devices_found": "No devices found on the network",
///     ...
///   }
/// }
/// ```
pub fn get_config_flow_translations(integrations: &[String], _language: &str) -> Value {
    let common = get_common_strings();
    let integration_strings = get_integration_strings();
    let mut resources: HashMap<String, String> = HashMap::new();

    for domain in integrations {
        let Some(strings) = integration_strings.get(domain) else {
            continue;
        };

        // Extract config flow translations
        if let Some(config) = strings.get("config") {
            flatten_translations(
                config,
                &format!("component.{}.config", domain),
                common,
                &mut resources,
            );
        }
    }

    serde_json::json!({
        "resources": resources
    })
}

/// Flatten nested JSON into dot-notation keys
fn flatten_translations(
    value: &Value,
    prefix: &str,
    common: &Value,
    output: &mut HashMap<String, String>,
) {
    match value {
        Value::Object(map) => {
            for (key, val) in map {
                let new_prefix = format!("{}.{}", prefix, key);
                flatten_translations(val, &new_prefix, common, output);
            }
        }
        Value::String(s) => {
            let resolved = resolve_string_value(s, common);
            output.insert(prefix.to_string(), resolved);
        }
        _ => {}
    }
}

/// Get all translations for specified category and integrations
pub fn get_translations(
    category: Option<&str>,
    integrations: Option<&[String]>,
    _config_flow: bool,
    _language: &str,
) -> Value {
    let common = get_common_strings();
    let integration_strings = get_integration_strings();
    let mut resources: HashMap<String, String> = HashMap::new();

    // Determine which integrations to include
    let domains: Vec<String> = match integrations {
        Some(list) => list.to_vec(),
        None => integration_strings.keys().cloned().collect(),
    };

    for domain in &domains {
        let Some(strings) = integration_strings.get(domain) else {
            continue;
        };

        // Filter by category if specified
        match category {
            Some("config") => {
                if let Some(config) = strings.get("config") {
                    flatten_translations(
                        config,
                        &format!("component.{}.config", domain),
                        common,
                        &mut resources,
                    );
                }
            }
            Some("options") => {
                if let Some(options) = strings.get("options") {
                    flatten_translations(
                        options,
                        &format!("component.{}.options", domain),
                        common,
                        &mut resources,
                    );
                }
            }
            Some(cat) => {
                if let Some(section) = strings.get(cat) {
                    flatten_translations(
                        section,
                        &format!("component.{}.{}", domain, cat),
                        common,
                        &mut resources,
                    );
                }
            }
            None => {
                // Include all categories
                flatten_translations(
                    strings,
                    &format!("component.{}", domain),
                    common,
                    &mut resources,
                );
            }
        }
    }

    serde_json::json!({
        "resources": resources
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_key_reference() {
        let common = serde_json::json!({
            "common": {
                "config_flow": {
                    "abort": {
                        "no_devices_found": "No devices found on the network"
                    }
                }
            }
        });

        let result = resolve_key_reference(
            "[%key:common::config_flow::abort::no_devices_found%]",
            &common,
        );
        assert_eq!(result, Some("No devices found on the network".to_string()));
    }

    #[test]
    fn test_resolve_string_value() {
        let common = serde_json::json!({
            "common": {
                "config_flow": {
                    "abort": {
                        "no_devices_found": "No devices found on the network"
                    }
                }
            }
        });

        // Key reference
        let result = resolve_string_value(
            "[%key:common::config_flow::abort::no_devices_found%]",
            &common,
        );
        assert_eq!(result, "No devices found on the network");

        // Plain string
        let result = resolve_string_value("Hello world", &common);
        assert_eq!(result, "Hello world");
    }
}
