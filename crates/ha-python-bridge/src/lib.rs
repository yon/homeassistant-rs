//! PyO3 bidirectional bridge for Home Assistant
//!
//! This crate provides two deployment modes:
//!
//! ## Mode 1: Extension (feature = "extension")
//! Build as a Python extension module that can be imported into existing Python HA.
//! ```python
//! from ha_core_rs import StateMachine, EventBus, ServiceRegistry
//! ```
//!
//! ## Mode 2: Fallback (feature = "fallback")
//! Embed Python interpreter to delegate unimplemented components to Python HA.

#[cfg(feature = "extension")]
mod extension;

#[cfg(feature = "fallback")]
mod fallback;

#[cfg(feature = "extension")]
use pyo3::prelude::*;

/// Python module initialization for Mode 1 (extension)
#[cfg(feature = "extension")]
#[pymodule]
fn ha_core_rs(m: &Bound<'_, PyModule>) -> PyResult<()> {
    // Core types
    m.add_class::<extension::PyEntityId>()?;
    m.add_class::<extension::PyContext>()?;
    m.add_class::<extension::PyState>()?;
    m.add_class::<extension::PyEvent>()?;

    // Core components
    m.add_class::<extension::PyEventBus>()?;
    m.add_class::<extension::PyStateMachine>()?;
    m.add_class::<extension::PyServiceRegistry>()?;

    // HomeAssistant wrapper
    m.add_class::<extension::PyHomeAssistant>()?;

    Ok(())
}
