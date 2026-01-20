//! BusWrapper - wraps Rust EventBus for Python access

use super::util::py_to_json;
use ha_core::{Context, Event};
use ha_event_bus::EventBus;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::sync::Arc;

/// Python wrapper for the Rust EventBus
#[pyclass(name = "BusWrapper")]
pub struct BusWrapper {
    bus: Arc<EventBus>,
}

impl BusWrapper {
    pub fn new(bus: Arc<EventBus>) -> Self {
        Self { bus }
    }
}

#[pymethods]
impl BusWrapper {
    /// Fire an event
    #[pyo3(signature = (event_type, event_data=None, _origin=None, _context=None))]
    fn async_fire<'py>(
        &self,
        py: Python<'py>,
        event_type: &str,
        event_data: Option<&Bound<'py, PyDict>>,
        _origin: Option<&str>,
        _context: Option<PyObject>,
    ) -> PyResult<Bound<'py, PyAny>> {
        // Convert event data to JSON
        let data: serde_json::Value = match event_data {
            Some(dict) => py_to_json(dict.as_any()),
            None => serde_json::Value::Object(serde_json::Map::new()),
        };

        // Fire the event via Rust EventBus
        let context = Context::new();
        let event = Event::new(event_type, data, context);
        self.bus.fire(event);

        tracing::debug!(event_type = %event_type, "Fired event via Rust EventBus");

        // Return completed future
        let asyncio = py.import_bound("asyncio")?;
        let future = asyncio.call_method0("Future")?;
        future.call_method1("set_result", (py.None(),))?;
        Ok(future)
    }

    /// Listen for events (placeholder - returns a dummy unsub function)
    #[pyo3(signature = (event_type, _listener, event_filter=None))]
    fn async_listen<'py>(
        &self,
        py: Python<'py>,
        event_type: &str,
        _listener: PyObject,
        event_filter: Option<PyObject>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let _ = event_filter; // Silence unused warning
        tracing::debug!(event_type = %event_type, "Event listener registered (stub)");

        // Return a dummy unsubscribe function
        let code = "lambda: None";
        let unsub = py.eval_bound(code, None, None)?;

        let asyncio = py.import_bound("asyncio")?;
        let future = asyncio.call_method0("Future")?;
        future.call_method1("set_result", (unsub,))?;
        Ok(future)
    }

    /// Listen for an event once (placeholder - returns a dummy unsub function)
    #[pyo3(signature = (event_type, _listener, event_filter=None))]
    fn async_listen_once<'py>(
        &self,
        py: Python<'py>,
        event_type: &str,
        _listener: PyObject,
        event_filter: Option<PyObject>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let _ = event_filter; // Silence unused warning
        tracing::debug!(event_type = %event_type, "One-time event listener registered (stub)");

        // Return a dummy unsubscribe function
        let code = "lambda: None";
        let unsub = py.eval_bound(code, None, None)?;

        let asyncio = py.import_bound("asyncio")?;
        let future = asyncio.call_method0("Future")?;
        future.call_method1("set_result", (unsub,))?;
        Ok(future)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bus_wrapper() {
        pyo3::prepare_freethreaded_python();

        Python::with_gil(|py| {
            let bus = Arc::new(EventBus::new());
            let wrapper = BusWrapper::new(bus);

            let data = PyDict::new_bound(py);
            data.set_item("test", "value").unwrap();

            // Should not panic
            let _ = wrapper.async_fire(py, "test_event", Some(&data), None, None);
        });
    }
}
