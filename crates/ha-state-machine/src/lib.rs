//! State machine with domain indexing for Home Assistant
//!
//! This crate provides the StateMachine, which tracks the current state of
//! all entities in Home Assistant. It maintains indices by domain for
//! efficient queries and fires STATE_CHANGED events on the event bus.

use dashmap::DashMap;
use ha_core::events::StateChangedData;
use ha_core::{Context, EntityId, State};
use ha_event_bus::EventBus;
use std::sync::Arc;
use tracing::{debug, instrument, trace};

/// The state machine tracks all entity states
///
/// The StateMachine is responsible for:
/// - Storing the current state of all entities
/// - Maintaining a domain index for efficient domain-based queries
/// - Firing STATE_CHANGED events when states change
/// - Providing thread-safe concurrent access to states
pub struct StateMachine {
    /// All entity states keyed by entity_id string
    states: DashMap<String, State>,
    /// Index of entity_ids by domain
    domain_index: DashMap<String, Vec<String>>,
    /// Event bus for firing state change events
    event_bus: Arc<EventBus>,
}

impl StateMachine {
    /// Create a new state machine with the given event bus
    pub fn new(event_bus: Arc<EventBus>) -> Self {
        Self {
            states: DashMap::new(),
            domain_index: DashMap::new(),
            event_bus,
        }
    }

    /// Set the state of an entity
    ///
    /// If the entity already has a state, the `last_changed` timestamp will
    /// only be updated if the state value actually changed.
    ///
    /// Fires a STATE_CHANGED event with the old and new state.
    #[instrument(skip(self, state, attributes, context), fields(entity_id = %entity_id))]
    pub fn set(
        &self,
        entity_id: EntityId,
        state: impl Into<String>,
        attributes: std::collections::HashMap<String, serde_json::Value>,
        context: Context,
    ) -> State {
        let entity_id_str = entity_id.to_string();
        let domain = entity_id.domain().to_string();

        let old_state = self.states.get(&entity_id_str).map(|s| s.clone());

        let new_state = match &old_state {
            Some(existing) => existing.with_update(state, attributes, context.clone()),
            None => State::new(entity_id.clone(), state, attributes, context.clone()),
        };

        debug!(
            state = %new_state.state,
            changed = old_state.as_ref().map(|s| s.state != new_state.state).unwrap_or(true),
            "Setting entity state"
        );

        // Update state
        self.states.insert(entity_id_str.clone(), new_state.clone());

        // Update domain index if this is a new entity
        if old_state.is_none() {
            self.domain_index
                .entry(domain)
                .or_default()
                .push(entity_id_str);
        }

        // Fire STATE_CHANGED event
        let event_data = StateChangedData {
            entity_id,
            old_state,
            new_state: Some(new_state.clone()),
        };
        self.event_bus.fire_typed(event_data, context);

        new_state
    }

    /// Get the current state of an entity
    pub fn get(&self, entity_id: &str) -> Option<State> {
        self.states.get(entity_id).map(|s| s.clone())
    }

    /// Get the state value as a string, or None if entity doesn't exist
    pub fn get_state(&self, entity_id: &str) -> Option<String> {
        self.states.get(entity_id).map(|s| s.state.clone())
    }

    /// Check if an entity is in a specific state
    pub fn is_state(&self, entity_id: &str, state: &str) -> bool {
        self.get_state(entity_id).as_deref() == Some(state)
    }

    /// Get all entity IDs for a domain
    pub fn entity_ids(&self, domain: &str) -> Vec<String> {
        self.domain_index
            .get(domain)
            .map(|v| v.clone())
            .unwrap_or_default()
    }

    /// Get all states for a domain
    pub fn domain_states(&self, domain: &str) -> Vec<State> {
        self.entity_ids(domain)
            .iter()
            .filter_map(|id| self.get(id))
            .collect()
    }

    /// Get all entity IDs
    pub fn all_entity_ids(&self) -> Vec<String> {
        self.states.iter().map(|r| r.key().clone()).collect()
    }

    /// Get all states
    pub fn all(&self) -> Vec<State> {
        self.states.iter().map(|r| r.value().clone()).collect()
    }

    /// Get all unique domains
    pub fn domains(&self) -> Vec<String> {
        self.domain_index.iter().map(|r| r.key().clone()).collect()
    }

    /// Remove an entity's state
    ///
    /// Fires a STATE_CHANGED event with the old state and None for new_state.
    #[instrument(skip(self, context), fields(entity_id = %entity_id))]
    pub fn remove(&self, entity_id: &EntityId, context: Context) -> Option<State> {
        let entity_id_str = entity_id.to_string();
        let domain = entity_id.domain();

        let old_state = self.states.remove(&entity_id_str).map(|(_, s)| s);

        if let Some(ref state) = old_state {
            trace!("Removing entity state");

            // Update domain index
            if let Some(mut ids) = self.domain_index.get_mut(domain) {
                ids.retain(|id| id != &entity_id_str);
            }

            // Fire STATE_CHANGED event with None for new_state
            let event_data = StateChangedData {
                entity_id: entity_id.clone(),
                old_state: Some(state.clone()),
                new_state: None,
            };
            self.event_bus.fire_typed(event_data, context);
        }

        old_state
    }

