//! Error types for the Home Assistant API crate.

use thiserror::Error;

/// Errors that can occur during WebSocket message handling.
#[derive(Debug, Error)]
pub enum WebSocketError {
    /// Failed to send a message through the response channel
    #[error("channel send failed: {0}")]
    ChannelSend(String),

    /// Config flow manager is not available (Python bridge not loaded)
    #[error("config flow manager not available")]
    ConfigFlowUnavailable,

    /// WebSocket connection was closed unexpectedly
    #[error("connection closed")]
    ConnectionClosed,

    /// Client sent a non-monotonic message ID
    #[error("message ID {0} is not greater than the last received ID")]
    InvalidId(u64),

    /// Failed to deserialize an incoming message
    #[error("invalid message format: {0}")]
    InvalidMessage(String),

    /// Received a non-auth message during auth phase
    #[error("expected auth message")]
    NotAuthenticated,

    /// JSON serialization failed when sending a response
    #[error("serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),

    /// WebSocket transport error
    #[error("transport error: {0}")]
    Transport(String),
}

/// Result type alias for WebSocket operations.
pub type WsResult<T> = Result<T, WebSocketError>;
