//! YAML configuration loader with custom tag support
//!
//! Supports Home Assistant's custom YAML tags:
//! - `!include path` - Include another YAML file
//! - `!include_dir_list dir` - Include all YAML files in a directory as a list
//! - `!include_dir_merge_list dir` - Merge lists from all YAML files in a directory
//! - `!include_dir_named dir` - Include all YAML files as a mapping keyed by filename
//! - `!include_dir_merge_named dir` - Merge mappings from all YAML files
//! - `!secret key` - Substitute from secrets.yaml
//! - `!env_var VAR` - Environment variable substitution

use crate::error::{ConfigError, ConfigResult};
use crate::secrets::Secrets;
use serde_yaml::Value;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{debug, trace};

/// YAML loader with support for Home Assistant custom tags
pub struct YamlLoader {
    /// Base directory for resolving relative paths
    config_dir: PathBuf,
    /// Secrets store
    secrets: Secrets,
    /// Track included files to detect circular includes
    include_stack: HashSet<PathBuf>,
}

impl YamlLoader {
    /// Create a new YAML loader for the given config directory
    pub fn new(config_dir: impl Into<PathBuf>) -> ConfigResult<Self> {
        let config_dir = config_dir.into();
        let secrets = Secrets::load(&config_dir)?;

        Ok(Self {
            config_dir,
            secrets,
            include_stack: HashSet::new(),
        })
    }

    /// Create a loader with pre-loaded secrets
    pub fn with_secrets(config_dir: impl Into<PathBuf>, secrets: Secrets) -> Self {
        Self {
            config_dir: config_dir.into(),
            secrets,
            include_stack: HashSet::new(),
        }
    }

    /// Load and process a YAML file
    pub fn load_file(&mut self, path: impl AsRef<Path>) -> ConfigResult<Value> {
        let path = self.resolve_path(path.as_ref());
        debug!("Loading YAML file: {:?}", path);

        // Check for circular includes
        if self.include_stack.contains(&path) {
            return Err(ConfigError::CircularInclude { path });
        }

        let content = fs::read_to_string(&path).map_err(|e| ConfigError::ReadFile {
            path: path.clone(),
            source: e,
        })?;

        self.include_stack.insert(path.clone());
        let result = self.load_string(&content, &path);
        self.include_stack.remove(&path);

        result
    }

    /// Load and process YAML from a string
    pub fn load_string(&mut self, content: &str, source_path: &Path) -> ConfigResult<Value> {
        let value: Value = serde_yaml::from_str(content).map_err(|e| ConfigError::ParseYaml {
            path: source_path.to_path_buf(),
            source: e,
        })?;

        self.process_value(value, source_path)
    }

    /// Process a YAML value, handling custom tags
    fn process_value(&mut self, value: Value, source_path: &Path) -> ConfigResult<Value> {
        match value {
            Value::Tagged(tagged) => self.process_tagged(*tagged, source_path),
            Value::Mapping(map) => {
                let mut result = serde_yaml::Mapping::new();
                for (k, v) in map {
                    let processed_key = self.process_value(k, source_path)?;
                    let processed_value = self.process_value(v, source_path)?;
                    result.insert(processed_key, processed_value);
                }
                Ok(Value::Mapping(result))
            }
            Value::Sequence(seq) => {
                let result: ConfigResult<Vec<Value>> = seq
                    .into_iter()
                    .map(|v| self.process_value(v, source_path))
                    .collect();
                Ok(Value::Sequence(result?))
            }
            _ => Ok(value),
        }
    }

    /// Process a tagged value
    fn process_tagged(
        &mut self,
        tagged: serde_yaml::value::TaggedValue,
        source_path: &Path,
    ) -> ConfigResult<Value> {
        let tag = tagged.tag.to_string();
        let value = tagged.value;

        trace!("Processing tag '{}' with value {:?}", tag, value);

        match tag.as_str() {
            "!include" => self.process_include(value, source_path),
            "!include_dir_list" => self.process_include_dir_list(value, source_path),
            "!include_dir_merge_list" => self.process_include_dir_merge_list(value, source_path),
            "!include_dir_named" => self.process_include_dir_named(value, source_path),
            "!include_dir_merge_named" => self.process_include_dir_merge_named(value, source_path),
            "!secret" => self.process_secret(value),
            "!env_var" => self.process_env_var(value),
            _ => {
                // Unknown tag, keep it as-is but process the inner value
                let processed = self.process_value(value, source_path)?;
                Ok(Value::Tagged(Box::new(serde_yaml::value::TaggedValue {
                    tag: tagged.tag,
                    value: processed,
                })))
            }
        }
    }

