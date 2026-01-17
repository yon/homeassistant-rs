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
//! ```rust,ignore
//! use ha_python_bridge::fallback::FallbackBridge;
//!
//! let bridge = FallbackBridge::new(None)?;
//! bridge.load_integration("hue")?;
//! ```

#[cfg(feature = "extension")]
mod extension;

#[cfg(feature = "fallback")]
pub mod fallback;

#[cfg(feature = "extension")]
use pyo3::prelude::*;

// Re-export fallback types for convenience
#[cfg(feature = "fallback")]
pub use fallback::{FallbackBridge, FallbackError, FallbackResult};

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

    // Storage
    m.add_class::<extension::PyStorage>()?;

    // Registries
    m.add_class::<extension::PyEntityEntry>()?;
    m.add_class::<extension::PyEntityRegistry>()?;
    m.add_class::<extension::PyDeviceEntry>()?;
    m.add_class::<extension::PyDeviceRegistry>()?;
    m.add_class::<extension::PyAreaEntry>()?;
    m.add_class::<extension::PyAreaRegistry>()?;
    m.add_class::<extension::PyFloorEntry>()?;
    m.add_class::<extension::PyFloorRegistry>()?;
    m.add_class::<extension::PyLabelEntry>()?;
    m.add_class::<extension::PyLabelRegistry>()?;

    // Template
    m.add_class::<extension::PyTemplate>()?;
    m.add_class::<extension::PyTemplateEngine>()?;

    // Config Entries
    m.add_class::<extension::PyConfigEntry>()?;
    m.add_class::<extension::PyConfigEntries>()?;

    // Automation
    m.add_class::<extension::PyAutomation>()?;
    m.add_class::<extension::PyAutomationManager>()?;

    // Condition Evaluation
    m.add_class::<extension::PyConditionEvaluator>()?;
    m.add_class::<extension::PyEvalContext>()?;

    // Trigger Evaluation
    m.add_class::<extension::PyTriggerEvaluator>()?;
    m.add_class::<extension::PyTriggerData>()?;
    m.add_class::<extension::PyTriggerEvalContext>()?;

    // HomeAssistant wrapper
    m.add_class::<extension::PyHomeAssistant>()?;

    // Helper types
    m.add_class::<extension::PyUnsubscribe>()?;

    Ok(())
}
