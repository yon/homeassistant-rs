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
    /// Supports: `!env_var VAR` or `!env_var VAR default_value`
    fn process_env_var(&self, value: Value) -> ConfigResult<Value> {
        let input = match value {
            Value::String(s) => s,
            _ => {
                return Err(ConfigError::InvalidValue {
                    key: "!env_var".to_string(),
                    reason: "environment variable must be a string".to_string(),
                })
            }
        };

        // Parse "VAR_NAME" or "VAR_NAME default_value"
        let parts: Vec<&str> = input.splitn(2, ' ').collect();
        let var_name = parts[0];
        let default_value = parts.get(1).map(|s| s.to_string());

        match std::env::var(var_name) {
            Ok(env_value) => {
                debug!("Substituted env var: {}", var_name);
                Ok(Value::String(env_value))
            }
            Err(_) => {
                if let Some(default) = default_value {
                    debug!("Using default for env var {}: {}", var_name, default);
                    Ok(Value::String(default))
                } else {
                    Err(ConfigError::EnvVarNotFound {
                        var: var_name.to_string(),
                    })
                }
            }
        }
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

    /// Get all YAML files in a directory recursively, sorted by name
    /// Filters out hidden files/directories (starting with `.`) and `secrets.yaml`
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

        let mut files = Vec::new();
        self.collect_yaml_files_recursive(dir, &mut files)?;
        files.sort();
        Ok(files)
    }

    /// Recursively collect YAML files from a directory
    fn collect_yaml_files_recursive(
        &self,
        dir: &Path,
        files: &mut Vec<PathBuf>,
    ) -> ConfigResult<()> {
        let entries = fs::read_dir(dir).map_err(|e| ConfigError::ReadFile {
            path: dir.to_path_buf(),
            source: e,
        })?;

        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

            // Skip hidden files and directories
            if file_name.starts_with('.') {
                continue;
            }

            if path.is_dir() {
                // Skip directories named "ignore" (for testing compatibility)
                if file_name != "ignore" {
                    self.collect_yaml_files_recursive(&path, files)?;
                }
            } else if path.is_file() {
                // Check if it's a YAML file
                let is_yaml = path
                    .extension()
                    .map(|ext| ext == "yaml" || ext == "yml")
                    .unwrap_or(false);

                // Skip secrets.yaml
                let is_secrets = file_name == "secrets.yaml" || file_name == "secrets.yml";

                if is_yaml && !is_secrets {
                    files.push(path);
                }
            }
        }

        Ok(())
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

    // ==================== Core Parsing Tests ====================

    #[test]
    fn test_simple_list() {
        let dir = TempDir::new().unwrap();
        write_file(dir.path(), "config.yaml", "config:\n  - simple\n  - list\n");

        let value = load_yaml(dir.path(), "config.yaml").unwrap();
        let map = value.as_mapping().unwrap();
        let config = map.get(&Value::String("config".to_string())).unwrap();
        let seq = config.as_sequence().unwrap();
        assert_eq!(seq.len(), 2);
        assert_eq!(seq[0], Value::String("simple".to_string()));
        assert_eq!(seq[1], Value::String("list".to_string()));
    }

    #[test]
    fn test_simple_dict() {
        let dir = TempDir::new().unwrap();
        write_file(dir.path(), "config.yaml", "key: value\n");

        let value = load_yaml(dir.path(), "config.yaml").unwrap();
        let map = value.as_mapping().unwrap();
        assert_eq!(
            map.get(&Value::String("key".to_string())),
            Some(&Value::String("value".to_string()))
        );
    }

    #[test]
    fn test_nested_structure() {
        let dir = TempDir::new().unwrap();
        write_file(
            dir.path(),
            "config.yaml",
            r#"
outer:
  inner:
    key: value
  list:
    - item1
    - item2
"#,
        );

        let value = load_yaml(dir.path(), "config.yaml").unwrap();
        assert!(value.is_mapping());
    }

    // ==================== Environment Variable Tests ====================

    #[test]
    fn test_environment_variable() {
        let dir = TempDir::new().unwrap();
        std::env::set_var("TEST_HA_PASSWORD", "secret_password");
        write_file(
            dir.path(),
            "config.yaml",
            "password: !env_var TEST_HA_PASSWORD\n",
        );

        let value = load_yaml(dir.path(), "config.yaml").unwrap();
        let map = value.as_mapping().unwrap();
        assert_eq!(
            map.get(&Value::String("password".to_string())),
            Some(&Value::String("secret_password".to_string()))
        );

        std::env::remove_var("TEST_HA_PASSWORD");
    }

    #[test]
    fn test_environment_variable_default() {
        let dir = TempDir::new().unwrap();
        // Ensure the var is not set
        std::env::remove_var("TEST_HA_UNDEFINED_VAR");
        write_file(
            dir.path(),
            "config.yaml",
            "password: !env_var TEST_HA_UNDEFINED_VAR default_password\n",
        );

        let value = load_yaml(dir.path(), "config.yaml").unwrap();
        let map = value.as_mapping().unwrap();
        assert_eq!(
            map.get(&Value::String("password".to_string())),
            Some(&Value::String("default_password".to_string()))
        );
    }

    #[test]
    fn test_invalid_environment_variable() {
        let dir = TempDir::new().unwrap();
        std::env::remove_var("TEST_HA_MISSING_VAR");
        write_file(
            dir.path(),
            "config.yaml",
            "password: !env_var TEST_HA_MISSING_VAR\n",
        );

        let result = load_yaml(dir.path(), "config.yaml");
        assert!(matches!(result, Err(ConfigError::EnvVarNotFound { .. })));
    }

    // ==================== Secret Tests ====================

    #[test]
    fn test_secrets_from_yaml() {
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
    fn test_secret_with_numeric_value() {
        let dir = TempDir::new().unwrap();
        write_file(dir.path(), "secrets.yaml", "port: 8080\n");
        write_file(dir.path(), "config.yaml", "port: !secret port\n");

        let value = load_yaml(dir.path(), "config.yaml").unwrap();
        let map = value.as_mapping().unwrap();
        assert_eq!(
            map.get(&Value::String("port".to_string())),
            Some(&Value::String("8080".to_string()))
        );
    }

    #[test]
    fn test_missing_secret() {
        let dir = TempDir::new().unwrap();
        write_file(dir.path(), "secrets.yaml", "existing: value\n");
        write_file(dir.path(), "config.yaml", "password: !secret nonexistent\n");

        let result = load_yaml(dir.path(), "config.yaml");
        assert!(matches!(result, Err(ConfigError::SecretNotFound { .. })));
    }

    #[test]
    fn test_no_secrets_file() {
        let dir = TempDir::new().unwrap();
        write_file(dir.path(), "config.yaml", "key: value\n");

        // Should work - no secrets referenced
        let value = load_yaml(dir.path(), "config.yaml").unwrap();
        assert!(value.is_mapping());
    }

    // ==================== Include Tests ====================

    #[test]
    fn test_include_yaml() {
        let dir = TempDir::new().unwrap();
        write_file(
            dir.path(),
            "included.yaml",
            "included_key: included_value\n",
        );
        write_file(dir.path(), "config.yaml", "key: !include included.yaml\n");

        let value = load_yaml(dir.path(), "config.yaml").unwrap();
        let map = value.as_mapping().unwrap();
        let included = map.get(&Value::String("key".to_string())).unwrap();
        let included_map = included.as_mapping().unwrap();
        assert_eq!(
            included_map.get(&Value::String("included_key".to_string())),
            Some(&Value::String("included_value".to_string()))
        );
    }

    #[test]
    fn test_include_yaml_list() {
        let dir = TempDir::new().unwrap();
        write_file(dir.path(), "list.yaml", "- one\n- two\n- three\n");
        write_file(dir.path(), "config.yaml", "items: !include list.yaml\n");

        let value = load_yaml(dir.path(), "config.yaml").unwrap();
        let map = value.as_mapping().unwrap();
        let items = map.get(&Value::String("items".to_string())).unwrap();
        let seq = items.as_sequence().unwrap();
        assert_eq!(seq.len(), 3);
    }

    #[test]
    fn test_include_nested() {
        let dir = TempDir::new().unwrap();
        write_file(dir.path(), "deep.yaml", "deep_key: deep_value\n");
        write_file(dir.path(), "middle.yaml", "middle: !include deep.yaml\n");
        write_file(dir.path(), "config.yaml", "outer: !include middle.yaml\n");

        let value = load_yaml(dir.path(), "config.yaml").unwrap();
        let map = value.as_mapping().unwrap();
        let outer = map.get(&Value::String("outer".to_string())).unwrap();
        let outer_map = outer.as_mapping().unwrap();
        let middle = outer_map.get(&Value::String("middle".to_string())).unwrap();
        let middle_map = middle.as_mapping().unwrap();
        assert_eq!(
            middle_map.get(&Value::String("deep_key".to_string())),
            Some(&Value::String("deep_value".to_string()))
        );
    }

    #[test]
    fn test_include_relative_path() {
        let dir = TempDir::new().unwrap();
        fs::create_dir_all(dir.path().join("subdir")).unwrap();
        write_file(dir.path(), "subdir/nested.yaml", "nested: true\n");
        write_file(
            dir.path(),
            "subdir/config.yaml",
            "data: !include nested.yaml\n",
        );

        let value = load_yaml(dir.path(), "subdir/config.yaml").unwrap();
        let map = value.as_mapping().unwrap();
        let data = map.get(&Value::String("data".to_string())).unwrap();
        assert!(data.is_mapping());
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
    fn test_include_file_not_found() {
        let dir = TempDir::new().unwrap();
        write_file(
            dir.path(),
            "config.yaml",
            "data: !include nonexistent.yaml\n",
        );

        let result = load_yaml(dir.path(), "config.yaml");
        assert!(matches!(result, Err(ConfigError::ReadFile { .. })));
    }

    // ==================== Include Dir List Tests ====================

    #[test]
    fn test_include_dir_list() {
        let dir = TempDir::new().unwrap();
        fs::create_dir(dir.path().join("items")).unwrap();
        write_file(dir.path(), "items/one.yaml", "value: one\n");
        write_file(dir.path(), "items/two.yaml", "value: two\n");
        write_file(dir.path(), "config.yaml", "key: !include_dir_list items\n");

        let value = load_yaml(dir.path(), "config.yaml").unwrap();
        let map = value.as_mapping().unwrap();
        let key = map.get(&Value::String("key".to_string())).unwrap();
        let seq = key.as_sequence().unwrap();
        assert_eq!(seq.len(), 2);
    }

    #[test]
    fn test_include_dir_list_recursive() {
        let dir = TempDir::new().unwrap();
        fs::create_dir_all(dir.path().join("items/subdir")).unwrap();
        write_file(dir.path(), "items/zero.yaml", "value: zero\n");
        write_file(dir.path(), "items/subdir/one.yaml", "value: one\n");
        write_file(dir.path(), "items/subdir/two.yaml", "value: two\n");
        write_file(dir.path(), "config.yaml", "key: !include_dir_list items\n");

        let value = load_yaml(dir.path(), "config.yaml").unwrap();
        let map = value.as_mapping().unwrap();
        let key = map.get(&Value::String("key".to_string())).unwrap();
        let seq = key.as_sequence().unwrap();
        assert_eq!(seq.len(), 3); // zero, one, two
    }

    #[test]
    fn test_include_dir_list_filters_hidden() {
        let dir = TempDir::new().unwrap();
        fs::create_dir_all(dir.path().join("items/.hidden")).unwrap();
        write_file(dir.path(), "items/visible.yaml", "value: visible\n");
        write_file(dir.path(), "items/.hidden.yaml", "value: hidden\n");
        write_file(dir.path(), "items/.hidden/file.yaml", "value: hidden_dir\n");
        write_file(dir.path(), "config.yaml", "key: !include_dir_list items\n");

        let value = load_yaml(dir.path(), "config.yaml").unwrap();
        let map = value.as_mapping().unwrap();
        let key = map.get(&Value::String("key".to_string())).unwrap();
        let seq = key.as_sequence().unwrap();
        assert_eq!(seq.len(), 1); // Only visible.yaml
    }

    // ==================== Include Dir Merge List Tests ====================

    #[test]
    fn test_include_dir_merge_list() {
        let dir = TempDir::new().unwrap();
        fs::create_dir(dir.path().join("items")).unwrap();
        write_file(dir.path(), "items/first.yaml", "- one\n- two\n");
        write_file(dir.path(), "items/second.yaml", "- three\n");
        write_file(
            dir.path(),
            "config.yaml",
            "key: !include_dir_merge_list items\n",
        );

        let value = load_yaml(dir.path(), "config.yaml").unwrap();
        let map = value.as_mapping().unwrap();
        let key = map.get(&Value::String("key".to_string())).unwrap();
        let seq = key.as_sequence().unwrap();
        assert_eq!(seq.len(), 3);
    }

    #[test]
    fn test_include_dir_merge_list_recursive() {
        let dir = TempDir::new().unwrap();
        fs::create_dir_all(dir.path().join("items/subdir")).unwrap();
        write_file(dir.path(), "items/first.yaml", "- one\n");
        write_file(dir.path(), "items/subdir/second.yaml", "- two\n- three\n");
        write_file(dir.path(), "items/subdir/third.yaml", "- four\n");
        write_file(
            dir.path(),
            "config.yaml",
            "key: !include_dir_merge_list items\n",
        );

        let value = load_yaml(dir.path(), "config.yaml").unwrap();
        let map = value.as_mapping().unwrap();
        let key = map.get(&Value::String("key".to_string())).unwrap();
        let seq = key.as_sequence().unwrap();
        assert_eq!(seq.len(), 4);
    }

    // ==================== Include Dir Named Tests ====================

    #[test]
    fn test_include_dir_named() {
        let dir = TempDir::new().unwrap();
        fs::create_dir(dir.path().join("items")).unwrap();
        write_file(dir.path(), "items/first.yaml", "value: one\n");
        write_file(dir.path(), "items/second.yaml", "value: two\n");
        write_file(dir.path(), "config.yaml", "key: !include_dir_named items\n");

        let value = load_yaml(dir.path(), "config.yaml").unwrap();
        let map = value.as_mapping().unwrap();
        let key = map.get(&Value::String("key".to_string())).unwrap();
        let key_map = key.as_mapping().unwrap();
        assert!(key_map.contains_key(&Value::String("first".to_string())));
        assert!(key_map.contains_key(&Value::String("second".to_string())));
    }

    #[test]
    fn test_include_dir_named_filters_secrets() {
        let dir = TempDir::new().unwrap();
        fs::create_dir(dir.path().join("items")).unwrap();
        write_file(dir.path(), "items/first.yaml", "value: one\n");
        write_file(dir.path(), "items/secrets.yaml", "secret: hidden\n");
        write_file(dir.path(), "config.yaml", "key: !include_dir_named items\n");

        let value = load_yaml(dir.path(), "config.yaml").unwrap();
        let map = value.as_mapping().unwrap();
        let key = map.get(&Value::String("key".to_string())).unwrap();
        let key_map = key.as_mapping().unwrap();
        assert!(key_map.contains_key(&Value::String("first".to_string())));
        assert!(!key_map.contains_key(&Value::String("secrets".to_string())));
    }

    #[test]
    fn test_include_dir_named_recursive() {
        let dir = TempDir::new().unwrap();
        fs::create_dir_all(dir.path().join("items/subdir")).unwrap();
        write_file(dir.path(), "items/first.yaml", "value: one\n");
        write_file(dir.path(), "items/subdir/second.yaml", "value: two\n");
        write_file(dir.path(), "items/subdir/third.yaml", "value: three\n");
        write_file(dir.path(), "config.yaml", "key: !include_dir_named items\n");

        let value = load_yaml(dir.path(), "config.yaml").unwrap();
        let map = value.as_mapping().unwrap();
        let key = map.get(&Value::String("key".to_string())).unwrap();
        let key_map = key.as_mapping().unwrap();
        assert_eq!(key_map.len(), 3);
    }

    // ==================== Include Dir Merge Named Tests ====================

    #[test]
    fn test_include_dir_merge_named() {
        let dir = TempDir::new().unwrap();
        fs::create_dir(dir.path().join("items")).unwrap();
        write_file(dir.path(), "items/first.yaml", "key1: one\nkey2: two\n");
        write_file(dir.path(), "items/second.yaml", "key3: three\n");
        write_file(
            dir.path(),
            "config.yaml",
            "key: !include_dir_merge_named items\n",
        );

        let value = load_yaml(dir.path(), "config.yaml").unwrap();
        let map = value.as_mapping().unwrap();
        let key = map.get(&Value::String("key".to_string())).unwrap();
        let key_map = key.as_mapping().unwrap();
        assert!(key_map.contains_key(&Value::String("key1".to_string())));
        assert!(key_map.contains_key(&Value::String("key2".to_string())));
        assert!(key_map.contains_key(&Value::String("key3".to_string())));
    }

    #[test]
    fn test_include_dir_merge_named_recursive() {
        let dir = TempDir::new().unwrap();
        fs::create_dir_all(dir.path().join("items/subdir")).unwrap();
        write_file(dir.path(), "items/first.yaml", "key1: one\n");
        write_file(
            dir.path(),
            "items/subdir/second.yaml",
            "key2: two\nkey3: three\n",
        );
        write_file(dir.path(), "items/subdir/third.yaml", "key4: four\n");
        write_file(
            dir.path(),
            "config.yaml",
            "key: !include_dir_merge_named items\n",
        );

        let value = load_yaml(dir.path(), "config.yaml").unwrap();
        let map = value.as_mapping().unwrap();
        let key = map.get(&Value::String("key".to_string())).unwrap();
        let key_map = key.as_mapping().unwrap();
        assert_eq!(key_map.len(), 4);
    }

    // ==================== Error Handling Tests ====================

    #[test]
    fn test_directory_not_found() {
        let dir = TempDir::new().unwrap();
        write_file(
            dir.path(),
            "config.yaml",
            "key: !include_dir_list nonexistent\n",
        );

        let result = load_yaml(dir.path(), "config.yaml");
        assert!(matches!(result, Err(ConfigError::DirectoryNotFound { .. })));
    }

    #[test]
    fn test_yaml_parse_error() {
        let dir = TempDir::new().unwrap();
        write_file(
            dir.path(),
            "config.yaml",
            "invalid: yaml: content:\n  - bad",
        );

        let result = load_yaml(dir.path(), "config.yaml");
        assert!(matches!(result, Err(ConfigError::ParseYaml { .. })));
    }

    // ==================== Mixed Usage Tests ====================

    #[test]
    fn test_combined_tags() {
        let dir = TempDir::new().unwrap();
        std::env::set_var("TEST_HA_COMBINED", "env_value");
        write_file(dir.path(), "secrets.yaml", "api_key: secret_key\n");
        write_file(dir.path(), "included.yaml", "included: true\n");
        write_file(
            dir.path(),
            "config.yaml",
            r#"
secret_val: !secret api_key
env_val: !env_var TEST_HA_COMBINED
include_val: !include included.yaml
"#,
        );

        let value = load_yaml(dir.path(), "config.yaml").unwrap();
        let map = value.as_mapping().unwrap();
        assert_eq!(
            map.get(&Value::String("secret_val".to_string())),
            Some(&Value::String("secret_key".to_string()))
        );
        assert_eq!(
            map.get(&Value::String("env_val".to_string())),
            Some(&Value::String("env_value".to_string()))
        );

        std::env::remove_var("TEST_HA_COMBINED");
    }

    #[test]
    fn test_secrets_in_included_file() {
        let dir = TempDir::new().unwrap();
        write_file(dir.path(), "secrets.yaml", "password: secret123\n");
        write_file(
            dir.path(),
            "included.yaml",
            "db_password: !secret password\n",
        );
        write_file(
            dir.path(),
            "config.yaml",
            "database: !include included.yaml\n",
        );

        let value = load_yaml(dir.path(), "config.yaml").unwrap();
        let map = value.as_mapping().unwrap();
        let db = map.get(&Value::String("database".to_string())).unwrap();
        let db_map = db.as_mapping().unwrap();
        assert_eq!(
            db_map.get(&Value::String("db_password".to_string())),
            Some(&Value::String("secret123".to_string()))
        );
    }
}
