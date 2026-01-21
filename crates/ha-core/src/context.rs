//! Context type for tracking request origin and causality

use serde::{Deserialize, Serialize};
use ulid::Ulid;

/// Context for tracking the origin and causality of events and service calls
///
/// Every event and service call in Home Assistant carries a Context that
/// identifies who initiated the action and allows tracing the chain of
/// actions that resulted from it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Context {
    /// Unique identifier for this context (ULID)
    pub id: String,

    /// User ID that initiated this action (if any)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,

    /// Parent context ID for tracking causality chains
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
}

impl Context {
    /// Create a new context with a fresh ULID
    pub fn new() -> Self {
        Self {
            id: Ulid::new().to_string(),
            user_id: None,
            parent_id: None,
        }
    }

    /// Create a new context with a specific ID
    pub fn with_id(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            user_id: None,
            parent_id: None,
        }
    }

    /// Create a new context with a specific user
    pub fn with_user(user_id: impl Into<String>) -> Self {
        Self {
            id: Ulid::new().to_string(),
            user_id: Some(user_id.into()),
            parent_id: None,
        }
    }

    /// Create a child context with this context as parent
    pub fn child(&self) -> Self {
        Self {
            id: Ulid::new().to_string(),
            user_id: self.user_id.clone(),
            parent_id: Some(self.id.clone()),
        }
    }

    /// Create a child context with a different user
    pub fn child_with_user(&self, user_id: impl Into<String>) -> Self {
        Self {
            id: Ulid::new().to_string(),
            user_id: Some(user_id.into()),
            parent_id: Some(self.id.clone()),
        }
    }
}

impl Default for Context {
    fn default() -> Self {
        Self::new()
    }
}

// Unit tests removed - covered by HA native tests via `make ha-compat-test`
// See tests/ha_compat/ for comprehensive Context testing through Python bindings
