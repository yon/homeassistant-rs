//! YAML configuration loading for Home Assistant
//!
//! This crate provides YAML configuration loading with support for
//! Home Assistant's custom tags:
//!
//! - `!include path` - Include another YAML file
//! - `!include_dir_list dir` - Include all YAML files in a directory as a list
//! - `!include_dir_merge_list dir` - Merge lists from all YAML files
//! - `!include_dir_named dir` - Include all YAML files as a mapping
//! - `!include_dir_merge_named dir` - Merge mappings from all YAML files
//! - `!secret key` - Substitute from secrets.yaml
//! - `!env_var VAR` - Environment variable substitution
//!
//! # Example
//!
//! ```ignore
//! use ha_config::{load_yaml, YamlLoader};
//!
//! // Load a configuration file
//! let config = load_yaml("/config", "configuration.yaml")?;
//!
//! // Or use the loader directly for more control
//! let mut loader = YamlLoader::new("/config")?;
//! let config = loader.load_file("configuration.yaml")?;
//! ```

mod error;
mod loader;
mod secrets;

pub use error::{ConfigError, ConfigResult};
pub use loader::{load_yaml, load_yaml_string, YamlLoader};
pub use secrets::Secrets;

// Re-export serde_yaml::Value for convenience
pub use serde_yaml::Value;
