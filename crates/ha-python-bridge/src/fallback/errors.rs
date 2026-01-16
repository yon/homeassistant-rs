//! Error types for the fallback module

use thiserror::Error;

/// Errors that can occur in the Python fallback system
#[derive(Debug, Error)]
pub enum FallbackError {
    /// Python interpreter error
    #[error("Python error: {0}")]
    Python(String),

    /// Integration not found
    #[error("Integration not found: {0}")]
    IntegrationNotFound(String),

    /// Integration failed to load
    #[error("Failed to load integration '{domain}': {reason}")]
    IntegrationLoadFailed { domain: String, reason: String },

    /// Component not implemented in Rust and no Python fallback available
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

impl From<pyo3::PyErr> for FallbackError {
    fn from(err: pyo3::PyErr) -> Self {
        FallbackError::Python(err.to_string())
    }
}

/// Result type for fallback operations
pub type FallbackResult<T> = Result<T, FallbackError>;
