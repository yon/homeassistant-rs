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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_context() {
        let ctx = Context::new();
        assert!(!ctx.id.is_empty());
        assert!(ctx.user_id.is_none());
        assert!(ctx.parent_id.is_none());
    }

    #[test]
    fn test_context_with_user() {
        let ctx = Context::with_user("user123");
        assert_eq!(ctx.user_id, Some("user123".to_string()));
        assert!(ctx.parent_id.is_none());
    }

    #[test]
    fn test_child_context() {
        let parent = Context::with_user("user123");
        let child = parent.child();

        assert_ne!(child.id, parent.id);
        assert_eq!(child.user_id, parent.user_id);
        assert_eq!(child.parent_id, Some(parent.id.clone()));
    }

    #[test]
    fn test_child_with_different_user() {
        let parent = Context::with_user("user123");
        let child = parent.child_with_user("user456");

        assert_eq!(child.user_id, Some("user456".to_string()));
        assert_eq!(child.parent_id, Some(parent.id.clone()));
    }

    #[test]
    fn test_unique_ids() {
        let ctx1 = Context::new();
        let ctx2 = Context::new();
        assert_ne!(ctx1.id, ctx2.id);
    }

    #[test]
    fn test_serde_roundtrip() {
        let ctx = Context::with_user("test_user");
        let json = serde_json::to_string(&ctx).unwrap();
        let parsed: Context = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, ctx.id);
        assert_eq!(parsed.user_id, ctx.user_id);
    }
}
