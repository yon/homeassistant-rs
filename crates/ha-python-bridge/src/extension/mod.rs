//! Mode 1: Python extension module
//!
//! Exposes Rust components as Python classes via #[pyclass].

// PyO3's #[pymethods] macro triggers false positive clippy warnings
#![allow(clippy::useless_conversion)]

mod py_area_registry;
mod py_automation;
mod py_config_entries;
mod py_device_registry;
mod py_entity_registry;
mod py_event_bus;
mod py_floor_registry;
mod py_home_assistant;
mod py_label_registry;
mod py_service_registry;
mod py_state_machine;
mod py_storage;
mod py_template;
mod py_types;

pub use py_area_registry::{PyAreaEntry, PyAreaRegistry};
pub use py_automation::{PyAutomation, PyAutomationManager};
pub use py_config_entries::{PyConfigEntries, PyConfigEntry};
pub use py_device_registry::{PyDeviceEntry, PyDeviceRegistry};
pub use py_entity_registry::{PyEntityEntry, PyEntityRegistry};
pub use py_event_bus::{PyEventBus, PyUnsubscribe};
pub use py_floor_registry::{PyFloorEntry, PyFloorRegistry};
pub use py_home_assistant::PyHomeAssistant;
pub use py_label_registry::{PyLabelEntry, PyLabelRegistry};
pub use py_service_registry::PyServiceRegistry;
pub use py_state_machine::PyStateMachine;
pub use py_storage::PyStorage;
pub use py_template::{PyTemplate, PyTemplateEngine};
pub use py_types::{PyContext, PyEntityId, PyEvent, PyState};