    /// Process !include tag
    fn process_include(&mut self, value: Value, source_path: &Path) -> ConfigResult<Value> {
        let include_path = self.value_to_path(&value, source_path)?;
        debug!("Including file: {:?}", include_path);
        self.load_file(&include_path)
    }

    /// Process !include_dir_list tag - include all YAML files as a list
    fn process_include_dir_list(
        &mut self,
        value: Value,
        source_path: &Path,
    ) -> ConfigResult<Value> {
        let dir_path = self.value_to_path(&value, source_path)?;
        debug!("Including directory as list: {:?}", dir_path);

        let files = self.get_yaml_files(&dir_path)?;
        let mut result = Vec::new();

        for file in files {
            let content = self.load_file(&file)?;
            result.push(content);
        }

        Ok(Value::Sequence(result))
    }

    /// Process !include_dir_merge_list tag - merge lists from all YAML files
    fn process_include_dir_merge_list(
        &mut self,
        value: Value,
        source_path: &Path,
    ) -> ConfigResult<Value> {
        let dir_path = self.value_to_path(&value, source_path)?;
        debug!("Including directory as merged list: {:?}", dir_path);

        let files = self.get_yaml_files(&dir_path)?;
        let mut result = Vec::new();

        for file in files {
            let content = self.load_file(&file)?;
            match content {
                Value::Sequence(seq) => result.extend(seq),
                other => result.push(other),
            }
        }

        Ok(Value::Sequence(result))
    }

    /// Process !include_dir_named tag - include all YAML files as a mapping
    fn process_include_dir_named(
        &mut self,
        value: Value,
        source_path: &Path,
    ) -> ConfigResult<Value> {
        let dir_path = self.value_to_path(&value, source_path)?;
        debug!("Including directory as named mapping: {:?}", dir_path);

        let files = self.get_yaml_files(&dir_path)?;
        let mut result = serde_yaml::Mapping::new();

        for file in files {
            let name = file
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or_default()
                .to_string();
            let content = self.load_file(&file)?;
            result.insert(Value::String(name), content);
        }

        Ok(Value::Mapping(result))
    }

    /// Process !include_dir_merge_named tag - merge mappings from all YAML files
    fn process_include_dir_merge_named(
        &mut self,
        value: Value,
        source_path: &Path,
    ) -> ConfigResult<Value> {
        let dir_path = self.value_to_path(&value, source_path)?;
        debug!("Including directory as merged mapping: {:?}", dir_path);

        let files = self.get_yaml_files(&dir_path)?;
        let mut result = serde_yaml::Mapping::new();

        for file in files {
            let content = self.load_file(&file)?;
            if let Value::Mapping(map) = content {
                for (k, v) in map {
                    result.insert(k, v);
                }
            }
        }

        Ok(Value::Mapping(result))
    }

    /// Process !secret tag
    fn process_secret(&self, value: Value) -> ConfigResult<Value> {
        let key = match value {
            Value::String(s) => s,
            _ => {
                return Err(ConfigError::InvalidValue {
                    key: "!secret".to_string(),
                    reason: "secret key must be a string".to_string(),
                })
            }
        };

        let secret_value = self.secrets.get(&key)?;
        debug!("Substituted secret: {}", key);
        Ok(Value::String(secret_value.to_string()))
    }

    /// Process !env_var tag
    fn process_env_var(&self, value: Value) -> ConfigResult<Value> {
        let var_name = match value {
            Value::String(s) => s,
            _ => {
                return Err(ConfigError::InvalidValue {
                    key: "!env_var".to_string(),
                    reason: "environment variable name must be a string".to_string(),
                })
            }
        };

        let env_value = std::env::var(&var_name).map_err(|_| ConfigError::EnvVarNotFound {
            var: var_name.clone(),
        })?;

        debug!("Substituted env var: {}", var_name);
        Ok(Value::String(env_value))
    }

