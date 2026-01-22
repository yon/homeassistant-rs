//! Entity state storage with domain indexing for Home Assistant
//!
//! This crate provides the StateStore, which tracks the current state of
//! all entities in Home Assistant. It maintains indices by domain for
//! efficient queries and fires STATE_CHANGED events on the event bus.

use dashmap::DashMap;
use ha_core::events::{StateChangedData, StateReportedData};
use ha_core::{Context, EntityId, State, MAX_STATE_LENGTH, STATE_UNKNOWN};
use ha_event_bus::EventBus;
use std::sync::Arc;
use tracing::{debug, instrument, trace, warn};

/// The state store tracks all entity states
///
/// The StateStore is responsible for:
/// - Storing the current state of all entities
/// - Maintaining a domain index for efficient domain-based queries
/// - Firing STATE_CHANGED events when states change
/// - Providing thread-safe concurrent access to states
pub struct StateStore {
    /// All entity states keyed by entity_id string
    states: DashMap<String, State>,
    /// Index of entity_ids by domain
    domain_index: DashMap<String, Vec<String>>,
    /// Event bus for firing state change events
    event_bus: Arc<EventBus>,
}

impl StateStore {
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
    /// only be updated if the state value actually changed (or `force_update` is true).
    ///
    /// If the state is unchanged and attributes are the same, fires a STATE_REPORTED
    /// event instead of STATE_CHANGED. If `force_update` is true, always fires
    /// STATE_CHANGED and updates `last_changed`.
    ///
    /// If the state value exceeds MAX_STATE_LENGTH (255), it is replaced with
    /// STATE_UNKNOWN ("unknown") and a warning is logged.
    #[instrument(skip(self, state, attributes, context), fields(entity_id = %entity_id))]
    pub fn set(
        &self,
        entity_id: EntityId,
        state: impl Into<String>,
        attributes: std::collections::HashMap<String, serde_json::Value>,
        context: Context,
    ) -> State {
        self.set_internal(entity_id, state, attributes, context, false)
    }

    /// Set the state of an entity with force_update option
    ///
    /// When `force_update` is true, `last_changed` is always updated even if
    /// the state value hasn't changed.
    #[instrument(skip(self, state, attributes, context), fields(entity_id = %entity_id))]
    pub fn set_with_force(
        &self,
        entity_id: EntityId,
        state: impl Into<String>,
        attributes: std::collections::HashMap<String, serde_json::Value>,
        context: Context,
        force_update: bool,
    ) -> State {
        self.set_internal(entity_id, state, attributes, context, force_update)
    }

    /// Internal implementation of state setting
    fn set_internal(
        &self,
        entity_id: EntityId,
        state: impl Into<String>,
        attributes: std::collections::HashMap<String, serde_json::Value>,
        context: Context,
        force_update: bool,
    ) -> State {
        let entity_id_str = entity_id.to_string();
        let domain = entity_id.domain().to_string();
        let mut state_str = state.into();

        let old_state = self.states.get(&entity_id_str).map(|s| s.clone());

        // Check if state and attributes are the same
        let same_state = old_state
            .as_ref()
            .map(|s| s.state == state_str && !force_update)
            .unwrap_or(false);
        let same_attr = old_state
            .as_ref()
            .map(|s| s.attributes == attributes)
            .unwrap_or(false);

        // If state and attributes are unchanged, fire STATE_REPORTED and return
        if same_state && same_attr {
            let existing = old_state.as_ref().unwrap();
            let old_last_reported = existing.last_reported;
            let now = chrono::Utc::now();

            // Update last_reported on the existing state
            let mut updated = existing.clone();
            updated.last_reported = Some(now);
            self.states.insert(entity_id_str.clone(), updated.clone());

            debug!(
                state = %updated.state,
                "State unchanged, firing STATE_REPORTED"
            );

            // Fire STATE_REPORTED event
            let event_data = StateReportedData {
                entity_id,
                new_state: updated.clone(),
                old_last_reported,
                last_reported: now,
            };
            self.event_bus.fire_typed(event_data, context);

            return updated;
        }

        // Validate state length - replace with STATE_UNKNOWN if too long
        if !same_state && state_str.len() > MAX_STATE_LENGTH {
            warn!(
                entity_id = %entity_id,
                state_length = state_str.len(),
                max_length = MAX_STATE_LENGTH,
                "State exceeds maximum length, falling back to unknown"
            );
            state_str = STATE_UNKNOWN.to_string();
        }

        let new_state = match &old_state {
            Some(existing) => {
                if force_update {
                    // Force update: always update last_changed
                    State::new(entity_id.clone(), state_str, attributes, context.clone())
                } else {
                    existing.with_update(state_str, attributes, context.clone())
                }
            }
            None => State::new(entity_id.clone(), state_str, attributes, context.clone()),
        };

        debug!(
            state = %new_state.state,
            changed = old_state.as_ref().map(|s| s.state != new_state.state).unwrap_or(true),
            force_update = force_update,
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

/// Thread-safe wrapper for StateStore
pub type SharedStateStore = Arc<StateStore>;

// Unit tests removed - covered by HA native tests via `make ha-compat-test`
// See tests/ha_compat/ for comprehensive StateStore testing through Python bindings