    /// Get the total number of entities
    pub fn entity_count(&self) -> usize {
        self.states.len()
    }
}

/// Thread-safe wrapper for StateMachine
pub type SharedStateMachine = Arc<StateMachine>;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::HashMap;

    fn make_test_setup() -> (Arc<EventBus>, StateMachine) {
        let event_bus = Arc::new(EventBus::new());
        let state_machine = StateMachine::new(event_bus.clone());
        (event_bus, state_machine)
    }

    #[test]
    fn test_set_and_get_state() {
        let (_, sm) = make_test_setup();

        let entity_id = EntityId::new("light", "living_room").unwrap();
        let attrs = HashMap::from([("brightness".to_string(), json!(255))]);
        let ctx = Context::new();

        let state = sm.set(entity_id, "on", attrs.clone(), ctx);

        assert_eq!(state.state, "on");
        assert_eq!(state.attributes, attrs);

        let retrieved = sm.get("light.living_room").unwrap();
        assert_eq!(retrieved.state, "on");
    }

    #[test]
    fn test_is_state() {
        let (_, sm) = make_test_setup();

        let entity_id = EntityId::new("switch", "kitchen").unwrap();
        sm.set(entity_id, "on", HashMap::new(), Context::new());

        assert!(sm.is_state("switch.kitchen", "on"));
        assert!(!sm.is_state("switch.kitchen", "off"));
        assert!(!sm.is_state("switch.nonexistent", "on"));
    }

    #[test]
    fn test_domain_indexing() {
        let (_, sm) = make_test_setup();

        let light1 = EntityId::new("light", "living_room").unwrap();
        let light2 = EntityId::new("light", "bedroom").unwrap();
        let switch1 = EntityId::new("switch", "kitchen").unwrap();

        sm.set(light1, "on", HashMap::new(), Context::new());
        sm.set(light2, "off", HashMap::new(), Context::new());
        sm.set(switch1, "on", HashMap::new(), Context::new());

        let light_ids = sm.entity_ids("light");
        assert_eq!(light_ids.len(), 2);
        assert!(light_ids.contains(&"light.living_room".to_string()));
        assert!(light_ids.contains(&"light.bedroom".to_string()));

        let switch_ids = sm.entity_ids("switch");
        assert_eq!(switch_ids.len(), 1);
        assert!(switch_ids.contains(&"switch.kitchen".to_string()));
    }

    #[test]
    fn test_domains() {
        let (_, sm) = make_test_setup();

        sm.set(
            EntityId::new("light", "test").unwrap(),
            "on",
            HashMap::new(),
            Context::new(),
        );
        sm.set(
            EntityId::new("switch", "test").unwrap(),
            "on",
            HashMap::new(),
            Context::new(),
        );
        sm.set(
            EntityId::new("sensor", "test").unwrap(),
            "23",
            HashMap::new(),
            Context::new(),
        );

        let domains = sm.domains();
        assert_eq!(domains.len(), 3);
        assert!(domains.contains(&"light".to_string()));
        assert!(domains.contains(&"switch".to_string()));
        assert!(domains.contains(&"sensor".to_string()));
    }

    #[test]
    fn test_state_update_preserves_last_changed() {
        let (_, sm) = make_test_setup();

        let entity_id = EntityId::new("sensor", "temp").unwrap();

        // Initial state
        let state1 = sm.set(entity_id.clone(), "20", HashMap::new(), Context::new());

        std::thread::sleep(std::time::Duration::from_millis(10));

        // Update with same value - last_changed should be preserved
        let state2 = sm.set(entity_id.clone(), "20", HashMap::new(), Context::new());

        assert_eq!(state1.last_changed, state2.last_changed);
        assert!(state2.last_updated > state1.last_updated);

        // Update with different value - last_changed should update
        let state3 = sm.set(entity_id, "21", HashMap::new(), Context::new());

        assert!(state3.last_changed > state2.last_changed);
    }

    #[test]
    fn test_remove_state() {
        let (_, sm) = make_test_setup();

        let entity_id = EntityId::new("light", "test").unwrap();
        sm.set(entity_id.clone(), "on", HashMap::new(), Context::new());

        assert!(sm.get("light.test").is_some());

        let removed = sm.remove(&entity_id, Context::new());
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().state, "on");

        assert!(sm.get("light.test").is_none());
        assert!(sm.entity_ids("light").is_empty());
    }

    #[tokio::test]
    async fn test_state_changed_event_fired() {
        let event_bus = Arc::new(EventBus::new());
        let sm = StateMachine::new(event_bus.clone());

        let mut rx = event_bus.subscribe_typed::<StateChangedData>();

        let entity_id = EntityId::new("light", "test").unwrap();
        sm.set(entity_id.clone(), "on", HashMap::new(), Context::new());

        let event = rx.recv().await.unwrap();
        assert_eq!(event.data.entity_id.to_string(), "light.test");
        assert!(event.data.old_state.is_none());
        assert!(event.data.new_state.is_some());
        assert_eq!(event.data.new_state.unwrap().state, "on");
    }
}
