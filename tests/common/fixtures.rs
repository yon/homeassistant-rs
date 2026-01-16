//! Test fixtures and data loading
//!
//! Provides utilities for loading test fixtures and test data.

use std::path::Path;

/// Load a fixture file as a string
///
/// Fixtures are stored in the `tests/fixtures/` directory.
pub fn load_fixture(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name);

    std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "Failed to load fixture '{}' from {:?}: {}",
            name, path, e
        )
    })
}

/// Load a fixture file as JSON
pub fn load_json_fixture(name: &str) -> serde_json::Value {
    let content = load_fixture(name);
    serde_json::from_str(&content).unwrap_or_else(|e| {
        panic!("Failed to parse fixture '{}' as JSON: {}", name, e)
    })
}

/// Load a fixture file as YAML
#[allow(dead_code)]
pub fn load_yaml_fixture(name: &str) -> serde_yaml::Value {
    let content = load_fixture(name);
    serde_yaml::from_str(&content).unwrap_or_else(|e| {
        panic!("Failed to parse fixture '{}' as YAML: {}", name, e)
    })
}

/// Macro for loading fixtures at compile time
#[macro_export]
macro_rules! include_fixture {
    ($name:expr) => {
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/",
            $name
        ))
    };
}
