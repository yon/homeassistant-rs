//! Configuration for comparison tests

use std::env;
use std::path::PathBuf;

/// Configuration for the comparison test environment
#[derive(Debug, Clone)]
pub struct ComparisonConfig {
    /// URL of the Python Home Assistant instance
    pub python_ha_url: String,
    /// URL of the Rust Home Assistant instance
    pub rust_ha_url: String,
    /// Bearer token for Python HA authentication
    pub python_ha_token: String,
    /// Bearer token for Rust HA authentication (if auth is implemented)
    pub rust_ha_token: Option<String>,
    /// Home Assistant version being tested
    pub ha_version: String,
    /// Path to the comparison test directory
    pub test_dir: PathBuf,
}

impl Default for ComparisonConfig {
    fn default() -> Self {
        Self::from_env()
    }
}

impl ComparisonConfig {
    /// Load configuration from environment variables
    pub fn from_env() -> Self {
        let test_dir = env::var("HA_TEST_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                // Try to find the tests/comparison directory relative to workspace root
                let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
                manifest_dir
                    .parent() // crates/
                    .and_then(|p| p.parent()) // workspace root
                    .map(|p| p.join("tests").join("comparison"))
                    .unwrap_or_else(|| manifest_dir.join("tests").join("comparison"))
            });

        // Try to read token from file if not set in environment
        let python_ha_token = env::var("PYTHON_HA_TOKEN").unwrap_or_else(|_| {
            let token_file = test_dir.join("ha-config").join("test-token.txt");
            std::fs::read_to_string(&token_file)
                .map(|s| s.trim().to_string())
                .unwrap_or_else(|_| "TOKEN_FILE_NOT_FOUND".to_string())
        });

        Self {
            python_ha_url: env::var("PYTHON_HA_URL")
                .unwrap_or_else(|_| "http://localhost:18123".to_string()),
            rust_ha_url: env::var("RUST_HA_URL")
                .unwrap_or_else(|_| "http://localhost:18124".to_string()),
            python_ha_token,
            rust_ha_token: env::var("RUST_HA_TOKEN").ok(),
            ha_version: env::var("HA_VERSION").unwrap_or_else(|_| "2026.1.1".to_string()),
            test_dir,
        }
    }

    /// Get the path to the HA config directory
    pub fn ha_config_dir(&self) -> PathBuf {
        self.test_dir.join("ha-config")
    }

    /// Get the path to the token file
    pub fn token_file(&self) -> PathBuf {
        self.ha_config_dir().join("test-token.txt")
    }

    /// Load the token from file if not set in environment
    pub fn load_token_from_file(&mut self) -> std::io::Result<()> {
        if self.python_ha_token.is_empty() {
            let token_file = self.token_file();
            if token_file.exists() {
                self.python_ha_token = std::fs::read_to_string(token_file)?.trim().to_string();
            }
        }
        Ok(())
    }
}

/// Parsed version from ha-versions.toml
#[derive(Debug, Clone)]
pub struct HaVersion {
    pub version: String,
    pub docker_image: String,
    pub release_date: String,
    pub release_notes: Option<String>,
}

/// Load HA versions from the TOML config file
pub fn load_ha_versions(
    test_dir: &std::path::Path,
) -> Result<HaVersions, Box<dyn std::error::Error>> {
    let versions_file = test_dir.join("ha-versions.toml");
    let content = std::fs::read_to_string(&versions_file)?;

    // Simple TOML parsing (in production, use the toml crate)
    let mut versions = HaVersions::default();

    // Parse primary version
    if let Some(primary_section) = extract_section(&content, "[primary]") {
        versions.primary = parse_version_section(&primary_section);
    }

    // Parse previous version
    if let Some(previous_section) = extract_section(&content, "[previous]") {
        versions.previous = Some(parse_version_section(&previous_section));
    }

    // Parse minimum version
    if let Some(minimum_section) = extract_section(&content, "[minimum]") {
        versions.minimum = Some(parse_version_section(&minimum_section));
    }

    Ok(versions)
}

/// Collection of HA versions we test against
#[derive(Debug, Default)]
pub struct HaVersions {
    pub primary: HaVersion,
    pub previous: Option<HaVersion>,
    pub minimum: Option<HaVersion>,
    pub beta: Option<HaVersion>,
}

impl Default for HaVersion {
    fn default() -> Self {
        Self {
            version: "2026.1.1".to_string(),
            docker_image: "ghcr.io/home-assistant/home-assistant:2026.1.1".to_string(),
            release_date: "2026-01-07".to_string(),
            release_notes: None,
        }
    }
}

fn extract_section(content: &str, header: &str) -> Option<String> {
    let start = content.find(header)?;
    let rest = &content[start + header.len()..];
    let end = rest.find("\n[").unwrap_or(rest.len());
    Some(rest[..end].to_string())
}

fn parse_version_section(section: &str) -> HaVersion {
    let mut version = HaVersion::default();

    for line in section.lines() {
        let line = line.trim();
        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim();
            let value = value.trim().trim_matches('"');
            match key {
                "version" => version.version = value.to_string(),
                "docker_image" => version.docker_image = value.to_string(),
                "release_date" => version.release_date = value.to_string(),
                "release_notes" => version.release_notes = Some(value.to_string()),
                _ => {}
            }
        }
    }

    version
}
