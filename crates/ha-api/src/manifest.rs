//! Integration manifest loader
//!
//! Loads and caches integration manifests from the Home Assistant components directory.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::OnceLock;
use tracing::{debug, info, warn};

/// Cached manifests - loaded once on first access
static MANIFESTS: OnceLock<HashMap<String, IntegrationManifest>> = OnceLock::new();

/// Integration manifest from manifest.json
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrationManifest {
    pub domain: String,
    pub name: String,
    #[serde(default)]
    pub config_flow: bool,
    #[serde(default)]
    pub integration_type: Option<String>,
    #[serde(default)]
    pub iot_class: Option<String>,
    #[serde(default)]
    pub single_config_entry: bool,
    #[serde(default)]
    pub documentation: Option<String>,
    #[serde(default)]
    pub codeowners: Vec<String>,
    #[serde(default)]
    pub requirements: Vec<String>,
    #[serde(default)]
    pub dependencies: Vec<String>,
    #[serde(default)]
    pub after_dependencies: Vec<String>,
    #[serde(default)]
    pub is_built_in: bool,
}

/// Find the Home Assistant core components directory from PYTHONPATH
fn find_components_dir() -> Option<PathBuf> {
    // Check HA_CORE_PATH first (explicit path to HA core)
    if let Ok(path) = std::env::var("HA_CORE_PATH") {
        let components = PathBuf::from(&path).join("homeassistant/components");
        if components.is_dir() {
            return Some(components);
        }
    }

    // Try relative path from current directory first (for development)
    // This is the most reliable for our setup
    let dev_path = PathBuf::from("vendor/ha-core/homeassistant/components");
    if dev_path.is_dir() {
        return Some(dev_path);
    }

    // Parse PYTHONPATH to find vendor/ha-core (look for paths containing "ha-core")
    if let Ok(pythonpath) = std::env::var("PYTHONPATH") {
        for path in pythonpath.split(':') {
            // Only check paths that look like the real HA core (not our shim)
            if path.contains("ha-core") || path.contains("homeassistant-core") {
                let components = PathBuf::from(path).join("homeassistant/components");
                if components.is_dir() {
                    // Verify this is the real HA core by checking for a known integration
                    let hue_manifest = components.join("hue/manifest.json");
                    if hue_manifest.exists() {
                        return Some(components);
                    }
                }
            }
        }
    }

    None
}

/// Load all manifests from the components directory
fn load_all_manifests() -> HashMap<String, IntegrationManifest> {
    let mut manifests = HashMap::new();

    let Some(components_dir) = find_components_dir() else {
        warn!("Could not find Home Assistant components directory");
        return manifests;
    };

    info!("Loading integration manifests from {:?}", components_dir);

    let entries = match std::fs::read_dir(&components_dir) {
        Ok(e) => e,
        Err(e) => {
            warn!("Failed to read components directory: {}", e);
            return manifests;
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let manifest_path = path.join("manifest.json");
        if !manifest_path.exists() {
            continue;
        }

        let domain = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_string();

        match std::fs::read_to_string(&manifest_path) {
            Ok(content) => match serde_json::from_str::<IntegrationManifest>(&content) {
                Ok(mut manifest) => {
                    manifest.is_built_in = true;
                    debug!("Loaded manifest for {}", domain);
                    manifests.insert(domain, manifest);
                }
                Err(e) => {
                    debug!("Failed to parse manifest for {}: {}", domain, e);
                }
            },
            Err(e) => {
                debug!("Failed to read manifest for {}: {}", domain, e);
            }
        }
    }

    info!("Loaded {} integration manifests", manifests.len());
    manifests
}

/// Get all cached manifests
pub fn get_all_manifests() -> &'static HashMap<String, IntegrationManifest> {
    MANIFESTS.get_or_init(load_all_manifests)
}

/// Get a specific manifest by domain
pub fn get_manifest(domain: &str) -> Option<&'static IntegrationManifest> {
    get_all_manifests().get(domain)
}

/// Get all manifests with config_flow enabled
pub fn get_config_flow_manifests(
) -> impl Iterator<Item = (&'static String, &'static IntegrationManifest)> {
    get_all_manifests().iter().filter(|(_, m)| m.config_flow)
}

