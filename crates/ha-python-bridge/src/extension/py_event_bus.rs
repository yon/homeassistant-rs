//! Python wrapper for EventBus

use super::py_types::{py_to_json, PyContext};
use ha_core::Event;
use ha_event_bus::EventBus;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::sync::Arc;

/// Python wrapper for EventBus
#[pyclass(name = "EventBus")]
pub struct PyEventBus {
    inner: Arc<EventBus>,
}

#[pymethods]
impl PyEventBus {
    #[new]
    fn new() -> Self {
        Self {
            inner: Arc::new(EventBus::new()),
        }
    }

    /// Fire an event
    ///
    /// Args:
    ///     event_type: The type of event to fire
    ///     event_data: Optional event data as a dictionary
    ///     context: Optional context for the event
    #[pyo3(signature = (event_type, event_data=None, context=None))]
    fn fire(
        &self,
        event_type: &str,
        event_data: Option<&Bound<'_, PyDict>>,
        context: Option<PyContext>,
    ) -> PyResult<()> {
        let data = match event_data {
            Some(dict) => py_to_json(dict.as_any())?,
            None => serde_json::Value::Object(Default::default()),
        };

        let ctx = context.map(|c| c.into_inner()).unwrap_or_default();
        let event = Event::new(event_type, data, ctx);
        self.inner.fire(event);

        Ok(())
    }

    /// Get the number of active event type subscriptions
    fn listener_count(&self) -> usize {
        self.inner.listener_count()
    }

    fn __repr__(&self) -> String {
        format!("EventBus(listeners={})", self.inner.listener_count())
    }
}

impl PyEventBus {
    pub fn from_arc(inner: Arc<EventBus>) -> Self {
        Self { inner }
    }

    pub fn inner(&self) -> &Arc<EventBus> {
        &self.inner
    }
}