    /// Convert a YAML value to a path, resolving relative to source file
    fn value_to_path(&self, value: &Value, source_path: &Path) -> ConfigResult<PathBuf> {
        let path_str = match value {
            Value::String(s) => s.clone(),
            _ => {
                return Err(ConfigError::InvalidIncludePath {
                    path: format!("{:?}", value),
                    reason: "path must be a string".to_string(),
                })
            }
        };

        // Resolve relative to the source file's directory
        let base_dir = source_path.parent().unwrap_or(&self.config_dir);
        let resolved = if Path::new(&path_str).is_absolute() {
            PathBuf::from(&path_str)
        } else {
            base_dir.join(&path_str)
        };

        Ok(resolved)
    }

    /// Resolve a path relative to the config directory
    fn resolve_path(&self, path: &Path) -> PathBuf {
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.config_dir.join(path)
        }
    }

    /// Get all YAML files in a directory, sorted by name
    fn get_yaml_files(&self, dir: &Path) -> ConfigResult<Vec<PathBuf>> {
        if !dir.exists() {
            return Err(ConfigError::DirectoryNotFound {
                path: dir.to_path_buf(),
            });
        }

        if !dir.is_dir() {
            return Err(ConfigError::DirectoryNotFound {
                path: dir.to_path_buf(),
            });
        }

        let mut files: Vec<PathBuf> = fs::read_dir(dir)
            .map_err(|e| ConfigError::ReadFile {
                path: dir.to_path_buf(),
                source: e,
            })?
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|path| {
                path.extension()
                    .map(|ext| ext == "yaml" || ext == "yml")
                    .unwrap_or(false)
            })
            .collect();

        files.sort();
        Ok(files)
    }

    /// Get a reference to the secrets store
    pub fn secrets(&self) -> &Secrets {
        &self.secrets
    }

    /// Get the config directory
    pub fn config_dir(&self) -> &Path {
        &self.config_dir
    }
}

/// Load a YAML file with full tag processing
pub fn load_yaml(config_dir: impl Into<PathBuf>, file: impl AsRef<Path>) -> ConfigResult<Value> {
    let mut loader = YamlLoader::new(config_dir)?;
    loader.load_file(file)
}

