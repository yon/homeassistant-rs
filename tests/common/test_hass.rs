//! Test Home Assistant instance
//!
//! Provides an isolated HomeAssistant instance for testing with
//! captured events and service calls.

use ha_core::{Context, EntityId, Event, ServiceCall, State};
use ha_event_bus::EventBus;
use ha_service_registry::ServiceRegistry;
use ha_state_store::StateStore;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// A test instance of Home Assistant with captured events and service calls
pub struct TestHomeAssistant {
    /// Event bus for pub/sub communication
    pub bus: Arc<EventBus>,
    /// State machine for entity states
    pub states: Arc<StateStore>,
    /// Service registry for service calls
    pub services: Arc<ServiceRegistry>,
    /// Captured events for assertions
    captured_events: Arc<Mutex<Vec<Event<serde_json::Value>>>>,
    /// Captured service calls for assertions
    captured_service_calls: Arc<Mutex<Vec<ServiceCall>>>,
}

impl TestHomeAssistant {
    /// Create a new test Home Assistant instance
    pub fn new() -> Self {
        let bus = Arc::new(EventBus::new());
        let states = Arc::new(StateStore::new(bus.clone()));
        let services = Arc::new(ServiceRegistry::new());

        Self {
            bus,
            states,
            services,
            captured_events: Arc::new(Mutex::new(Vec::new())),
            captured_service_calls: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Set the state of an entity
    pub fn set_state(
        &self,
        entity_id: &str,
        state: &str,
        attributes: HashMap<String, serde_json::Value>,
    ) -> State {
        let entity_id: EntityId = entity_id.parse().expect("Invalid entity_id");
        self.states.set(entity_id, state, attributes, Context::new())
    }

    /// Get the state of an entity
    pub fn get_state(&self, entity_id: &str) -> Option<State> {
        self.states.get(entity_id)
    }

    /// Assert that an entity is in a specific state
    pub fn assert_state(&self, entity_id: &str, expected: &str) {
        let state = self.states.get_state(entity_id);
        assert_eq!(
            state.as_deref(),
            Some(expected),
            "Expected entity {} to be in state '{}', but was {:?}",
            entity_id,
            expected,
            state
        );
    }

    /// Get all captured events
    pub fn captured_events(&self) -> Vec<Event<serde_json::Value>> {
        self.captured_events.lock().unwrap().clone()
    }

    /// Get all captured service calls
    pub fn captured_service_calls(&self) -> Vec<ServiceCall> {
        self.captured_service_calls.lock().unwrap().clone()
    }

    /// Get captured service calls for a specific domain
    pub fn service_calls(&self, domain: &str) -> Vec<ServiceCall> {
        self.captured_service_calls
            .lock()
            .unwrap()
            .iter()
            .filter(|c| c.domain == domain)
            .cloned()
            .collect()
    }

    /// Clear all captured events
    pub fn clear_events(&self) {
        self.captured_events.lock().unwrap().clear();
    }

    /// Clear all captured service calls
    pub fn clear_service_calls(&self) {
        self.captured_service_calls.lock().unwrap().clear();
    }
}

impl Default for TestHomeAssistant {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_set_and_get_state() {
        let hass = TestHomeAssistant::new();

        hass.set_state(
            "light.test",
            "on",
            HashMap::from([("brightness".to_string(), json!(255))]),
        );

        let state = hass.get_state("light.test").unwrap();
        assert_eq!(state.state, "on");
        assert_eq!(state.attributes.get("brightness"), Some(&json!(255)));
    }

    #[test]
    fn test_assert_state() {
        let hass = TestHomeAssistant::new();
        hass.set_state("switch.test", "on", HashMap::new());
        hass.assert_state("switch.test", "on");
    }

    #[test]
    #[should_panic(expected = "Expected entity switch.test to be in state 'off'")]
    fn test_assert_state_fails() {
        let hass = TestHomeAssistant::new();
        hass.set_state("switch.test", "on", HashMap::new());
        hass.assert_state("switch.test", "off");
    }
}
