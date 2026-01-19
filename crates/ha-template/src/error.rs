//! Error types for template rendering

use std::path::PathBuf;
use thiserror::Error;

/// Result type for template operations
pub type TemplateResult<T> = Result<T, TemplateError>;

/// Errors that can occur during template rendering
#[derive(Debug, Error)]
pub enum TemplateError {
    /// Failed to compile template
    #[error("failed to compile template: {message}")]
    CompileError { message: String },

    /// Failed to render template
    #[error("failed to render template: {message}")]
    RenderError { message: String },

    /// Invalid template syntax
    #[error("invalid template syntax: {message}")]
    SyntaxError { message: String },

    /// Undefined variable in template
    #[error("undefined variable: {name}")]
    UndefinedVariable { name: String },

    /// Type error in template
    #[error("type error: {message}")]
    TypeError { message: String },

    /// Invalid argument to function
    #[error("invalid argument to {function}: {message}")]
    InvalidArgument { function: String, message: String },

    /// IO error reading template file
    #[error("failed to read template from {}: {source}", path.display())]
    IoError {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// Invalid path (e.g., no filename)
    #[error("invalid template path: {}", path.display())]
    InvalidPath { path: PathBuf },

    /// Failed to parse a template file
    #[error("failed to parse template '{}': {message}", name)]
    ParseError { name: String, message: String },
}

impl From<minijinja::Error> for TemplateError {
    fn from(err: minijinja::Error) -> Self {
        match err.kind() {
            minijinja::ErrorKind::SyntaxError => TemplateError::SyntaxError {
                message: err.to_string(),
            },
            minijinja::ErrorKind::UndefinedError => TemplateError::UndefinedVariable {
                name: err.to_string(),
            },
            _ => TemplateError::RenderError {
                message: err.to_string(),
            },
        }
    }
}
