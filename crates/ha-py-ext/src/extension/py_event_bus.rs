//! Python wrapper for EventBus using synchronous callbacks

use std::sync::Arc;

use ha_core::{Event, State};
use ha_event_bus::{EventBus, ListenerId, SyncCallback};
use pyo3::prelude::*;
use pyo3::types::PyDict;

use super::py_types::{json_to_py, py_to_json, PyContext, PyState};

/// Event type constant for state_changed
const STATE_CHANGED: &str = "state_changed";

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

    /// Fire an event on the bus.
    #[pyo3(signature = (event_type, event_data=None, origin=None, context=None, time_fired=None))]
    fn async_fire(
        &self,
        event_type: &str,
        event_data: Option<&Bound<'_, PyDict>>,
        #[allow(unused_variables)] origin: Option<&Bound<'_, PyAny>>,
        context: Option<PyContext>,
        #[allow(unused_variables)] time_fired: Option<f64>,
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

    /// Register a synchronous listener for an event type.
    ///
    /// The callback fires inline during fire() on the calling thread.
    /// Returns a callable that removes the listener when called.
    #[pyo3(signature = (event_type, listener, run_immediately=false, event_filter=None))]
    fn async_listen(
        &self,
        py: Python<'_>,
        event_type: &str,
        listener: PyObject,
        #[allow(unused_variables)] run_immediately: bool,
        event_filter: Option<PyObject>,
    ) -> PyResult<PyObject> {
        if !listener.bind(py).is_callable() {
            return Err(PyErr::new::<pyo3::exceptions::PyTypeError, _>(
                "listener must be callable",
            ));
        }

        let callback = listener.clone_ref(py);
        let filter = event_filter.map(|f| f.clone_ref(py));
        let event_type_owned = event_type.to_string();

        let sync_callback: SyncCallback = Arc::new(move |event: &Event<serde_json::Value>| {
            Python::with_gil(|py| {
                call_python_listener(py, event, &callback, filter.as_ref(), &event_type_owned);
            });
        });

        let event_type_for_bus = if event_type == "*" {
            ha_core::EventType::match_all()
        } else {
            ha_core::EventType::from(event_type)
        };

        let listener_id = self.inner.listen_sync(event_type_for_bus, sync_callback);

        // Return remove_listener callable
        let bus = self.inner.clone();
        let unsubscribe = PyUnsubscribe { bus, listener_id };
        Ok(unsubscribe.into_py(py))
    }

    /// Listen for a single event, then automatically unsubscribe.
    #[pyo3(signature = (event_type, listener, run_immediately=false))]
    fn async_listen_once(
        &self,
        py: Python<'_>,
        event_type: &str,
        listener: PyObject,
        #[allow(unused_variables)] run_immediately: bool,
    ) -> PyResult<PyObject> {
        if !listener.bind(py).is_callable() {
            return Err(PyErr::new::<pyo3::exceptions::PyTypeError, _>(
                "listener must be callable",
            ));
        }

        let callback = listener.clone_ref(py);
        let fired = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let fired_clone = fired.clone();
        // We need the listener_id to remove ourselves, but we don't have it yet.
        // Use a shared cell to store it after registration.
        let stored_id = Arc::new(std::sync::Mutex::new(None::<ListenerId>));
        let stored_id_clone = stored_id.clone();
        let bus = self.inner.clone();

        let sync_callback: SyncCallback = Arc::new(move |event: &Event<serde_json::Value>| {
            // Only fire once
            if fired_clone.swap(true, std::sync::atomic::Ordering::SeqCst) {
                return;
            }
            // Remove ourselves
            if let Ok(guard) = stored_id_clone.lock() {
                if let Some(id) = *guard {
                    bus.remove_sync_listener(id);
                }
            }
            Python::with_gil(|py| {
                call_python_listener(py, event, &callback, None, "");
            });
        });

        let event_type_for_bus = if event_type == "*" {
            ha_core::EventType::match_all()
        } else {
            ha_core::EventType::from(event_type)
        };

        let listener_id = self.inner.listen_sync(event_type_for_bus, sync_callback);
        // Store the listener_id so the callback can remove itself
        if let Ok(mut guard) = stored_id.lock() {
            *guard = Some(listener_id);
        }

        // Return remove_listener callable
        let bus = self.inner.clone();
        let unsubscribe = PyUnsubscribe { bus, listener_id };
        Ok(unsubscribe.into_py(py))
    }

    /// Get listener counts per event type
    fn async_listeners(&self) -> std::collections::HashMap<String, usize> {
        let mut result = std::collections::HashMap::new();
        for (event_type, count) in self.inner.sync_listeners_iter() {
            *result.entry(event_type.to_string()).or_insert(0) += count;
        }
        result
    }

    /// Aliases for API compatibility
    #[pyo3(signature = (event_type, event_data=None, origin=None, context=None, time_fired=None))]
    fn fire(
        &self,
        event_type: &str,
        event_data: Option<&Bound<'_, PyDict>>,
        origin: Option<&Bound<'_, PyAny>>,
        context: Option<PyContext>,
        time_fired: Option<f64>,
    ) -> PyResult<()> {
        self.async_fire(event_type, event_data, origin, context, time_fired)
    }

    #[pyo3(signature = (event_type, listener, run_immediately=false, event_filter=None))]
    fn listen(
        &self,
        py: Python<'_>,
        event_type: &str,
        listener: PyObject,
        run_immediately: bool,
        event_filter: Option<PyObject>,
    ) -> PyResult<PyObject> {
        self.async_listen(py, event_type, listener, run_immediately, event_filter)
    }

    #[pyo3(signature = (event_type, listener, run_immediately=false))]
    fn listen_once(
        &self,
        py: Python<'_>,
        event_type: &str,
        listener: PyObject,
        run_immediately: bool,
    ) -> PyResult<PyObject> {
        self.async_listen_once(py, event_type, listener, run_immediately)
    }

    fn listeners(&self) -> std::collections::HashMap<String, usize> {
        self.async_listeners()
    }

    fn __repr__(&self) -> String {
        format!(
            "EventBus(sync_listeners={})",
            self.inner.sync_listener_count()
        )
    }

    fn __len__(&self) -> usize {
        self.inner.sync_listener_count()
    }
}

