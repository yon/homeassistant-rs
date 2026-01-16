//! Secrets loader for Home Assistant configuration
//!
//! Loads secrets from secrets.yaml and provides lookup functionality.

use crate::error::{ConfigError, ConfigResult};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use tracing::debug;

/// Secrets store loaded from secrets.yaml
#[derive(Debug, Clone)]
pub struct Secrets {
    /// The secrets as key-value pairs
    secrets: HashMap<String, String>,
    /// Path to the secrets file
    path: PathBuf,
}

impl Secrets {
    /// Load secrets from a secrets.yaml file
    pub fn load(config_dir: &Path) -> ConfigResult<Self> {
        let path = config_dir.join("secrets.yaml");

        if !path.exists() {
            debug!("No secrets.yaml found at {:?}, using empty secrets", path);
            return Ok(Self {
                secrets: HashMap::new(),
                path,
            });
        }

        let content = fs::read_to_string(&path).map_err(|e| ConfigError::ReadFile {
            path: path.clone(),
            source: e,
        })?;

        let secrets: HashMap<String, serde_yaml::Value> =
            serde_yaml::from_str(&content).map_err(|e| ConfigError::ParseYaml {
                path: path.clone(),
                source: e,
            })?;

        // Convert all values to strings
        let secrets: HashMap<String, String> = secrets
            .into_iter()
            .map(|(k, v)| {
                let str_value = match v {
                    serde_yaml::Value::String(s) => s,
                    serde_yaml::Value::Number(n) => n.to_string(),
                    serde_yaml::Value::Bool(b) => b.to_string(),
                    serde_yaml::Value::Null => String::new(),
                    _ => serde_yaml::to_string(&v)
                        .unwrap_or_default()
                        .trim()
                        .to_string(),
                };
                (k, str_value)
            })
            .collect();

        debug!("Loaded {} secrets from {:?}", secrets.len(), path);

        Ok(Self { secrets, path })
    }

    /// Get a secret by key
    pub fn get(&self, key: &str) -> ConfigResult<&str> {
        self.secrets
            .get(key)
            .map(|s| s.as_str())
            .ok_or_else(|| ConfigError::SecretNotFound {
                key: key.to_string(),
            })
    }

    /// Check if a secret exists
    pub fn contains(&self, key: &str) -> bool {
        self.secrets.contains_key(key)
    }

    /// Get the path to the secrets file
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Get the number of secrets loaded
    pub fn len(&self) -> usize {
        self.secrets.len()
    }

    /// Check if secrets store is empty
    pub fn is_empty(&self) -> bool {
        self.secrets.is_empty()
    }
}

impl Default for Secrets {
    fn default() -> Self {
        Self {
            secrets: HashMap::new(),
            path: PathBuf::from("secrets.yaml"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    fn create_secrets_file(dir: &Path, content: &str) {
        let path = dir.join("secrets.yaml");
        let mut file = fs::File::create(path).unwrap();
        file.write_all(content.as_bytes()).unwrap();
    }

    #[test]
    fn test_load_secrets() {
        let dir = TempDir::new().unwrap();
        create_secrets_file(
            dir.path(),
            r#"
api_key: secret123
password: hunter2
port: 8080
enabled: true
"#,
        );

        let secrets = Secrets::load(dir.path()).unwrap();
        assert_eq!(secrets.get("api_key").unwrap(), "secret123");
        assert_eq!(secrets.get("password").unwrap(), "hunter2");
        assert_eq!(secrets.get("port").unwrap(), "8080");
        assert_eq!(secrets.get("enabled").unwrap(), "true");
        assert_eq!(secrets.len(), 4);
    }

    #[test]
    fn test_missing_secret() {
        let dir = TempDir::new().unwrap();
        create_secrets_file(dir.path(), "key: value\n");

        let secrets = Secrets::load(dir.path()).unwrap();
        let result = secrets.get("nonexistent");
        assert!(matches!(result, Err(ConfigError::SecretNotFound { .. })));
    }

    #[test]
    fn test_no_secrets_file() {
        let dir = TempDir::new().unwrap();
        let secrets = Secrets::load(dir.path()).unwrap();
        assert!(secrets.is_empty());
    }

    #[test]
    fn test_contains() {
        let dir = TempDir::new().unwrap();
        create_secrets_file(dir.path(), "existing: value\n");

        let secrets = Secrets::load(dir.path()).unwrap();
        assert!(secrets.contains("existing"));
        assert!(!secrets.contains("nonexistent"));
    }
}
