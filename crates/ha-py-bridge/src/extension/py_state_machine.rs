//! Python wrapper for StateMachine

use std::sync::Arc;

use ha_core::EntityId;
use ha_event_bus::EventBus;
use ha_state_machine::StateMachine;
use pyo3::prelude::*;
use pyo3::types::PyDict;

use super::py_types::{py_dict_to_hashmap, PyContext, PyState};

/// Python wrapper for StateMachine
#[pyclass(name = "StateMachine")]
pub struct PyStateMachine {
    inner: Arc<StateMachine>,
}

#[pymethods]
impl PyStateMachine {
    /// Set the state of an entity
    ///
    /// Args:
    ///     entity_id: The entity ID (e.g., "light.living_room")
    ///     state: The new state value
    ///     attributes: Optional attributes dictionary
    ///     context: Optional context for the state change
    ///
    /// Returns:
    ///     The new State object
    #[pyo3(signature = (entity_id, state, attributes=None, context=None))]
    fn set(
        &self,
        entity_id: &str,
        state: &str,
        attributes: Option<&Bound<'_, PyDict>>,
        context: Option<PyContext>,
    ) -> PyResult<PyState> {
        let entity_id: EntityId = entity_id
            .parse()
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(format!("{}", e)))?;

        let attrs = match attributes {
            Some(dict) => py_dict_to_hashmap(dict)?,
            None => std::collections::HashMap::new(),
        };

        let ctx = context.map(|c| c.into_inner()).unwrap_or_default();
        let state = self.inner.set(entity_id, state, attrs, ctx);

        Ok(PyState::from_inner(state))
    }

    /// Get the current state of an entity
    ///
    /// Args:
    ///     entity_id: The entity ID to look up
    ///
    /// Returns:
    ///     The State object, or None if entity doesn't exist
    fn get(&self, entity_id: &str) -> Option<PyState> {
        self.inner.get(entity_id).map(PyState::from_inner)
    }

    /// Get the state value as a string
    ///
    /// Args:
    ///     entity_id: The entity ID to look up
    ///
    /// Returns:
    ///     The state value, or None if entity doesn't exist
    fn get_state(&self, entity_id: &str) -> Option<String> {
        self.inner.get_state(entity_id)
    }

    /// Check if an entity is in a specific state
    ///
    /// Args:
    ///     entity_id: The entity ID to check
    ///     state: The expected state value
    ///
    /// Returns:
    ///     True if the entity is in the specified state
    fn is_state(&self, entity_id: &str, state: &str) -> bool {
        self.inner.is_state(entity_id, state)
    }

    /// Get all entity IDs for a domain
    ///
    /// Args:
    ///     domain: The domain to filter by (e.g., "light")
    ///
    /// Returns:
    ///     List of entity IDs in the domain
    fn entity_ids(&self, domain: &str) -> Vec<String> {
        self.inner.entity_ids(domain)
    }

    /// Get all states for a domain
    ///
    /// Args:
    ///     domain: The domain to filter by
    ///
    /// Returns:
    ///     List of State objects for the domain
    fn domain_states(&self, domain: &str) -> Vec<PyState> {
        self.inner
            .domain_states(domain)
            .into_iter()
            .map(PyState::from_inner)
            .collect()
    }

    /// Get all entity IDs
    ///
    /// Returns:
    ///     List of all entity IDs
    fn all_entity_ids(&self) -> Vec<String> {
        self.inner.all_entity_ids()
    }

    /// Get all states
    ///
    /// Returns:
    ///     List of all State objects
    fn all(&self) -> Vec<PyState> {
        self.inner
            .all()
            .into_iter()
            .map(PyState::from_inner)
            .collect()
    }

    /// Get all unique domains
    ///
    /// Returns:
    ///     List of domain names
    fn domains(&self) -> Vec<String> {
        self.inner.domains()
    }

    /// Remove an entity's state
    ///
    /// Args:
    ///     entity_id: The entity ID to remove
    ///     context: Optional context for the removal
    ///
    /// Returns:
    ///     The removed State, or None if entity didn't exist
    #[pyo3(signature = (entity_id, context=None))]
    fn remove(&self, entity_id: &str, context: Option<PyContext>) -> PyResult<Option<PyState>> {
        let entity_id: EntityId = entity_id
            .parse()
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(format!("{}", e)))?;

        let ctx = context.map(|c| c.into_inner()).unwrap_or_default();
        Ok(self.inner.remove(&entity_id, ctx).map(PyState::from_inner))
    }

    /// Get the total number of entities
    fn entity_count(&self) -> usize {
        self.inner.entity_count()
    }

    fn __repr__(&self) -> String {
        format!("StateMachine(entities={})", self.inner.entity_count())
    }

    fn __len__(&self) -> usize {
        self.inner.entity_count()
    }
}

impl PyStateMachine {
    pub fn new(event_bus: Arc<EventBus>) -> Self {
        Self {
            inner: Arc::new(StateMachine::new(event_bus)),
        }
    }

    pub fn from_arc(inner: Arc<StateMachine>) -> Self {
        Self { inner }
    }

    pub fn inner(&self) -> &Arc<StateMachine> {
        &self.inner
    }
}