/// Convert a Rust Event to a Python event object and call the listener.
fn call_python_listener(
    py: Python<'_>,
    event: &Event<serde_json::Value>,
    callback: &PyObject,
    filter: Option<&PyObject>,
    _event_type_hint: &str,
) {
    // Convert event data to Python
    let data_dict = match convert_event_data(py, event) {
        Ok(d) => d,
        Err(e) => {
            tracing::error!("Error converting event data: {}", e);
            return;
        }
    };

    // Apply event filter if present
    if let Some(filter_fn) = filter {
        match filter_fn.call1(py, (&data_dict,)) {
            Ok(result) => {
                if !result.is_truthy(py).unwrap_or(false) {
                    return; // Filter rejected
                }
            }
            Err(_) => return, // Filter error = skip
        }
    }

    // Create the Python event object
    let time_fired_timestamp = event.time_fired.timestamp() as f64
        + (event.time_fired.timestamp_subsec_nanos() as f64 / 1_000_000_000.0);

    let py_event = PyBusEvent {
        event_type: event.event_type.to_string(),
        data: data_dict.unbind(),
        context: PyContext::from_inner(event.context.clone()),
        time_fired_timestamp,
    };

    // Call the callback
    if let Err(e) = callback.call1(py, (py_event,)) {
        tracing::error!("Error in event listener: {}", e);
    }
}

/// Convert event data to a Python dict, with special handling for STATE_CHANGED.
fn convert_event_data<'py>(
    py: Python<'py>,
    event: &Event<serde_json::Value>,
) -> PyResult<Bound<'py, PyDict>> {
    let dict = PyDict::new_bound(py);

    if event.event_type.as_str() == STATE_CHANGED {
        // Special handling: convert old_state/new_state to PyState objects
        if let serde_json::Value::Object(ref map) = event.data {
            // entity_id
            if let Some(eid) = map.get("entity_id") {
                dict.set_item("entity_id", json_to_py(py, eid)?)?;
            }

            // old_state → PyState or None
            if let Some(old_state_val) = map.get("old_state") {
                if old_state_val.is_null() {
                    dict.set_item("old_state", py.None())?;
                } else if let Ok(state) = serde_json::from_value::<State>(old_state_val.clone()) {
                    let py_state = Py::new(py, PyState::from_inner(state))?;
                    dict.set_item("old_state", py_state)?;
                } else {
                    dict.set_item("old_state", json_to_py(py, old_state_val)?)?;
                }
            } else {
                dict.set_item("old_state", py.None())?;
            }

            // new_state → PyState or None
            if let Some(new_state_val) = map.get("new_state") {
                if new_state_val.is_null() {
                    dict.set_item("new_state", py.None())?;
                } else if let Ok(state) = serde_json::from_value::<State>(new_state_val.clone()) {
                    let py_state = Py::new(py, PyState::from_inner(state))?;
                    dict.set_item("new_state", py_state)?;
                } else {
                    dict.set_item("new_state", json_to_py(py, new_state_val)?)?;
                }
            } else {
                dict.set_item("new_state", py.None())?;
            }
        }
    } else {
        // Generic: convert JSON object to Python dict
        if let serde_json::Value::Object(ref map) = event.data {
            for (k, v) in map {
                dict.set_item(k, json_to_py(py, v)?)?;
            }
        }
    }

    Ok(dict)
}

/// Python event object passed to listeners.
/// Provides the same interface as HA's Event class.
#[pyclass(name = "Event")]
pub struct PyBusEvent {
    #[pyo3(get)]
    event_type: String,
    data: Py<PyDict>,
    #[pyo3(get)]
    context: PyContext,
    #[pyo3(get)]
    time_fired_timestamp: f64,
}

#[pymethods]
impl PyBusEvent {
    #[getter]
    fn data(&self, py: Python<'_>) -> PyObject {
        self.data.clone_ref(py).into_any()
    }

    #[getter]
    fn origin(&self) -> &str {
        "local"
    }

    #[getter]
    fn time_fired(&self, py: Python<'_>) -> PyResult<PyObject> {
        let dt_module = py.import_bound("datetime")?;
        let dt_class = dt_module.getattr("datetime")?;
        let tz = dt_module.getattr("timezone")?.getattr("utc")?;
        let dt = dt_class.call_method1("fromtimestamp", (self.time_fired_timestamp, tz))?;
        Ok(dt.unbind())
    }

    #[classmethod]
    fn __class_getitem__(cls: &Bound<'_, pyo3::types::PyType>, _item: PyObject) -> PyObject {
        cls.clone().into_any().unbind()
    }

    fn __repr__(&self) -> String {
        format!("<Event {}>", self.event_type)
    }
}

/// Callable that removes a listener when invoked.
#[pyclass(name = "Unsubscribe")]
pub struct PyUnsubscribe {
    bus: Arc<EventBus>,
    listener_id: ListenerId,
}

#[pymethods]
impl PyUnsubscribe {
    fn __call__(&self) -> PyResult<()> {
        self.bus.remove_sync_listener(self.listener_id);
        Ok(())
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