/// Load a YAML string with tag processing
pub fn load_yaml_string(
    config_dir: impl Into<PathBuf>,
    content: &str,
    source_name: &str,
) -> ConfigResult<Value> {
    let mut loader = YamlLoader::new(config_dir)?;
    loader.load_string(content, Path::new(source_name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    fn write_file(dir: &Path, name: &str, content: &str) {
        let path = dir.join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        let mut file = fs::File::create(path).unwrap();
        file.write_all(content.as_bytes()).unwrap();
    }

    #[test]
    fn test_load_simple_yaml() {
        let dir = TempDir::new().unwrap();
        write_file(
            dir.path(),
            "config.yaml",
            r#"
key: value
number: 42
list:
  - one
  - two
"#,
        );

        let value = load_yaml(dir.path(), "config.yaml").unwrap();
        assert!(value.is_mapping());
    }

    #[test]
    fn test_include() {
        let dir = TempDir::new().unwrap();
        write_file(
            dir.path(),
            "included.yaml",
            "included_key: included_value\n",
        );
        write_file(
            dir.path(),
            "config.yaml",
            "main_key: main_value\nincluded: !include included.yaml\n",
        );

        let value = load_yaml(dir.path(), "config.yaml").unwrap();
        let map = value.as_mapping().unwrap();
        let included = map.get(&Value::String("included".to_string())).unwrap();
        let included_map = included.as_mapping().unwrap();
        assert_eq!(
            included_map.get(&Value::String("included_key".to_string())),
            Some(&Value::String("included_value".to_string()))
        );
    }

    #[test]
    fn test_secret() {
        let dir = TempDir::new().unwrap();
        write_file(dir.path(), "secrets.yaml", "my_password: secret123\n");
        write_file(dir.path(), "config.yaml", "password: !secret my_password\n");

        let value = load_yaml(dir.path(), "config.yaml").unwrap();
        let map = value.as_mapping().unwrap();
        assert_eq!(
            map.get(&Value::String("password".to_string())),
            Some(&Value::String("secret123".to_string()))
        );
    }

    #[test]
    fn test_env_var() {
        let dir = TempDir::new().unwrap();
        std::env::set_var("TEST_HA_CONFIG_VAR", "env_value");
        write_file(
            dir.path(),
            "config.yaml",
            "from_env: !env_var TEST_HA_CONFIG_VAR\n",
        );

        let value = load_yaml(dir.path(), "config.yaml").unwrap();
        let map = value.as_mapping().unwrap();
        assert_eq!(
            map.get(&Value::String("from_env".to_string())),
            Some(&Value::String("env_value".to_string()))
        );

        std::env::remove_var("TEST_HA_CONFIG_VAR");
    }

    #[test]
    fn test_include_dir_list() {
        let dir = TempDir::new().unwrap();
        fs::create_dir(dir.path().join("automations")).unwrap();
        write_file(
            dir.path(),
            "automations/auto1.yaml",
            "alias: Automation 1\n",
        );
        write_file(
            dir.path(),
            "automations/auto2.yaml",
            "alias: Automation 2\n",
        );
        write_file(
            dir.path(),
            "config.yaml",
            "automation: !include_dir_list automations\n",
        );

        let value = load_yaml(dir.path(), "config.yaml").unwrap();
        let map = value.as_mapping().unwrap();
        let automation = map.get(&Value::String("automation".to_string())).unwrap();
        let seq = automation.as_sequence().unwrap();
        assert_eq!(seq.len(), 2);
    }

    #[test]
    fn test_include_dir_merge_list() {
        let dir = TempDir::new().unwrap();
        fs::create_dir(dir.path().join("automations")).unwrap();
        write_file(
            dir.path(),
            "automations/auto1.yaml",
            "- alias: Automation 1\n- alias: Automation 2\n",
        );
        write_file(
            dir.path(),
            "automations/auto2.yaml",
            "- alias: Automation 3\n",
        );
        write_file(
            dir.path(),
            "config.yaml",
            "automation: !include_dir_merge_list automations\n",
        );

        let value = load_yaml(dir.path(), "config.yaml").unwrap();
        let map = value.as_mapping().unwrap();
        let automation = map.get(&Value::String("automation".to_string())).unwrap();
        let seq = automation.as_sequence().unwrap();
        assert_eq!(seq.len(), 3);
    }

    #[test]
    fn test_include_dir_named() {
        let dir = TempDir::new().unwrap();
        fs::create_dir(dir.path().join("lights")).unwrap();
        write_file(dir.path(), "lights/bedroom.yaml", "brightness: 100\n");
        write_file(dir.path(), "lights/kitchen.yaml", "brightness: 50\n");
        write_file(
            dir.path(),
            "config.yaml",
            "lights: !include_dir_named lights\n",
        );

        let value = load_yaml(dir.path(), "config.yaml").unwrap();
        let map = value.as_mapping().unwrap();
        let lights = map.get(&Value::String("lights".to_string())).unwrap();
        let lights_map = lights.as_mapping().unwrap();
        assert!(lights_map.contains_key(&Value::String("bedroom".to_string())));
        assert!(lights_map.contains_key(&Value::String("kitchen".to_string())));
    }

    #[test]
    fn test_circular_include_detection() {
        let dir = TempDir::new().unwrap();
        write_file(dir.path(), "a.yaml", "include_b: !include b.yaml\n");
        write_file(dir.path(), "b.yaml", "include_a: !include a.yaml\n");

        let result = load_yaml(dir.path(), "a.yaml");
        assert!(matches!(result, Err(ConfigError::CircularInclude { .. })));
    }

    #[test]
    fn test_missing_secret() {
        let dir = TempDir::new().unwrap();
        write_file(dir.path(), "secrets.yaml", "existing: value\n");
        write_file(dir.path(), "config.yaml", "password: !secret nonexistent\n");

        let result = load_yaml(dir.path(), "config.yaml");
        assert!(matches!(result, Err(ConfigError::SecretNotFound { .. })));
    }
}
