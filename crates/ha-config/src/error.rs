//! Error types for configuration loading

use std::path::PathBuf;
use thiserror::Error;

/// Result type for configuration operations
pub type ConfigResult<T> = Result<T, ConfigError>;

/// Errors that can occur during configuration loading
#[derive(Debug, Error)]
pub enum ConfigError {
    /// Failed to read a file
    #[error("failed to read file {path}: {source}")]
    ReadFile {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// Failed to parse YAML
    #[error("failed to parse YAML in {path}: {source}")]
    ParseYaml {
        path: PathBuf,
        #[source]
        source: serde_yaml::Error,
    },

    /// Secret not found
    #[error("secret '{key}' not found in secrets.yaml")]
    SecretNotFound { key: String },

    /// Secrets file not found
    #[error("secrets.yaml not found at {path}")]
    SecretsFileNotFound { path: PathBuf },

    /// Invalid include path
    #[error("invalid include path '{path}': {reason}")]
    InvalidIncludePath { path: String, reason: String },

    /// Include file not found
    #[error("included file not found: {path}")]
    IncludeNotFound { path: PathBuf },

    /// Directory not found for include_dir_*
    #[error("directory not found: {path}")]
    DirectoryNotFound { path: PathBuf },

    /// Circular include detected
    #[error("circular include detected: {path}")]
    CircularInclude { path: PathBuf },

    /// Environment variable not found
    #[error("environment variable '{var}' not set")]
    EnvVarNotFound { var: String },

    /// Invalid configuration value
    #[error("invalid configuration value for '{key}': {reason}")]
    InvalidValue { key: String, reason: String },

    /// Configuration validation failed
    #[error("configuration validation failed: {message}")]
    ValidationFailed { message: String },
}