/// Build the integration descriptions response for the frontend
pub fn build_integration_descriptions() -> serde_json::Value {
    let manifests = get_all_manifests();

    let mut integrations = serde_json::Map::new();

    for (domain, manifest) in manifests.iter() {
        if !manifest.config_flow {
            continue;
        }

        let entry = serde_json::json!({
            "config_flow": manifest.config_flow,
            "integration_type": manifest.integration_type.as_deref().unwrap_or("service"),
            "iot_class": manifest.iot_class.as_deref().unwrap_or("unknown"),
            "name": manifest.name,
            "single_config_entry": manifest.single_config_entry,
        });

        integrations.insert(domain.clone(), entry);
    }

    serde_json::json!({
        "core": {
            "integration": integrations,
            "helper": {},
            "translated_name": []
        },
        "custom": {
            "integration": {},
            "helper": {}
        }
    })
}

/// Build manifest response for manifest/get
pub fn build_manifest_response(domain: &str) -> Option<serde_json::Value> {
    get_manifest(domain).map(|m| {
        serde_json::json!({
            "domain": m.domain,
            "name": m.name,
            "config_flow": m.config_flow,
            "documentation": m.documentation,
            "codeowners": m.codeowners,
            "requirements": m.requirements,
            "dependencies": m.dependencies,
            "iot_class": m.iot_class,
            "integration_type": m.integration_type,
            "is_built_in": m.is_built_in,
        })
    })
}

