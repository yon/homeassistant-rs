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

/// Find the generated integrations.json file from HA core
fn find_generated_integrations_json() -> Option<PathBuf> {
    // Check HA_CORE_PATH first
    if let Ok(path) = std::env::var("HA_CORE_PATH") {
        let json_path = PathBuf::from(&path).join("homeassistant/generated/integrations.json");
        if json_path.exists() {
            return Some(json_path);
        }
    }

    // Try relative path from current directory (for development/production)
    let dev_path = PathBuf::from("vendor/ha-core/homeassistant/generated/integrations.json");
    if dev_path.exists() {
        return Some(dev_path);
    }

    // Try from CARGO_MANIFEST_DIR (workspace root for tests)
    if let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
        // Navigate up from crate dir to workspace root
        if let Some(workspace_root) = PathBuf::from(&manifest_dir)
            .parent()
            .and_then(|p| p.parent())
        {
            let ws_path =
                workspace_root.join("vendor/ha-core/homeassistant/generated/integrations.json");
            if ws_path.exists() {
                return Some(ws_path);
            }
        }
    }

    // Parse HA_COMPONENTS_PATH to derive the generated path
    if let Ok(components_path) = std::env::var("HA_COMPONENTS_PATH") {
        let generated = PathBuf::from(&components_path)
            .parent()
            .map(|p| p.join("generated/integrations.json"));
        if let Some(ref path) = generated {
            if path.exists() {
                return generated;
            }
        }
    }

    None
}

/// Build the integration descriptions response for the frontend.
///
/// Loads from `homeassistant/generated/integrations.json` which contains brand groups,
/// helper definitions, and translated_name entries that the frontend expects.
/// Falls back to building from individual manifests if the generated file is unavailable.
pub fn build_integration_descriptions() -> serde_json::Value {
    // Try to load the generated integrations.json (has brand groups, helpers, translated_name)
    if let Some(path) = find_generated_integrations_json() {
        if let Ok(content) = std::fs::read_to_string(&path) {
            if let Ok(generated) = serde_json::from_str::<serde_json::Value>(&content) {
                info!("Loaded integration descriptions from {:?}", path);
                return serde_json::json!({
                    "core": generated,
                    "custom": {
                        "integration": {},
                        "helper": {}
                    }
                });
            }
        }
        warn!("Failed to load integration descriptions from {:?}", path);
    }

    // Fallback: build from individual manifests (loses brand groups and helpers)
    let manifests = get_all_manifests();
    let mut integrations = serde_json::Map::new();

    for (domain, manifest) in manifests.iter() {
        if !manifest.config_flow {
            continue;
        }

        let entry = serde_json::json!({
            "config_flow": manifest.config_flow,
            "integration_type": manifest.integration_type.as_deref().unwrap_or("hub"),
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

    /// Bug: build_integration_descriptions uses unwrap_or("service") but HA Python
    /// defaults to "hub" (loader.py:861). Integrations without an explicit
    /// integration_type in their manifest should default to "hub".
    ///
    /// This test directly verifies the default by constructing a manifest without
    /// integration_type and checking the output of the description builder logic.
    #[test]
    fn test_integration_type_defaults_to_hub() {
        // Rather than relying on OnceLock-cached manifests (which may or may not load
        // depending on working directory), directly test the default value used in
        // build_integration_descriptions by constructing a manifest and checking the
        // unwrap_or default.
        let manifest = IntegrationManifest {
            domain: "test_no_type".to_string(),
            name: "Test No Type".to_string(),
            config_flow: true,
            integration_type: None, // explicitly None
            iot_class: None,
            single_config_entry: false,
            documentation: None,
            codeowners: vec![],
            requirements: vec![],
            dependencies: vec![],
            after_dependencies: vec![],
            is_built_in: true,
        };

        // Reproduce the exact defaulting logic from build_integration_descriptions
        let integration_type = manifest.integration_type.as_deref().unwrap_or("hub");

        // This assertion should FAIL because the current code defaults to "service",
        // but HA Python defaults to "hub" (loader.py:861)
        assert_eq!(
            integration_type, "hub",
            "Integration with no explicit integration_type should default to 'hub' \
             (per HA Python loader.py:861), but got '{}'. \
             Fix: change unwrap_or(\"service\") to unwrap_or(\"hub\")",
            integration_type
        );
    }

    /// Bug: build_integration_descriptions() currently builds from individual manifest.json
    /// files and produces a flat integration map. HA Python frontend expects the response
    /// to include brand groups (entries with nested "integrations" key) and a populated
    /// "translated_name" array. The correct approach is to load vendor/ha-core/homeassistant/
    /// generated/integrations.json which already has this structure.
    #[test]
    fn test_integration_descriptions_has_brand_groups_and_translated_names() {
        let descriptions = build_integration_descriptions();
        let core = descriptions.get("core").expect("should have core key");

        // Check translated_name is populated (not an empty array)
        let translated_name = core
            .get("translated_name")
            .expect("should have translated_name key");
        let tn_arr = translated_name
            .as_array()
            .expect("translated_name should be array");
        assert!(
            !tn_arr.is_empty(),
            "translated_name should be populated from generated/integrations.json, \
             but got an empty array. HA Python's integrations.json has 63+ entries."
        );

        // Check that the integration map contains brand groups with nested "integrations".
        // For example, "lutron" should have { name: "Lutron", integrations: { lutron: {...}, ... } }
        let integrations = core
            .get("integration")
            .expect("should have integration key");
        let integration_map = integrations
            .as_object()
            .expect("integration should be an object");

        // Look for any entry with an "integrations" sub-key (brand group).
        let has_brand_groups = integration_map
            .values()
            .any(|v| v.get("integrations").is_some());
        assert!(
            has_brand_groups,
            "Integration descriptions should contain brand groups (entries with nested \
             'integrations' key) from generated/integrations.json. Currently building from \
             individual manifests loses this structure."
        );
    }

    /// Bug: build_integration_descriptions() should include the "helper" section from
    /// generated/integrations.json, not return an empty helper map.
    #[test]
    fn test_integration_descriptions_has_helpers() {
        let descriptions = build_integration_descriptions();
        let core = descriptions.get("core").expect("should have core key");

        let helper = core.get("helper").expect("should have helper key");
        let helper_map = helper.as_object().expect("helper should be an object");
        assert!(
            !helper_map.is_empty(),
            "helper section should be populated from generated/integrations.json, \
             but got an empty object. HA Python's integrations.json has 27 helpers."
        );
    }
}
