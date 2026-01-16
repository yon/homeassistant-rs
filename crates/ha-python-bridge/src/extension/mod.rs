//! Mode 1: Python extension module
//!
//! Exposes Rust components as Python classes via #[pyclass].

// PyO3's #[pymethods] macro triggers false positive clippy warnings
#![allow(clippy::useless_conversion)]

mod py_event_bus;
mod py_home_assistant;
mod py_service_registry;
mod py_state_machine;
mod py_types;

pub use py_event_bus::PyEventBus;
pub use py_home_assistant::PyHomeAssistant;
pub use py_service_registry::PyServiceRegistry;
pub use py_state_machine::PyStateMachine;
pub use py_types::{PyContext, PyEntityId, PyEvent, PyState};
