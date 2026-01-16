//! Mock entity for testing
//!
//! Provides configurable mock entities for testing entity platforms.

use serde_json::Value;
use std::collections::HashMap;

/// A mock entity for testing
#[derive(Debug, Clone)]
pub struct MockEntity {
    /// The entity ID
    pub entity_id: String,
    /// The entity's unique ID (for registry)
    pub unique_id: Option<String>,
    /// The entity's name
    pub name: Option<String>,
    /// Current state value
    pub state: String,
    /// Entity attributes
    pub attributes: HashMap<String, Value>,
    /// Whether the entity is available
    pub available: bool,
    /// Device ID this entity belongs to
    pub device_id: Option<String>,
    /// Config entry ID this entity belongs to
    pub config_entry_id: Option<String>,
}

impl MockEntity {
    /// Create a new mock entity
    pub fn new(entity_id: impl Into<String>) -> Self {
        Self {
            entity_id: entity_id.into(),
            unique_id: None,
            name: None,
            state: "unknown".to_string(),
            attributes: HashMap::new(),
            available: true,
            device_id: None,
            config_entry_id: None,
        }
    }

    /// Set the unique ID
    pub fn with_unique_id(mut self, unique_id: impl Into<String>) -> Self {
        self.unique_id = Some(unique_id.into());
        self
    }

    /// Set the name
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Set the state
    pub fn with_state(mut self, state: impl Into<String>) -> Self {
        self.state = state.into();
        self
    }

    /// Set an attribute
    pub fn with_attribute(mut self, key: impl Into<String>, value: Value) -> Self {
        self.attributes.insert(key.into(), value);
        self
    }

    /// Set multiple attributes
    pub fn with_attributes(mut self, attributes: HashMap<String, Value>) -> Self {
        self.attributes = attributes;
        self
    }

    /// Set availability
    pub fn with_available(mut self, available: bool) -> Self {
        self.available = available;
        self
    }

    /// Make the entity unavailable
    pub fn unavailable(mut self) -> Self {
        self.available = false;
        self.state = "unavailable".to_string();
        self
    }

    /// Set the device ID
    pub fn with_device_id(mut self, device_id: impl Into<String>) -> Self {
        self.device_id = Some(device_id.into());
        self
    }

    /// Set the config entry ID
    pub fn with_config_entry_id(mut self, config_entry_id: impl Into<String>) -> Self {
        self.config_entry_id = Some(config_entry_id.into());
        self
    }

    /// Get the domain from the entity ID
    pub fn domain(&self) -> &str {
        self.entity_id.split('.').next().unwrap_or("")
    }
}

impl Default for MockEntity {
    fn default() -> Self {
        Self::new("test.mock_entity")
    }
}

/// A mock toggle entity (light, switch, etc.)
#[derive(Debug, Clone)]
pub struct MockToggleEntity {
    /// Base entity
    pub entity: MockEntity,
    /// Whether the entity is on
    pub is_on: bool,
}

impl MockToggleEntity {
    /// Create a new mock toggle entity
    pub fn new(entity_id: impl Into<String>) -> Self {
        let entity_id = entity_id.into();
        Self {
            entity: MockEntity::new(&entity_id).with_state("off"),
            is_on: false,
        }
    }

    /// Turn the entity on
    pub fn turn_on(mut self) -> Self {
        self.is_on = true;
        self.entity.state = "on".to_string();
        self
    }

    /// Turn the entity off
    pub fn turn_off(mut self) -> Self {
        self.is_on = false;
        self.entity.state = "off".to_string();
        self
    }

    /// Set the unique ID
    pub fn with_unique_id(mut self, unique_id: impl Into<String>) -> Self {
        self.entity = self.entity.with_unique_id(unique_id);
        self
    }

    /// Set the name
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.entity = self.entity.with_name(name);
        self
    }
}

impl Default for MockToggleEntity {
    fn default() -> Self {
        Self::new("switch.mock_toggle")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_mock_entity() {
        let entity = MockEntity::new("light.living_room")
            .with_unique_id("abc123")
            .with_name("Living Room Light")
            .with_state("on")
            .with_attribute("brightness", json!(255));

        assert_eq!(entity.entity_id, "light.living_room");
        assert_eq!(entity.unique_id, Some("abc123".to_string()));
        assert_eq!(entity.name, Some("Living Room Light".to_string()));
        assert_eq!(entity.state, "on");
        assert_eq!(entity.attributes.get("brightness"), Some(&json!(255)));
        assert_eq!(entity.domain(), "light");
    }

    #[test]
    fn test_mock_entity_unavailable() {
        let entity = MockEntity::new("sensor.temp").unavailable();

        assert!(!entity.available);
        assert_eq!(entity.state, "unavailable");
    }

    #[test]
    fn test_mock_toggle_entity() {
        let entity = MockToggleEntity::new("switch.test");
        assert!(!entity.is_on);
        assert_eq!(entity.entity.state, "off");

        let entity = entity.turn_on();
        assert!(entity.is_on);
        assert_eq!(entity.entity.state, "on");

        let entity = entity.turn_off();
        assert!(!entity.is_on);
        assert_eq!(entity.entity.state, "off");
    }
}
