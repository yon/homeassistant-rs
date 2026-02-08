//! Shared error types for area, floor, and label registries.

use thiserror::Error;

/// Errors for area, floor, and label registry operations.
#[derive(Debug, Error)]
pub enum RegistryError {
    #[error("The name {name} ({normalized}) is already in use")]
    DuplicateName { name: String, normalized: String },

    #[error("{kind} not found: {id}")]
    NotFound { kind: &'static str, id: String },
}

/// Convenience alias for registry operations.
pub type RegistryResult<T> = Result<T, RegistryError>;
