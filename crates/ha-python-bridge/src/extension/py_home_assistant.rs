//! Python wrapper for the HomeAssistant struct

use super::{PyEventBus, PyServiceRegistry, PyStateMachine};
use ha_event_bus::EventBus;
use ha_service_registry::ServiceRegistry;
use ha_state_machine::StateMachine;
use pyo3::prelude::*;
use std::sync::Arc;

/// Python wrapper for the central HomeAssistant instance
///
/// This provides access to all core components:
/// - bus: The event bus for pub/sub
/// - states: The state machine for entity states
/// - services: The service registry
#[pyclass(name = "HomeAssistant")]
pub struct PyHomeAssistant {
    bus: Arc<EventBus>,
    states: Arc<StateMachine>,
    services: Arc<ServiceRegistry>,
}

#[pymethods]
impl PyHomeAssistant {
    #[new]
    fn new() -> Self {
        let bus = Arc::new(EventBus::new());
        let states = Arc::new(StateMachine::new(bus.clone()));
        let services = Arc::new(ServiceRegistry::new());

        Self {
            bus,
            states,
            services,
        }
    }

    /// Get the event bus
    #[getter]
    fn bus(&self) -> PyEventBus {
        PyEventBus::from_arc(self.bus.clone())
    }

    /// Get the state machine
    #[getter]
    fn states(&self) -> PyStateMachine {
        PyStateMachine::from_arc(self.states.clone())
    }

    /// Get the service registry
    #[getter]
    fn services(&self) -> PyServiceRegistry {
        PyServiceRegistry::from_arc(self.services.clone())
    }

    fn __repr__(&self) -> String {
        format!(
            "HomeAssistant(entities={}, services={})",
            self.states.entity_count(),
            self.services.service_count()
        )
    }
}

impl PyHomeAssistant {
    pub fn bus_arc(&self) -> &Arc<EventBus> {
        &self.bus
    }

    pub fn states_arc(&self) -> &Arc<StateMachine> {
        &self.states
    }

    pub fn services_arc(&self) -> &Arc<ServiceRegistry> {
        &self.services
    }
}
