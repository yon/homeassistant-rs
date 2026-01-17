//! State type representing an entity's current state

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{Context, EntityId};

/// Represents the state of an entity at a point in time
///
/// State includes the entity's current value (as a string), any associated
/// attributes, and timestamps for when the state was last changed and updated.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct State {
    /// The entity this state belongs to
    pub entity_id: EntityId,

    /// The state value (e.g., "on", "off", "23.5", "unavailable")
    pub state: String,

    /// Additional attributes associated with the state
    #[serde(default)]
    pub attributes: HashMap<String, serde_json::Value>,

    /// When the state was last changed (different from previous state)
    pub last_changed: DateTime<Utc>,

    /// When the state was last updated (even if value didn't change)
    pub last_updated: DateTime<Utc>,

    /// When the state was last reported by the integration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_reported: Option<DateTime<Utc>>,

    /// Context of the change that created this state
    pub context: Context,
}

impl State {
    /// Create a new state with current timestamp
    pub fn new(
        entity_id: EntityId,
        state: impl Into<String>,
        attributes: HashMap<String, serde_json::Value>,
        context: Context,
    ) -> Self {
        let now = Utc::now();
        Self {
            entity_id,
            state: state.into(),
            attributes,
            last_changed: now,
            last_updated: now,
            last_reported: Some(now),
            context,
        }
    }

    /// Create an updated state, preserving last_changed if state value is the same
    pub fn with_update(
        &self,
        new_state: impl Into<String>,
        new_attributes: HashMap<String, serde_json::Value>,
        context: Context,
    ) -> Self {
        let now = Utc::now();
        let new_state = new_state.into();
        let state_changed = self.state != new_state;

        Self {
            entity_id: self.entity_id.clone(),
            state: new_state,
            attributes: new_attributes,
            last_changed: if state_changed {
                now
            } else {
                self.last_changed
            },
            last_updated: now,
            last_reported: Some(now),
            context,
        }
    }

    /// Check if the state value represents an unavailable entity
    pub fn is_unavailable(&self) -> bool {
        self.state == "unavailable"
    }

    /// Check if the state value represents an unknown state
    pub fn is_unknown(&self) -> bool {
        self.state == "unknown"
    }

    /// Get an attribute value by key
    pub fn attribute<T: serde::de::DeserializeOwned>(&self, key: &str) -> Option<T> {
        self.attributes
            .get(key)
            .and_then(|v| serde_json::from_value(v.clone()).ok())
    }
}

impl PartialEq for State {
    fn eq(&self, other: &Self) -> bool {
        // Two states are equal if they have the same entity_id, state value, and attributes
        // Timestamps and context are not compared
        self.entity_id == other.entity_id
            && self.state == other.state
            && self.attributes == other.attributes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use serde_json::json;

    fn make_entity_id() -> EntityId {
        EntityId::new("light", "test").unwrap()
    }

    #[test]
    fn test_new_state() {
        let entity_id = make_entity_id();
        let attrs = HashMap::from([("brightness".to_string(), json!(255))]);
        let ctx = Context::new();

        let state = State::new(entity_id.clone(), "on", attrs.clone(), ctx);

        assert_eq!(state.entity_id, entity_id);
        assert_eq!(state.state, "on");
        assert_eq!(state.attributes, attrs);
        assert_eq!(state.last_changed, state.last_updated);
    }

    #[test]
    fn test_state_update_same_value() {
        let entity_id = make_entity_id();
        let ctx1 = Context::new();
        let state1 = State::new(entity_id, "on", HashMap::new(), ctx1);

        // Small delay to ensure timestamps differ
        std::thread::sleep(std::time::Duration::from_millis(10));

        let ctx2 = Context::new();
        let state2 = state1.with_update("on", HashMap::new(), ctx2);

        // State value same, so last_changed should be preserved
        assert_eq!(state2.last_changed, state1.last_changed);
        assert!(state2.last_updated > state1.last_updated);
    }

    #[test]
    fn test_state_update_different_value() {
        let entity_id = make_entity_id();
        let ctx1 = Context::new();
        let state1 = State::new(entity_id, "on", HashMap::new(), ctx1);

        std::thread::sleep(std::time::Duration::from_millis(10));

        let ctx2 = Context::new();
        let state2 = state1.with_update("off", HashMap::new(), ctx2);

        // State value changed, so last_changed should be updated
        assert!(state2.last_changed > state1.last_changed);
        assert!(state2.last_updated > state1.last_updated);
    }

    #[test]
    fn test_unavailable_and_unknown() {
        let entity_id = make_entity_id();
        let ctx = Context::new();

        let unavailable = State::new(
            entity_id.clone(),
            "unavailable",
            HashMap::new(),
            ctx.clone(),
        );
        assert!(unavailable.is_unavailable());
        assert!(!unavailable.is_unknown());

        let unknown = State::new(entity_id, "unknown", HashMap::new(), ctx);
        assert!(!unknown.is_unavailable());
        assert!(unknown.is_unknown());
    }

    #[test]
    fn test_get_attribute() {
        let entity_id = make_entity_id();
        let attrs = HashMap::from([
            ("brightness".to_string(), json!(200)),
            ("color_temp".to_string(), json!(4000)),
            ("friendly_name".to_string(), json!("Test Light")),
        ]);
        let ctx = Context::new();

        let state = State::new(entity_id, "on", attrs, ctx);

        assert_eq!(state.attribute::<i32>("brightness"), Some(200));
        assert_eq!(
            state.attribute::<String>("friendly_name"),
            Some("Test Light".to_string())
        );
        assert_eq!(state.attribute::<i32>("nonexistent"), None);
    }

    #[test]
    fn test_state_equality() {
        let entity_id = make_entity_id();
        let attrs = HashMap::from([("brightness".to_string(), json!(255))]);

        let state1 = State::new(entity_id.clone(), "on", attrs.clone(), Context::new());
        std::thread::sleep(std::time::Duration::from_millis(10));
        let state2 = State::new(entity_id, "on", attrs, Context::new());

        // States are equal even with different timestamps/contexts
        assert_eq!(state1, state2);
    }
}