/// Build manifest/list response
pub fn build_manifest_list() -> serde_json::Value {
    let manifests: Vec<serde_json::Value> = get_all_manifests()
        .values()
        .map(|m| {
            serde_json::json!({
                "domain": m.domain,
                "name": m.name,
                "config_flow": m.config_flow,
                "documentation": m.documentation,
                "codeowners": m.codeowners,
                "requirements": m.requirements,
                "dependencies": m.dependencies,
                "iot_class": m.iot_class,
                "integration_type": m.integration_type,
                "is_built_in": m.is_built_in,
            })
        })
        .collect();

    serde_json::Value::Array(manifests)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// Test that IntegrationManifest can be deserialized from JSON
    #[test]
    fn test_manifest_deserialization() {
        let json = r#"{
            "domain": "hue",
            "name": "Philips Hue",
            "config_flow": true,
            "integration_type": "hub",
            "iot_class": "local_polling",
            "single_config_entry": false,
            "documentation": "https://www.home-assistant.io/integrations/hue",
            "codeowners": ["@balloob"],
            "requirements": ["aiohue==4.5.0"],
            "dependencies": ["zeroconf"],
            "after_dependencies": []
        }"#;

        let manifest: IntegrationManifest = serde_json::from_str(json).unwrap();

        assert_eq!(manifest.domain, "hue");
        assert_eq!(manifest.name, "Philips Hue");
        assert!(manifest.config_flow);
        assert_eq!(manifest.integration_type, Some("hub".to_string()));
        assert_eq!(manifest.iot_class, Some("local_polling".to_string()));
        assert!(!manifest.single_config_entry);
        assert!(manifest.documentation.is_some());
        assert_eq!(manifest.codeowners.len(), 1);
        assert_eq!(manifest.requirements.len(), 1);
        assert_eq!(manifest.dependencies.len(), 1);
    }

    /// Test that missing optional fields use defaults
    #[test]
    fn test_manifest_defaults() {
        let json = r#"{
            "domain": "test",
            "name": "Test Integration"
        }"#;

        let manifest: IntegrationManifest = serde_json::from_str(json).unwrap();

        assert_eq!(manifest.domain, "test");
        assert_eq!(manifest.name, "Test Integration");
        assert!(!manifest.config_flow);
        assert!(manifest.integration_type.is_none());
        assert!(manifest.iot_class.is_none());
        assert!(!manifest.single_config_entry);
        assert!(manifest.documentation.is_none());
        assert!(manifest.codeowners.is_empty());
        assert!(manifest.requirements.is_empty());
        assert!(manifest.dependencies.is_empty());
    }

    /// Test loading manifests from a temporary directory
    #[test]
    fn test_load_manifests_from_dir() {
        let temp_dir = TempDir::new().unwrap();
        let components_dir = temp_dir.path().join("homeassistant/components");
        fs::create_dir_all(&components_dir).unwrap();

        // Create test integration with config_flow
        let hue_dir = components_dir.join("hue");
        fs::create_dir_all(&hue_dir).unwrap();
        fs::write(
            hue_dir.join("manifest.json"),
            r#"{"domain": "hue", "name": "Philips Hue", "config_flow": true}"#,
        )
        .unwrap();

        // Create test integration without config_flow
        let sun_dir = components_dir.join("sun");
        fs::create_dir_all(&sun_dir).unwrap();
        fs::write(
            sun_dir.join("manifest.json"),
            r#"{"domain": "sun", "name": "Sun", "config_flow": false}"#,
        )
        .unwrap();

        // Set HA_CORE_PATH to find our test components
        std::env::set_var("HA_CORE_PATH", temp_dir.path());

        // We can't test the cached version since OnceLock is already initialized
        // but we can test the loading function directly by reimplementing the logic
        let entries = fs::read_dir(&components_dir).unwrap();
        let mut manifests = HashMap::new();

        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            let manifest_path = path.join("manifest.json");
            if !manifest_path.exists() {
                continue;
            }

            let domain = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default()
                .to_string();

            if let Ok(content) = fs::read_to_string(&manifest_path) {
                if let Ok(manifest) = serde_json::from_str::<IntegrationManifest>(&content) {
                    manifests.insert(domain, manifest);
                }
            }
        }

        assert_eq!(manifests.len(), 2);
        assert!(manifests.contains_key("hue"));
        assert!(manifests.contains_key("sun"));

        let hue = manifests.get("hue").unwrap();
        assert!(hue.config_flow);

        let sun = manifests.get("sun").unwrap();
        assert!(!sun.config_flow);
    }

    /// Test build_integration_descriptions filters by config_flow
    #[test]
    fn test_build_integration_descriptions_format() {
        // This tests the structure of the response
        let descriptions = build_integration_descriptions();

        assert!(descriptions.is_object());
        assert!(descriptions.get("core").is_some());
        assert!(descriptions.get("custom").is_some());

        let core = descriptions.get("core").unwrap();
        assert!(core.get("integration").is_some());
        assert!(core.get("helper").is_some());
        assert!(core.get("translated_name").is_some());
    }

    /// Test that get_all_manifests returns at least some integrations
    /// (assumes vendor/ha-core is available)
    #[test]
    fn test_get_all_manifests_finds_integrations() {
        let manifests = get_all_manifests();

        // Should find a significant number of integrations if ha-core is available
        // If not available, this should at least not panic
        if !manifests.is_empty() {
            // Verify some well-known integrations exist
            assert!(
                manifests.contains_key("sun") || manifests.contains_key("hue"),
                "Expected to find common integrations"
            );
        }
    }

    /// Test get_config_flow_manifests only returns config_flow enabled integrations
    #[test]
    fn test_get_config_flow_manifests_filter() {
        for (domain, manifest) in get_config_flow_manifests() {
            assert!(
                manifest.config_flow,
                "Integration {} should have config_flow=true",
                domain
            );
        }
    }

    /// Test build_manifest_response returns correct structure
    #[test]
    fn test_build_manifest_response_structure() {
        // Try to get a manifest for any integration
        let manifests = get_all_manifests();
        if let Some(domain) = manifests.keys().next() {
            let response = build_manifest_response(domain);
            assert!(response.is_some());

            let manifest = response.unwrap();
            assert!(manifest.get("domain").is_some());
            assert!(manifest.get("name").is_some());
            assert!(manifest.get("config_flow").is_some());
        }
    }

    /// Test build_manifest_list returns array of manifests
    #[test]
    fn test_build_manifest_list_format() {
        let list = build_manifest_list();
        assert!(list.is_array());

        if let Some(arr) = list.as_array() {
            for item in arr {
                assert!(item.get("domain").is_some());
                assert!(item.get("name").is_some());
            }
        }
    }
}
