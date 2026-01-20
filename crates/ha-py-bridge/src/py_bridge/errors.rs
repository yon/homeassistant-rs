//! Error types for the Python bridge module

use thiserror::Error;

/// Errors that can occur in the Python bridge system
#[derive(Debug, Error)]
pub enum PyBridgeError {
    /// Python interpreter error
    #[error("Python error: {0}")]
    Python(String),

    /// Integration not found
    #[error("Integration not found: {0}")]
    IntegrationNotFound(String),

    /// Integration failed to load
    #[error("Failed to load integration '{domain}': {reason}")]
    IntegrationLoadFailed { domain: String, reason: String },

    /// Integration not allowed (not whitelisted or Rust-blocked)
    #[error("Integration '{0}' not allowed")]
    IntegrationNotAllowed(String),

    /// Component not implemented in Rust and no Python available
    #[error("Component '{0}' not implemented")]
    NotImplemented(String),

    /// Async bridge error
    #[error("Async bridge error: {0}")]
    AsyncBridge(String),

    /// Configuration error
    #[error("Configuration error: {0}")]
    Config(String),

    /// Service call error
    #[error("Service call failed: {0}")]
    ServiceCall(String),
}

impl From<pyo3::PyErr> for PyBridgeError {
    fn from(err: pyo3::PyErr) -> Self {
        PyBridgeError::Python(err.to_string())
    }
}

/// Result type for Python bridge operations
pub type PyBridgeResult<T> = Result<T, PyBridgeError>;
